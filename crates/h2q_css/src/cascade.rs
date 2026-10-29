use h2q_dom::{Document, NodeId};

use crate::{ComputedStyle, Keyframe, StyleMap, Stylesheet, Viewport};

pub(crate) fn compute_styles(
    _doc: &Document,
    _sheets: &[Stylesheet],
    _viewport: &Viewport,
) -> StyleMap {
    unimplemented!("h2q_css worker")
}

pub(crate) fn keyframe_style(
    _map: &StyleMap,
    _node: NodeId,
    _keyframe: &Keyframe,
) -> Option<ComputedStyle> {
    unimplemented!("h2q_css worker")
}
