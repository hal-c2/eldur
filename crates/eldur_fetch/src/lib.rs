//! A page given by an `http(s)` URL, and what it references, fetched for
//! [`eldur_convert`].
//!
//! [`HttpResources`] is an [`eldur_convert::Resources`] that resolves each
//! reference against the URL of the document or style sheet holding it and
//! fetches it over HTTP(S) (or decodes it, for `data:`). It is bounded: a
//! timeout per request, a size cap per resource and in total, and a cap on
//! the number of fetches. A reference that fails or is refused is a warning
//! [`Diagnostic`] naming its URL ([`HttpResources::diagnostics`]), never an
//! error; only the page itself failing is one ([`HttpResources::fetch_page`]).
//!
//! [`MixedResources`] puts it beside a local resolver, for a local page whose
//! references reach the web, and [`convert_url`] is the whole round.
//!
//! The public API in this file is the contract other programs are written
//! against. Add to it freely; change or remove items only with the lead's
//! agreement.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use eldur_convert::{Diagnostic, Input, Options, Output, Resource, Resources, Severity};
use ureq::tls::{RootCerts, TlsConfig};
use url::Url;

/// [`Diagnostic::stage`] of what fetching reports.
pub const STAGE: &str = "fetch";

/// How much a page may fetch. [`Limits::default`] suits a single web page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most one request may take, from connecting to the body's end.
    pub timeout: Duration,
    /// The largest body one resource may have, in bytes.
    pub max_resource_bytes: u64,
    /// The most bytes all of a page's fetches together may bring, decoded
    /// `data:` URLs included.
    pub max_total_bytes: u64,
    /// The most HTTP requests a page may make, the page's own and each
    /// redirect hop included. `data:` URLs are decoded, not fetched, and do
    /// not count.
    pub max_fetches: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_resource_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_fetches: 256,
        }
    }
}

/// Why the page itself could not be fetched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The page's address is not an absolute `http` or `https` URL.
    #[error("{url} is not an http(s) URL")]
    NotHttp { url: String },
    /// The request failed, was refused by a [`Limits`] bound, or the server
    /// answered with an error status.
    #[error("failed to fetch {url}: {reason}")]
    Failed { url: String, reason: String },
}

/// Whether `input` is an `http://` or `https://` URL (the scheme in any
/// case), i.e. a page [`HttpResources::fetch_page`] takes rather than a file
/// path. The `//` must be written: `https:report.html` can name a file.
#[must_use]
pub fn is_http_url(input: &str) -> bool {
    let input = input.trim();
    let spelled = ["http://", "https://"].iter().any(|prefix| {
        input
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    });
    spelled && Url::parse(input).is_ok()
}

/// A reference that was not served, kept to report it.
#[derive(Debug, Clone)]
struct Failure {
    url: String,
    reason: String,
}

/// The most redirects one request follows, as browsers allow (Fetch's
/// limit of 20 is for a navigation; ureq's default is 10).
const MAX_REDIRECTS: usize = 10;

/// An [`eldur_convert::Resources`] over HTTP(S) and `data:` URLs, for a page
/// given by URL. See the crate documentation.
///
/// A resource is fetched once per URL (without its fragment); asking again
/// returns the first answer.
///
/// Credentials in the page's URL (`https://user:secret@host/`) are sent to
/// that origin only and appear in no URL it reports: not in
/// [`Input::url`], a [`Resource::resolved`], a diagnostic or an error.
pub struct HttpResources {
    agent: ureq::Agent,
    limits: Limits,
    fetches: usize,
    total_bytes: u64,
    /// By request URL: the resource, or why it was not served.
    answers: HashMap<String, Result<Resource, String>>,
    failures: Vec<Failure>,
    fetched: Vec<String>,
    /// The page's origin and the user name and password its URL gave.
    credentials: Option<(url::Origin, String, Option<String>)>,
}

impl std::fmt::Debug for HttpResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResources")
            .field("limits", &self.limits)
            .field("fetches", &self.fetches)
            .field("total_bytes", &self.total_bytes)
            .field("fetched", &self.fetched)
            .field("failures", &self.failures)
            .finish_non_exhaustive()
    }
}

impl Default for HttpResources {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpResources {
    /// With [`Limits::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(Limits::default())
    }

