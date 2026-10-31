//! What Qt paints differently from Chromium at fractional positions
//! (layout.md §1.2).
//!
//! Chromium rounds a box's edges to whole pixels when it paints. A Rectangle
//! without antialiasing comes out the same on the GPU, whose fill covers the
//! pixels with their centres inside. One with a radius antialiases by default,
//! and spreads a straight edge over two rows at partial intensity; aliased
//! corners are the lesser error, so it only antialiases while it sits whole in
//! its parent. A box in a fractional parent looks whole from here and stays
//! soft: knowing better takes a scene position, which no binding can follow.

use eldur_qml::{Member, Object, Value};

const RADII: [&str; 5] = [
    "radius",
    "topLeftRadius",
    "topRightRadius",
    "bottomLeftRadius",
    "bottomRightRadius",
];

/// Whole to Chromium's 1/64 px: the carry leaves micropixels behind.
const SNAPPED: &str = "Math.round(x * 64) % 64 === 0 && Math.round(y * 64) % 64 === 0 \
                       && Math.round(width * 64) % 64 === 0 && Math.round(height * 64) % 64 === 0";

pub(super) fn snap_rounded_boxes(obj: &mut Object, turned: bool) {
    // Edges that are not along the pixel grid need their antialiasing.
    let turned = turned
        || ["transform", "rotation", "scale", "layer.enabled"]
            .iter()
            .any(|p| obj.get(p).is_some());
    if !turned && obj.type_name == "Rectangle" && rounded(obj) {
        obj.set("antialiasing", Value::expr(SNAPPED));
    }
    for m in &mut obj.members {
        match m {
            Member::Child(c) => snap_rounded_boxes(c, turned),
            Member::Property { value, .. } => snap_value(value, turned),
            _ => {}
        }
    }
}

fn snap_value(value: &mut Value, turned: bool) {
    match value {
        Value::Object(o) => snap_rounded_boxes(o, turned),
        Value::List(items) => items.iter_mut().for_each(|v| snap_value(v, turned)),
        _ => {}
    }
}

fn rounded(obj: &Object) -> bool {
    RADII.iter().any(|r| match obj.get(r) {
        Some(Value::Number(n)) => *n != 0.0,
        Some(_) => true,
        None => false,
    })
}
