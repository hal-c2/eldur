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

use std::collections::{HashMap, HashSet};
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
    /// The most one request may take, from connecting to the body's end
    /// (redirects included).
    pub timeout: Duration,
    /// The largest body one resource may have, in bytes.
    pub max_resource_bytes: u64,
    /// The most bytes all of a page's fetches together may bring.
    pub max_total_bytes: u64,
    /// The most HTTP requests a page may make, the page's own included.
    /// `data:` URLs are decoded, not fetched, and do not count.
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

/// Whether `input` is an absolute `http` or `https` URL (any case), i.e. a
/// page [`HttpResources::fetch_page`] takes rather than a file path.
#[must_use]
pub fn is_http_url(input: &str) -> bool {
    Url::parse(input.trim()).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// A reference that was not served, kept to report it.
#[derive(Debug, Clone)]
struct Failure {
    /// The reference as written, to drop the report when another base
    /// serves it (see [`HttpResources::diagnostics`]).
    written: String,
    url: String,
    reason: String,
}

/// An [`eldur_convert::Resources`] over HTTP(S) and `data:` URLs, for a page
/// given by URL. See the crate documentation.
///
/// A resource is fetched once per URL (without its fragment); asking again
/// returns the first answer.
pub struct HttpResources {
    agent: ureq::Agent,
    limits: Limits,
    fetches: usize,
    total_bytes: u64,
    /// By request URL: the resource, or `None` when it was not served.
    answers: HashMap<String, Option<Resource>>,
    /// References as written that some base served.
    served: HashSet<String>,
    failures: Vec<Failure>,
    fetched: Vec<String>,
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
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(limits.timeout))
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
            served: HashSet::new(),
            failures: Vec::new(),
            fetched: Vec::new(),
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
    pub fn fetch_page(&mut self, url: &str) -> Result<Input, FetchError> {
        let not_http = || FetchError::NotHttp {
            url: url.to_owned(),
        };
        let mut target = Url::parse(url.trim()).map_err(|_| not_http())?;
        if !matches!(target.scheme(), "http" | "https") {
            return Err(not_http());
        }
        target.set_fragment(None);
        match self.fetch(&target) {
            Ok(resource) => {
                self.answers
                    .insert(target.to_string(), Some(resource.clone()));
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

    /// A warning for every reference that was not served, in the order they
    /// were asked for, naming its URL and why. A reference the converter
    /// tried against several bases (a `background-image` is tried against
    /// the page and then each style sheet) and some base served is left out.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.failures
            .iter()
            .filter(|f| !self.served.contains(&f.written))
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

    /// The number of HTTP requests made so far.
    #[must_use]
    pub fn fetch_count(&self) -> usize {
        self.fetches
    }

    /// The bytes all fetches have brought so far.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn fail(&mut self, written: &str, url: String, reason: String) {
        self.failures.push(Failure {
            written: written.to_owned(),
            url,
            reason,
        });
    }

    fn fetch(&mut self, target: &Url) -> Result<Resource, String> {
        if self.fetches >= self.limits.max_fetches {
            return Err(format!(
                "refused: the page has made {} requests, the most allowed",
                self.limits.max_fetches
            ));
        }
        let room = self.limits.max_total_bytes.saturating_sub(self.total_bytes);
        if room == 0 {
            return Err(format!(
                "refused: the page has fetched {} bytes, the most allowed",
                self.limits.max_total_bytes
            ));
        }
        self.fetches += 1;
        let mut response = self
            .agent
            .get(target.as_str())
            .call()
            .map_err(|err| describe(&err))?;
        let resolved = {
            use ureq::ResponseExt as _;
            response.get_uri().to_string()
        };
        let cap = self.limits.max_resource_bytes.min(room);
        let bytes = response
            .body_mut()
            .with_config()
            .limit(cap)
            .read_to_vec()
            .map_err(|err| match err {
                ureq::Error::BodyExceedsLimit(_) if cap < self.limits.max_resource_bytes => {
                    format!(
                        "refused: it would take the page past {} bytes in all",
                        self.limits.max_total_bytes
                    )
                }
                ureq::Error::BodyExceedsLimit(_) => {
                    format!("refused: it is larger than {cap} bytes")
                }
                other => describe(&other),
            })?;
        self.total_bytes += bytes.len() as u64;
        self.fetched.push(target.to_string());
        Ok(Resource { resolved, bytes })
    }

    fn decode_data(&mut self, url: &str) -> Result<Resource, String> {
        let data = data_url::DataUrl::process(url).map_err(|_| "a malformed data: URL")?;
        let (bytes, _) = data
            .decode_to_vec()
            .map_err(|_| "a data: URL with invalid base64")?;
        if bytes.len() as u64 > self.limits.max_resource_bytes {
            return Err(format!(
                "refused: it is larger than {} bytes",
                self.limits.max_resource_bytes
            ));
        }
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

/// `written` resolved against `base`, without its fragment: `None` for a
/// reference to nothing (empty, or only a fragment).
fn resolve(written: &str, base: &str) -> Option<Result<Url, String>> {
    if written.is_empty() || written.starts_with('#') {
        return None;
    }
    let joined = match Url::parse(written) {
        Ok(absolute) => Ok(absolute),
        Err(url::ParseError::RelativeUrlWithoutBase) => match Url::parse(base.trim()) {
            Ok(base) => base
                .join(written)
                .map_err(|err| format!("cannot be resolved against {base}: {err}")),
            Err(_) => Err(format!(
                "a relative reference in `{base}`, which is not a URL"
            )),
        },
        Err(err) => Err(format!("not a URL: {err}")),
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
                self.fail(written, written.to_owned(), reason);
                return None;
            }
        };
        let key = target.to_string();
        if let Some(answer) = self.answers.get(&key) {
            let answer = answer.clone();
            if answer.is_some() {
                self.served.insert(written.to_owned());
            }
            return answer;
        }
        let answer = match target.scheme() {
            "http" | "https" => self.fetch(&target),
            // The fragment is part of a data: URL's body, so it is decoded
            // as written.
            "data" => self.decode_data(written),
            scheme => Err(format!(
                "refused: a fetched page reads only http, https and data URLs, not {scheme}:"
            )),
        };
        let answer = match answer {
            Ok(resource) => {
                self.served.insert(written.to_owned());
                Some(resource)
            }
            Err(reason) => {
                self.fail(written, key.clone(), reason);
                None
            }
        };
        self.answers.insert(key, answer.clone());
        answer
    }
}

/// A local page's resolver `L` with [`HttpResources`] beside it: references
/// that are `http(s)` or `data:` URLs, or relative ones in a document or
/// sheet that was itself fetched, go to `remote`; the rest (paths, `file:`
/// URLs) to `local`.
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
        let remote = match Url::parse(url.trim()) {
            Ok(absolute) => matches!(absolute.scheme(), "http" | "https" | "data"),
            Err(_) => is_web_url(base),
        };
        if remote {
            self.remote.read(url, base)
        } else {
            self.local.read(url, base)
        }
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
