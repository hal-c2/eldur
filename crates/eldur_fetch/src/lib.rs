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
use std::net::{IpAddr, Ipv4Addr};
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

/// Which machines a page and what it references may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reach {
    /// Only addresses on the public internet (the default). A page, a
    /// reference or a redirect that leads to this machine (`localhost`,
    /// `127.0.0.1`, `::1`), a private network (`10.0.0.0/8`, `192.168.0.0/16`,
    /// `fc00::/7`, …), a link-local address (`169.254.169.254`, a cloud's
    /// metadata service) or any other address that is not globally routable
    /// is refused, so a page cannot turn the conversion against the services
    /// beside it. A name is checked against every address it resolves to,
    /// and the connection is made only to the addresses that pass. Through a
    /// proxy that resolves names itself, only an address written in the URL
    /// is checked: where the name leads is the proxy's to decide.
    #[default]
    Public,
    /// Every address, this machine's and its network's included: for a page
    /// served locally (a development server) or on an intranet.
    Any,
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
    reach: Reach,
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
            .field("reach", &self.reach)
            .field("fetches", &self.fetches)
            .field("total_bytes", &self.total_bytes)
            // Counts only: a reference's URL can hold a user and a query.
            .field("fetched", &self.fetched.len())
            .field("failures", &self.failures.len())
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

    /// With the given bounds, reaching only the public internet
    /// ([`Reach::Public`]).
    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        Self::with_reach(limits, Reach::Public)
    }

    /// With the given bounds and reach.
    #[must_use]
    pub fn with_reach(limits: Limits, reach: Reach) -> Self {
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
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(limits.timeout))
            .max_redirects(0)
            .tls_config(tls)
            .user_agent(concat!("eldur/", env!("CARGO_PKG_VERSION")))
            .build();
        let agent = match reach {
            Reach::Any => config.new_agent(),
            Reach::Public => ureq::Agent::with_parts(
                config,
                ureq::unversioned::transport::DefaultConnector::default(),
                PublicOnly::default(),
            ),
        };
        Self {
            agent,
            limits,
            reach,
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

    /// The machines this resolver may reach.
    #[must_use]
    pub fn reach(&self) -> Reach {
        self.reach
    }

    /// Fetches the page at `url`. Its [`Input::url`] is where the page was
    /// found after redirects, as a browser has it, so its references resolve
    /// there. The bytes are read as UTF-8, invalid sequences replaced. The
    /// fetch counts against the same [`Limits`] as the page's references.
    /// Each page starts afresh: an earlier page's credentials, answers,
    /// failures and requests do not carry over to it, and [`Limits`] are
    /// counted per page.
    pub fn fetch_page(&mut self, url: &str) -> Result<Input, FetchError> {
        let mut target = Url::parse(url.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| FetchError::NotHttp { url: redacted(url) })?;
        target.set_fragment(None);
        self.credentials = None;
        self.answers.clear();
        self.fetches = 0;
        self.total_bytes = 0;
        self.failures.clear();
        self.fetched.clear();
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
                // Shown without the user, query and fragment a reference
                // or a redirect's URL can carry.
                source: redacted(&eldur_convert::quoted_reference(&f.url)),
                line: None,
                message: redacted_in(&format!("not fetched: {}", f.reason)),
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
            // Checked before each hop is counted: the page and every
            // redirect it gives. The resolver checks what a name leads to.
            if self.reach == Reach::Public {
                refuse_written_host(&at)?;
            }
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
                let mut next = at.join(location).map_err(|err| {
                    format!(
                        "a redirect to `{}`: {err}",
                        eldur_convert::quoted_reference(location)
                    )
                })?;
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
        ureq::Error::Other(inner) if inner.is::<NotPublic>() => inner.to_string(),
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

/// A host that [`Reach::Public`] does not reach.
#[derive(Debug)]
struct NotPublic(String);

impl std::fmt::Display for NotPublic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "refused: {} is not on the public internet (this machine, a private network or a link-local address)",
            self.0
        )
    }
}

impl std::error::Error for NotPublic {}

