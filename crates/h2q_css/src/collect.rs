use h2q_dom::Document;

use crate::{Diagnostic, Stylesheet, StylesheetLoader, Viewport};

pub(crate) fn collect_stylesheets(
    _doc: &Document,
    _loader: &mut dyn StylesheetLoader,
    _viewport: &Viewport,
) -> (Vec<Stylesheet>, Vec<Diagnostic>) {
    unimplemented!("h2q_css worker")
}