    /// With the given bounds.
    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        // The platform's verifier and roots rather than ureq's bundled
        // webpki-roots (CDLA-Permissive-2.0, not a licence this project
        // takes); ring as rustls's provider, passed here so that no
        // process-wide default has to be installed.
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        // Redirects are followed here, not by ureq, so that each hop is a
        // fetch the limits count.
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(limits.timeout))
            .max_redirects(0)
            .tls_config(tls)
            .user_agent(concat!("eldur/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent();
        Self {
            agent,
            limits,
            fetches: 0,
            total_bytes: 0,
            answers: HashMap::new(),
            failures: Vec::new(),
            fetched: Vec::new(),
            credentials: None,
        }
    }

    /// The bounds in force.
    #[must_use]
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Fetches the page at `url`. Its [`Input::url`] is where the page was
    /// found after redirects, as a browser has it, so its references resolve
    /// there. The bytes are read as UTF-8, invalid sequences replaced. The
    /// fetch counts against the same [`Limits`] as the page's references.
    /// Each page starts afresh: an earlier page's credentials and the
    /// answers fetched with them do not carry over to it.
    pub fn fetch_page(&mut self, url: &str) -> Result<Input, FetchError> {
        let mut target = Url::parse(url.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| FetchError::NotHttp { url: redacted(url) })?;
        target.set_fragment(None);
        self.credentials = None;
        self.answers.clear();
        if !target.username().is_empty() || target.password().is_some() {
            self.credentials = Some((
                target.origin(),
                target.username().to_owned(),
                target.password().map(str::to_owned),
            ));
            // Both are set on a URL with a host, which an http(s) URL has.
            let _ = target.set_username("");
            let _ = target.set_password(None);
        }
        match self.fetch(&target) {
            Ok(resource) => {
                self.answers
                    .insert(target.to_string(), Ok(resource.clone()));
                Ok(Input {
                    html: String::from_utf8_lossy(&resource.bytes).into_owned(),
                    url: resource.resolved,
                })
            }
            Err(reason) => Err(FetchError::Failed {
                url: target.to_string(),
                reason,
            }),
        }
    }

    /// A warning for every lookup that was not served, in the order they
    /// were asked for, naming its URL and why. A lookup tried against
    /// several bases ([`Resources::read_any`]) that one base served is left
    /// out.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.failures
            .iter()
            .map(|f| Diagnostic {
                severity: Severity::Warning,
                stage: STAGE,
                source: f.url.clone(),
                line: None,
                message: format!("not fetched: {}", f.reason),
            })
            .collect()
    }

    /// Every URL fetched successfully, the page included, in order, as
    /// requested (before redirects).
    #[must_use]
    pub fn fetched(&self) -> &[String] {
        &self.fetched
    }

    /// The number of HTTP requests made so far, each redirect hop one.
    #[must_use]
    pub fn fetch_count(&self) -> usize {
        self.fetches
    }

    /// The bytes all fetches have brought so far, those of a body that broke
    /// off included, and the bytes of decoded `data:` URLs.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Reports `url` as not served, once however often it is asked for.
    fn fail(&mut self, url: String, reason: String) {
        if !self.failures.iter().any(|f| f.url == url) {
            self.failures.push(Failure { url, reason });
        }
    }

    /// Room for one more request, which it takes, and for some bytes.
    fn take_request(&mut self) -> Result<(), String> {
        if self.fetches >= self.limits.max_fetches {
            return Err(format!(
                "refused: the page has made {} requests, the most allowed",
                self.limits.max_fetches
            ));
        }
        self.room()?;
        self.fetches += 1;
        Ok(())
    }

    /// The bytes one more resource may have: within its own cap and what is
    /// left of the page's.
    fn room(&self) -> Result<u64, String> {
        let room = self.limits.max_total_bytes.saturating_sub(self.total_bytes);
        if room == 0 {
            return Err(format!(
                "refused: the page has fetched {} bytes, the most allowed",
                self.limits.max_total_bytes
            ));
        }
        Ok(self.limits.max_resource_bytes.min(room))
    }

    fn too_large(&self, cap: u64) -> String {
        if cap < self.limits.max_resource_bytes {
            format!(
                "refused: it would take the page past {} bytes in all",
                self.limits.max_total_bytes
            )
        } else {
            format!("refused: it is larger than {cap} bytes")
        }
    }

    /// `at` as requested: with the page's credentials when it is the page's
    /// origin and gives none of its own.
    fn request_url(&self, at: &Url) -> Url {
        let mut request = at.clone();
        if let Some((origin, user, password)) = &self.credentials
            && at.origin() == *origin
            && at.username().is_empty()
            && at.password().is_none()
        {
            let _ = request.set_username(user);
            let _ = request.set_password(password.as_deref());
        }
        request
    }

    fn fetch(&mut self, target: &Url) -> Result<Resource, String> {
        let mut at = target.clone();
        for _ in 0..=MAX_REDIRECTS {
            self.take_request()?;
            let mut response = self
                .agent
                .get(self.request_url(&at).as_str())
                .call()
                .map_err(|err| describe(&err))?;
            let location = response
                .status()
                .is_redirection()
                .then(|| response.headers().get(ureq::http::header::LOCATION))
                .flatten();
            if let Some(location) = location {
                let location = location
                    .to_str()
                    .map_err(|_| "a redirect whose Location is not text".to_owned())?;
                let mut next = at
                    .join(location)
                    .map_err(|err| format!("a redirect to `{location}`: {err}"))?;
                if !matches!(next.scheme(), "http" | "https") {
                    return Err(format!(
                        "refused: a redirect to {}:, not http or https",
                        next.scheme()
                    ));
                }
                next.set_fragment(None);
                at = next;
                continue;
            }
            let cap = self.room()?;
            // A body that says it is too large is refused unread, so it costs
            // the page nothing; one that says nothing or lies is metered.
            if response
                .body()
                .content_length()
                .is_some_and(|length| length > cap)
            {
                return Err(self.too_large(cap));
            }
            let bytes = self.read_body(response.body_mut())?;
            self.fetched.push(target.to_string());
            return Ok(Resource {
                resolved: at.to_string(),
                bytes,
            });
        }
        Err(format!("refused: more than {MAX_REDIRECTS} redirects"))
    }

    /// The body, each byte counted toward the page's total as it arrives,
    /// so a body that breaks off is charged what it brought.
    fn read_body(&mut self, body: &mut ureq::Body) -> Result<Vec<u8>, String> {
        use std::io::Read as _;
        let cap = self.room()?;
        // ureq's own limit is turned off: `cap` is enforced here.
        let mut reader = body.with_config().limit(u64::MAX).reader();
        let mut bytes = Vec::new();
        let mut chunk = vec![0; 64 * 1024];
        loop {
            // One byte past the cap shows that the body is larger.
            let left = cap.saturating_add(1) - bytes.len() as u64;
            let want = usize::try_from(left).map_or(chunk.len(), |left| left.min(chunk.len()));
            let n = match reader.read(&mut chunk[..want]) {
                Ok(0) => return Ok(bytes),
                Ok(n) => n,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(describe_io(&err)),
            };
            self.total_bytes += n as u64;
            bytes.extend_from_slice(&chunk[..n]);
            if bytes.len() as u64 > cap {
                return Err(self.too_large(cap));
            }
        }
    }

    fn decode_data(&mut self, url: &str) -> Result<Resource, String> {
        let data = data_url::DataUrl::process(url).map_err(|_| "a malformed data: URL")?;
        let (bytes, _) = data
            .decode_to_vec()
            .map_err(|_| "a data: URL with invalid base64")?;
        let cap = self.room()?;
        if bytes.len() as u64 > cap {
            return Err(self.too_large(cap));
        }
        self.total_bytes += bytes.len() as u64;
        Ok(Resource {
            resolved: url.to_owned(),
            bytes,
        })
    }
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(code) => format!("HTTP status {code}"),
        ureq::Error::Timeout(_) => "timed out".to_owned(),
        other => other.to_string(),
    }
}