/// Refuses a URL whose host, as written, is this machine or an address
/// that is not public. A name is left to [`PublicOnly`], which sees what it
/// resolves to.
fn refuse_written_host(at: &Url) -> Result<(), String> {
    let refused = match at.host() {
        Some(url::Host::Ipv4(ip)) => !is_public(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => !is_public(IpAddr::V6(ip)),
        // RFC 6761: `localhost` and every name under it is this machine,
        // whatever a resolver says.
        Some(url::Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost")
        }
        None => false,
    };
    if refused {
        let host = at.host_str().unwrap_or_default().to_owned();
        return Err(NotPublic(host).to_string());
    }
    Ok(())
}

/// Whether `ip` is a globally routable address: not this machine, not a
/// private, shared, link-local, documentation, benchmarking, reserved,
/// broadcast or multicast range (IANA's special-purpose registries).
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let s = ip.segments();
            // NAT64 (64:ff9b::/96) carries an IPv4 address in its last 32 bits.
            if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = s[6].to_be_bytes();
                let [c, d] = s[7].to_be_bytes();
                return is_public_v4(Ipv4Addr::new(a, b, c, d));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] & 0xffc0) == 0xfec0 // site-local, deprecated
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                // 6to4 and Teredo relay to an IPv4 address the check never
                // sees, so neither is followed.
                || s[0] == 0x2002 // 6to4
                || (s[0] == 0x2001 && s[1] == 0) // Teredo
                || s[..4] == [0x100, 0, 0, 0] // discard-only
                || s[..6] == [0, 0, 0, 0, 0, 0]) // IPv4-compatible, deprecated
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (b & 0xc0) == 64) // shared address space (CGNAT)
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 198 && (b & 0xfe) == 18) // benchmarking
        || a >= 240) // reserved
}

/// ureq's resolver, keeping only public addresses ([`Reach::Public`]). The
/// connection is made to what is kept, so a name cannot pass the check with
/// one address and connect to another. A proxy's own address is the
/// person's choice, not the page's, and is kept as it resolves.
#[derive(Debug, Default)]
struct PublicOnly {
    inner: ureq::unversioned::resolver::DefaultResolver,
}

impl ureq::unversioned::resolver::Resolver for PublicOnly {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let addrs = self.inner.resolve(uri, config, timeout)?;
        let host = uri.host().unwrap_or_default();
        if config
            .proxy()
            .is_some_and(|proxy| proxy.host().eq_ignore_ascii_case(host))
        {
            return Ok(addrs);
        }
        let mut public = self.inner.empty();
        for addr in addrs.iter().filter(|addr| is_public(addr.ip())) {
            public.push(*addr);
        }
        if public.is_empty() {
            return Err(ureq::Error::Other(Box::new(NotPublic(host.to_owned()))));
        }
        Ok(public)
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
/// case. A URL ends at whitespace or at the quote, bracket or backtick
/// around it.
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
        Ok(base) => base.join(written).map_err(|err| {
            format!(
                "cannot be resolved against {}: {err}",
                redacted(base.as_str())
            )
        }),
        Err(_) => match Url::parse(written) {
            Ok(absolute) => Ok(absolute),
            Err(url::ParseError::RelativeUrlWithoutBase) => Err(format!(
                "a relative reference in `{}`, which is not a URL",
                eldur_convert::quoted_reference(base)
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

    fn fetches_web(&self) -> bool {
        true
    }
}

/// A local page's resolver `L` with [`HttpResources`] beside it:
/// references that are `http(s)` or `data:` URLs go to `remote`, and so does
/// everything in a document or sheet that was itself fetched, so that
/// fetched content never reaches `local` (`remote` reads no `file:` URL or
/// path). The rest (paths and `file:` URLs in local files) go to `local`.
///
/// A relative `background-image` is read with [`Resources::read_any`]: its
/// computed style no longer says which sheet it came from. When a fetched
/// sheet is among its bases, it may be that sheet's, so it is tried against
/// the fetched bases only and never reaches `local`: a page that mixes a
/// local sheet with a fetched one loses the local sheet's relative
/// backgrounds (each a warning), and no fetched sheet reads a local file.
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
        let fetched: Vec<&str> = bases.iter().copied().filter(|b| is_web_url(b)).collect();
        let bases = if fetched.is_empty() {
            bases
        } else {
            &fetched[..]
        };
        let found = bases.iter().find_map(|base| self.read(url, base));
        if found.is_some() {
            self.remote.failures.truncate(reported);
        }
        found
    }

    fn fetches_web(&self) -> bool {
        true
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
