//! swc_css_parser → [`Sheet`]: style rules with parsed selectors and
//! declarations, `@media` (kept with its condition), `@keyframes`,
//! `@font-face`, `@import`, `@supports`, `@layer`.

use crate::{Diagnostic, Origin, Stylesheet};

/// Internal representation of a parsed sheet. Shape is the implementer's
/// choice.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sheet {}

pub(crate) fn parse_stylesheet(
    _text: &str,
    _origin: Origin,
    _source_name: &str,
    _base_url: Option<&str>,
) -> (Stylesheet, Vec<Diagnostic>) {
    unimplemented!("h2q_css worker")
}