fn describe_io(err: &std::io::Error) -> String {
    match err
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>())
    {
        Some(inner) => describe(inner),
        None if err.kind() == std::io::ErrorKind::TimedOut => "timed out".to_owned(),
        None => err.to_string(),
    }
}

/// `url` as it may be shown: without user name, password, query and
/// fragment, which can carry secrets. A string that is not a URL is shown
/// as it is.
#[must_use]
pub fn redacted(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url.trim()) else {
        return url.to_owned();
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

/// `text` with every `http(s)` URL in it [`redacted`], the scheme in any
/// case, and every reference quoted in backticks without its query and
/// fragment: a diagnostic's message can quote a reference that carries a
/// signed query, absolute or relative to the page. A URL ends at whitespace
/// or at the quote, bracket or backtick around it.
#[must_use]
pub fn redacted_in(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        // ASCII lowering keeps every byte where it was.
        let lower = rest.to_ascii_lowercase();
        let Some(at) = ["http://", "https://"]
            .iter()
            .filter_map(|scheme| lower.find(scheme))
            .min()
        else {
            break;
        };
        out.push_str(&rest[..at]);
        let url = &rest[at..];
        let end = url
            .find(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | ')' | '>' | '<'))
            .unwrap_or(url.len());
        out.push_str(&redacted(&url[..end]));
        rest = &url[end..];
    }
    out.push_str(rest);
    quoted_redacted(&out)
}

/// `text` with each backtick-quoted reference that is not an absolute URL
/// (those are [`redacted`] already) cut at its query or fragment, and one
/// without a scheme (`//user:pw@host/x`) also without its user.
fn quoted_redacted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut parts = text.split('`');
    out.push_str(parts.next().unwrap_or(""));
    let mut inside = true;
    for part in parts {
        out.push('`');
        if inside && !part.contains("://") {
            if let Some(rest) = part.strip_prefix("//") {
                let shown = redacted(&format!("http://{rest}"));
                out.push_str(shown.strip_prefix("http:").unwrap_or(&shown));
            } else {
                out.push_str(part.split(['?', '#']).next().unwrap_or(part));
            }
        } else {
            out.push_str(part);
        }
        inside = !inside;
    }
    out
}

