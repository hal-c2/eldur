//! HTML + CSS + JS → QML.
//!
//! Pipeline (see `docs/ARCHITECTURE.md`):
//!
//! 1. `h2q_dom::Document::parse` — the DOM.
//! 2. `h2q_css::collect_stylesheets` + `compute_styles` — computed styles.
//! 3. `h2q_js::analyze` — which elements scripts touch.
//! 4. Box tree: elements with `display != none` become boxes (block,
//!    inline run, flex/grid container, table, replaced, control), with
//!    anonymous boxes where CSS needs them and whitespace collapsed.
//! 5. QML tree: each box picks its QML form (`docs/spec/layout.md`,
//!    `docs/spec/html-elements.md`, `docs/spec/css-properties.md`).
//! 6. `h2q_js::rewrite` — `page.js`, with ids from step 5.
//! 7. `h2q_qml` prints `Main.qml`.
//!
//! The public API in this file is the contract the CLI is written against.
//! Add to it freely; change or remove items only with the lead's agreement.

use std::path::PathBuf;

pub use h2q_css::Viewport;

/// Options that change the output (they are part of the cache key; keep
/// `Debug` output stable).
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Viewport media queries and `vw`/`vh` are evaluated against; also the
    /// window's initial size.
    pub viewport: Viewport,
    /// Root of `Main.qml`: an `ApplicationWindow` (runnable with `qml6`) or
    /// an `Item` (to embed).
    pub root: RootKind,
    /// Emit `// <div class="card">` comments above converted elements.
    pub source_comments: bool,
    /// Turn `:hover`/`:focus`/… into QML `states` (else ignore them).
    pub state_variants: bool,
    /// Turn CSS transitions/animations into QML Behaviors/animations.
    pub animations: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            viewport: Viewport::default(),
            root: RootKind::Window,
            source_comments: true,
            state_variants: true,
            animations: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootKind {
    #[default]
    Window,
    Item,
}

/// Where the page's sub-resources come from. The CLI implements it over
/// the file system (recording every read for the build cache); tests
/// implement it over a map.
pub trait Resources {
    /// Reads a resource referenced from the page as `url` (relative to
    /// `base`, the URL of the document or style sheet that references it).
    /// Returns its resolved name and bytes.
    fn read(&mut self, url: &str, base: &str) -> Option<Resource>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// Resolved path/URL (used in diagnostics and as base for its own
    /// relative references).
    pub resolved: String,
    pub bytes: Vec<u8>,
}

/// Resources that are never found (for converting a single string).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoResources;

impl Resources for NoResources {
    fn read(&mut self, _url: &str, _base: &str) -> Option<Resource> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub html: String,
    /// URL or path of the HTML document (base for relative references).
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// `html`, `css`, `js` or `qml`.
    pub stage: &'static str,
    pub source: String,
    pub line: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{sev}[{}]: {}", self.stage, self.source)?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// A file to write, relative to the output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFile {
    pub path: PathBuf,
    pub contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    /// Always `Main.qml`; plus `page.js` and `h2q_runtime.js` when the page
    /// has scripts, and copies of local images/fonts it uses under
    /// `assets/` (referenced relatively from the QML).
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Output {
    /// `Main.qml` as text.
    #[must_use]
    pub fn main_qml(&self) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.path == std::path::Path::new("Main.qml"))
            .and_then(|f| std::str::from_utf8(&f.contents).ok())
    }
}

/// Converts one HTML document. Never fails: what cannot be converted is
/// approximated or dropped, with a diagnostic.
#[must_use]
pub fn convert(input: &Input, options: &Options, resources: &mut dyn Resources) -> Output {
    pipeline::convert(input, options, resources)
}

/// Converts an HTML string with no sub-resources (tests, quick use).
#[must_use]
pub fn convert_str(html: &str, options: &Options) -> Output {
    convert(
        &Input {
            html: html.to_owned(),
            url: "index.html".to_owned(),
        },
        options,
        &mut NoResources,
    )
}

mod boxes;
mod diag;
mod emit;
mod pipeline;
mod scripts;
