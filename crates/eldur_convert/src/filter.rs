//! `filter` values as what draws them: one colour matrix for the colour
//! functions, in order, and the blur and shadow. Both the emitter and the
//! run-time paint values use this, so that the two draw alike.

use eldur_css::{PaintValue, Rgba, Shadow, Viewport};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Filter {
    /// Rows, on unpremultiplied RGBA.
    pub matrix: [[f64; 4]; 4],
    pub offset: [f64; 4],
    /// `blur()`'s standard deviation.
    pub blur: f64,
    pub drop: Option<DropShadow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DropShadow {
    pub x: f64,
    pub y: f64,
    /// `drop-shadow()`'s blur is a standard deviation, unlike `box-shadow`'s.
    pub deviation: f64,
    pub color: Rgba,
}

const IDENTITY: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `None` when part of the value has no drawing here (`url()` references).
pub(crate) fn parse(text: &str, font_size: f32, viewport: &Viewport) -> Option<Filter> {
    let mut filter = Filter {
        matrix: IDENTITY,
        offset: [0.0; 4],
        blur: 0.0,
        drop: None,
    };
    let mut rest = text.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let name = rest[..open].trim().to_ascii_lowercase();
        let mut depth = 0;
        let mut close = None;
        for (i, c) in rest.char_indices().skip(open) {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let args = rest[open + 1..close].trim();
        rest = rest[close + 1..].trim_start();
        match name.as_str() {
            "blur" => filter.blur += length(args, font_size)?,
            "drop-shadow" => {
                let PaintValue::Shadows(list) =
                    eldur_css::parse_paint_value("box-shadow", args, viewport)?
                else {
                    return None;
                };
                let Shadow {
                    offset_x,
                    offset_y,
                    blur,
                    color,
                    ..
                } = *list.first()?;
                filter.drop = Some(DropShadow {
                    x: f64::from(offset_x),
                    y: f64::from(offset_y),
                    deviation: f64::from(blur),
                    color,
                });
            }
            "url" => return None,
            _ => {
                let (matrix, offset) = colour_function(&name, args)?;
                filter.then(matrix, offset);
            }
        }
    }
    Some(filter)
}

impl Filter {
    fn then(&mut self, m: [[f64; 4]; 4], o: [f64; 4]) {
        let mut matrix = [[0.0; 4]; 4];
        let mut offset = [0.0; 4];
        for r in 0..4 {
            for (c, cell) in matrix[r].iter_mut().enumerate() {
                *cell = (0..4).map(|k| m[r][k] * self.matrix[k][c]).sum();
            }
            offset[r] = (0..4).map(|k| m[r][k] * self.offset[k]).sum::<f64>() + o[r];
        }
        self.matrix = matrix;
        self.offset = offset;
    }

    pub(crate) fn recolours(&self) -> bool {
        self.matrix != IDENTITY || self.offset != [0.0; 4]
    }
}

/// The matrices are those of Filter Effects 1 §10.1, by name.
fn colour_function(name: &str, args: &str) -> Option<([[f64; 4]; 4], [f64; 4])> {
    let rgb = |m: [[f64; 3]; 3]| {
        [
            [m[0][0], m[0][1], m[0][2], 0.0],
            [m[1][0], m[1][1], m[1][2], 0.0],
            [m[2][0], m[2][1], m[2][2], 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    };
    let diagonal = |d: f64| rgb([[d, 0.0, 0.0], [0.0, d, 0.0], [0.0, 0.0, d]]);
    Some(match name {
        "brightness" => (diagonal(amount(args)?), [0.0; 4]),
        "contrast" => {
            let c = amount(args)?;
            let shift = (1.0 - c) / 2.0;
            (diagonal(c), [shift, shift, shift, 0.0])
        }
        "invert" => {
            let a = amount(args)?.min(1.0);
            (diagonal(1.0 - 2.0 * a), [a, a, a, 0.0])
        }
        "opacity" => {
            let mut m = IDENTITY;
            m[3][3] = amount(args)?.min(1.0);
            (m, [0.0; 4])
        }
        "grayscale" => {
            let k = 1.0 - amount(args)?.min(1.0);
            (
                rgb([
                    [
                        0.2126 + 0.7874 * k,
                        0.7152 - 0.7152 * k,
                        0.0722 - 0.0722 * k,
                    ],
                    [
                        0.2126 - 0.2126 * k,
                        0.7152 + 0.2848 * k,
                        0.0722 - 0.0722 * k,
                    ],
                    [
                        0.2126 - 0.2126 * k,
                        0.7152 - 0.7152 * k,
                        0.0722 + 0.9278 * k,
                    ],
                ]),
                [0.0; 4],
            )
        }
        "sepia" => {
            let k = 1.0 - amount(args)?.min(1.0);
            (
                rgb([
                    [0.393 + 0.607 * k, 0.769 - 0.769 * k, 0.189 - 0.189 * k],
                    [0.349 - 0.349 * k, 0.686 + 0.314 * k, 0.168 - 0.168 * k],
                    [0.272 - 0.272 * k, 0.534 - 0.534 * k, 0.131 + 0.869 * k],
                ]),
                [0.0; 4],
            )
        }
        "saturate" => {
            let s = amount(args)?;
            (
                rgb([
                    [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
                    [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
                    [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
                ]),
                [0.0; 4],
            )
        }
        "hue-rotate" => {
            let (sin, cos) = angle(args)?.sin_cos();
            (
                rgb([
                    [
                        0.213 + cos * 0.787 - sin * 0.213,
                        0.715 - cos * 0.715 - sin * 0.715,
                        0.072 - cos * 0.072 + sin * 0.928,
                    ],
                    [
                        0.213 - cos * 0.213 + sin * 0.143,
                        0.715 + cos * 0.285 + sin * 0.140,
                        0.072 - cos * 0.072 - sin * 0.283,
                    ],
                    [
                        0.213 - cos * 0.213 - sin * 0.787,
                        0.715 - cos * 0.715 + sin * 0.715,
                        0.072 + cos * 0.928 + sin * 0.072,
                    ],
                ]),
                [0.0; 4],
            )
        }
        _ => return None,
    })
}

fn amount(args: &str) -> Option<f64> {
    if args.is_empty() {
        return Some(1.0);
    }
    match args.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok().map(|n| n / 100.0),
        None => args.parse().ok(),
    }
}

fn angle(args: &str) -> Option<f64> {
    if args.is_empty() || args == "0" {
        return Some(0.0);
    }
    for (unit, per_radian) in [
        ("grad", std::f64::consts::PI / 200.0),
        ("deg", std::f64::consts::PI / 180.0),
        ("rad", 1.0),
        ("turn", std::f64::consts::TAU),
    ] {
        if let Some(n) = args.strip_suffix(unit) {
            return n.trim().parse::<f64>().ok().map(|n| n * per_radian);
        }
    }
    None
}

fn length(args: &str, font_size: f32) -> Option<f64> {
    if args.is_empty() || args == "0" {
        return Some(0.0);
    }
    if let Some(n) = args.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    let n: f64 = args.strip_suffix("em")?.trim().parse().ok()?;
    Some(n * f64::from(font_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(f: &Filter, c: [f64; 4]) -> [f64; 4] {
        let mut out = [0.0; 4];
        for (r, value) in out.iter_mut().enumerate() {
            *value = (0..4).map(|k| f.matrix[r][k] * c[k]).sum::<f64>() + f.offset[r];
        }
        out
    }

    #[test]
    fn colour_functions_chain_in_order() {
        let vp = Viewport::default();
        let f = parse("invert(1) brightness(.5)", 16.0, &vp).expect("parses");
        let out = apply(&f, [1.0, 0.0, 0.5, 1.0]);
        assert!(
            (out[0] - 0.0).abs() < 1e-9 && (out[1] - 0.5).abs() < 1e-9,
            "{out:?}"
        );
        assert!(
            (out[2] - 0.25).abs() < 1e-9 && (out[3] - 1.0).abs() < 1e-9,
            "{out:?}"
        );
    }

    #[test]
    fn blur_and_drop_shadow_are_apart_from_the_matrix() {
        let vp = Viewport::default();
        let f = parse("blur(.5em) drop-shadow(1px 2px 3px red)", 8.0, &vp).expect("parses");
        assert!(!f.recolours());
        assert_eq!(f.blur, 4.0);
        let d = f.drop.expect("shadow");
        assert_eq!((d.x, d.y, d.deviation), (1.0, 2.0, 3.0));
    }

    #[test]
    fn a_reference_has_no_drawing() {
        assert_eq!(parse("url(#a)", 16.0, &Viewport::default()), None);
    }
}