/// `written` resolved against `base`, without its fragment: `None` for a
/// reference to nothing (empty, or only a fragment). Against a URL base, as
/// a browser resolves it (`http:x.css` on an `http` page is relative); an
/// absolute reference needs no base.
fn resolve(written: &str, base: &str) -> Option<Result<Url, String>> {
    if written.is_empty() || written.starts_with('#') {
        return None;
    }
    let joined = match Url::parse(base.trim()) {
        Ok(base) => base
            .join(written)
            .map_err(|err| format!("cannot be resolved against {base}: {err}")),
        Err(_) => match Url::parse(written) {
            Ok(absolute) => Ok(absolute),
            Err(url::ParseError::RelativeUrlWithoutBase) => Err(format!(
                "a relative reference in `{base}`, which is not a URL"
            )),
            Err(err) => Err(format!("not a URL: {err}")),
        },
    };
    Some(joined.map(|mut url| {
        url.set_fragment(None);
        url
    }))
}

impl Resources for HttpResources {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        let written = url.trim();
        let target = match resolve(written, base)? {
            Ok(target) => target,
            Err(reason) => {
                self.fail(written.to_owned(), reason);
                return None;
            }
        };
        let key = target.to_string();
        let answer = match self.answers.get(&key) {
            Some(answer) => answer.clone(),
            None => {
                let answer = match target.scheme() {
                    "http" | "https" => self.fetch(&target),
                    // The fragment is part of a data: URL's body, so it is
                    // decoded as written.
                    "data" => self.decode_data(written),
                    scheme => Err(format!(
                        "refused: a fetched page reads only http, https and data URLs, not {scheme}:"
                    )),
                };
                self.answers.insert(key.clone(), answer.clone());
                answer
            }
        };
        match answer {
            Ok(resource) => Some(resource),
            Err(reason) => {
                // A data: URL is named by its start: it can be the size of
                // the file it holds.
                let name = if target.scheme() == "data" {
                    written.chars().take(64).collect()
                } else {
                    key
                };
                self.fail(name, reason);
                None
            }
        }
    }

    fn read_any(&mut self, url: &str, bases: &[&str]) -> Option<Resource> {
        let reported = self.failures.len();
        let found = bases.iter().find_map(|base| self.read(url, base));
        if found.is_some() {
            self.failures.truncate(reported);
        }
        found
    }
}

/// A local page's resolver `L` with [`HttpResources`] beside it:
/// references that are `http(s)` or `data:` URLs go to `remote`, and so does
/// everything in a document or sheet that was itself fetched, so that
/// fetched content never reaches `local` (`remote` reads no `file:` URL or
/// path). The rest (paths and `file:` URLs in local files) go to `local`.
///
/// One lookup cannot keep to that: a relative `background-image` read with
/// [`Resources::read_any`], whose computed style no longer says which sheet
/// it came from, is tried against every base, the local page's too. A
/// fetched sheet can so name a file `local` serves, and its bytes are copied
/// into the output. A caller whose `local` holds what a fetched sheet must
/// not see gives such a page no fetched sheets.
#[derive(Debug, Default)]
pub struct MixedResources<L> {
    pub local: L,
    pub remote: HttpResources,
}

impl<L> MixedResources<L> {
    /// `local` beside an [`HttpResources`] with [`Limits::default`].
    #[must_use]
    pub fn new(local: L) -> Self {
        Self {
            local,
            remote: HttpResources::new(),
        }
    }
}

fn is_web_url(s: &str) -> bool {
    Url::parse(s.trim()).is_ok_and(|u| matches!(u.scheme(), "http" | "https" | "data"))
}

impl<L: Resources> Resources for MixedResources<L> {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        if is_web_url(base) || is_web_url(url) {
            self.remote.read(url, base)
        } else {
            self.local.read(url, base)
        }
    }

    fn read_any(&mut self, url: &str, bases: &[&str]) -> Option<Resource> {
        let reported = self.remote.failures.len();
        let found = bases.iter().find_map(|base| self.read(url, base));
        if found.is_some() {
            self.remote.failures.truncate(reported);
        }
        found
    }
}

/// Fetches the page at `url` and converts it, fetching what it references
/// within [`Limits::default`]. The fetch warnings follow the converter's
/// diagnostics. Only the page failing is an error.
pub fn convert_url(url: &str, options: &Options) -> Result<Output, FetchError> {
    let mut resources = HttpResources::new();
    let input = resources.fetch_page(url)?;
    let mut output = eldur_convert::convert(&input, options, &mut resources);
    output.diagnostics.extend(resources.diagnostics());
    Ok(output)
}

#[cfg(test)]
mod tests;
