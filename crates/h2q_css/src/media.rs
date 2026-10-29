//! Media queries (`@media`, `<link media>`, `<style media>`, `@import ... media`)
//! evaluated against a [`crate::Viewport`] per Media Queries Level 4.

use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_css_ast as ast;
use swc_css_codegen::{
    CodeGenerator, CodegenConfig, Emit,
    writer::basic::{BasicCssWriter, BasicCssWriterConfig},
};

use crate::Viewport;

/// Three-valued Kleene logic required by Media Queries 4 §3.2 so unknown
/// features or `<general-enclosed>` expressions propagate through `not`/`and`/`or`
/// before collapsing to `false` (`not all`) at the top of each media query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl From<bool> for Truth {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}

impl Truth {
    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    fn to_bool(self) -> bool {
        matches!(self, Self::True)
    }
}

/// A comma-separated list of media queries. An empty list matches all media.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MediaQueryList {
    queries: Vec<MediaQuery>,
}

#[derive(Debug, Clone, PartialEq)]
struct MediaQuery {
    negated: bool,
    media_type: Option<MediaTypePat>,
    condition: Option<MediaCond>,
    /// True when the query had a syntax error and must evaluate to `not all`.
    invalid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaTypePat {
    All,
    Screen,
    Print,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
enum MediaCond {
    Not(Box<MediaCond>),
    And(Vec<MediaCond>),
    Or(Vec<MediaCond>),
    Feature(FeatureExpr),
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
enum FeatureExpr {
    Boolean(String),
    Plain {
        name: String,
        value: FeatureVal,
    },
    Range {
        name: String,
        op: RangeOp,
        value: FeatureVal,
    },
    Interval {
        name: String,
        left_val: FeatureVal,
        left_op: RangeOp,
        right_op: RangeOp,
        right_val: FeatureVal,
    },
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RangeOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

impl RangeOp {
    fn from_ast(cmp: ast::MediaFeatureRangeComparison) -> Self {
        match cmp {
            ast::MediaFeatureRangeComparison::Lt => Self::Lt,
            ast::MediaFeatureRangeComparison::Le => Self::Le,
            ast::MediaFeatureRangeComparison::Gt => Self::Gt,
            ast::MediaFeatureRangeComparison::Ge => Self::Ge,
            ast::MediaFeatureRangeComparison::Eq => Self::Eq,
        }
    }

    /// Flips `value OP feature` into `feature FLIPPED_OP value`.
    fn flip(self) -> Self {
        match self {
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
            Self::Eq => Self::Eq,
        }
    }

    fn is_less(self) -> bool {
        matches!(self, Self::Lt | Self::Le)
    }

    fn is_greater(self) -> bool {
        matches!(self, Self::Gt | Self::Ge)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FeatureVal {
    LengthPx(f32),
    Ratio(f64, f64),
    ResolutionDppx(f32),
    Ident(String),
    Zero,
    Invalid,
}

impl FeatureVal {
    fn as_length_px(&self) -> Option<f32> {
        match *self {
            Self::LengthPx(px) if px >= 0.0 => Some(px),
            Self::Zero => Some(0.0),
            _ => None,
        }
    }

    fn as_ratio(&self) -> Option<(f64, f64)> {
        match *self {
            Self::Ratio(a, b) if a >= 0.0 && b > 0.0 => Some((a, b)),
            Self::Zero => Some((0.0, 1.0)),
            _ => None,
        }
    }

    fn as_resolution_dppx(&self) -> Option<f32> {
        match *self {
            Self::ResolutionDppx(d) if d >= 0.0 => Some(d),
            _ => None,
        }
    }
}

impl MediaQuery {
    fn never() -> Self {
        Self {
            negated: false,
            media_type: None,
            condition: None,
            invalid: true,
        }
    }

    fn matches(&self, viewport: &Viewport) -> bool {
        if self.invalid {
            return false;
        }
        let type_matches = match self.media_type {
            None | Some(MediaTypePat::All | MediaTypePat::Screen) => Truth::True,
            Some(MediaTypePat::Print | MediaTypePat::Other) => Truth::False,
        };
        let cond_matches = match &self.condition {
            Some(c) => c.eval(viewport),
            None => Truth::True,
        };
        let combined = type_matches.and(cond_matches);
        let final_truth = if self.negated {
            combined.not()
        } else {
            combined
        };
        final_truth.to_bool()
    }
}

impl MediaQueryList {
    pub(crate) fn from_ast(list: &ast::MediaQueryList) -> Self {
        Self {
            queries: list.queries.iter().map(convert_media_query).collect(),
        }
    }

    /// Recovers comma-separated queries individually when `swc_css_parser` left
    /// an `@media` prelude as `ListOfComponentValues` due to a syntax error in
    /// one branch.
    pub(crate) fn from_component_values(cvs: &[ast::ComponentValue]) -> Self {
        let mut out = String::new();
        let locv = ast::ListOfComponentValues {
            span: swc_common::DUMMY_SP,
            children: cvs.to_vec(),
        };
        let writer = BasicCssWriter::new(&mut out, None, BasicCssWriterConfig::default());
        let mut codegen = CodeGenerator::new(writer, CodegenConfig { minify: false });
        if codegen.emit(&locv).is_err() {
            return Self {
                queries: vec![MediaQuery::never()],
            };
        }
        Self::parse_str(&out)
    }

    /// Parses a media query list string (such as `<link media="...">`).
    pub(crate) fn parse_str(input: &str) -> Self {
        let trimmed = strip_css_comments(input);
        let trimmed = trimmed.trim();
        if trimmed.is_empty() {
            return Self {
                queries: Vec::new(),
            };
        }

        let parts = split_top_level_commas(trimmed);
        let mut queries = Vec::with_capacity(parts.len());
        for part in parts {
            let chunk = part.trim();
            if chunk.is_empty() {
                queries.push(MediaQuery::never());
                continue;
            }
            queries.push(parse_single_media_query(chunk));
        }
        Self { queries }
    }

    pub(crate) fn matches(&self, viewport: &Viewport) -> bool {
        if self.queries.is_empty() {
            return true;
        }
        self.queries.iter().any(|q| q.matches(viewport))
    }
}

/// Evaluates a media query list string directly against `viewport`.
pub(crate) fn matches_str(input: &str, viewport: &Viewport) -> bool {
    MediaQueryList::parse_str(input).matches(viewport)
}

fn parse_single_media_query(chunk: &str) -> MediaQuery {
    // Reject unbalanced closing delimiters or braces/semicolons that could
    // escape the synthetic `@media <chunk> {}` wrapper.
    if !is_safe_media_chunk(chunk) {
        return MediaQuery::never();
    }
    let wrapped = format!("@media {chunk} {{}}");
    let cm: Lrc<SourceMap> = Lrc::default();
    let fm = cm.new_source_file(FileName::Anon.into(), wrapped);
    let mut errors = Vec::new();
    let parsed: Result<Result<ast::Stylesheet, _>, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            swc_css_parser::parse_file(&fm, None, Default::default(), &mut errors)
        }));
    let Ok(Ok(sheet)) = parsed else {
        return MediaQuery::never();
    };
    if !errors.is_empty() || sheet.rules.len() != 1 {
        return MediaQuery::never();
    }
    if let ast::Rule::AtRule(at) = &sheet.rules[0]
        && let Some(prelude) = &at.prelude
        && let ast::AtRulePrelude::MediaPrelude(mql) = &**prelude
        && mql.queries.len() == 1
    {
        return convert_media_query(&mql.queries[0]);
    }
    MediaQuery::never()
}

fn is_safe_media_chunk(chunk: &str) -> bool {
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut chars = chunk.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            if c == '\\' {
                let _ = chars.next();
            } else if c == q {
                in_quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => in_quote = Some(c),
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return false;
                }
            }
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return false;
                }
            }
            '{' | '}' | ';' => return false,
            _ => {}
        }
    }
    in_quote.is_none() && paren_depth == 0 && bracket_depth == 0
}

fn strip_css_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == q {
                in_quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_quote = Some(c);
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'*') {
            let _ = chars.next();
            while let Some(cc) = chars.next() {
                if cc == '*' && chars.peek() == Some(&'/') {
                    let _ = chars.next();
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut in_quote: Option<char> = None;
    let mut start = 0;
    let mut iter = s.char_indices();

    while let Some((idx, c)) = iter.next() {
        if let Some(q) = in_quote {
            if c == '\\' {
                let _ = iter.next();
            } else if c == q {
                in_quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => in_quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                parts.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn convert_media_query(mq: &ast::MediaQuery) -> MediaQuery {
    let negated = mq
        .modifier
        .as_ref()
        .is_some_and(|m| m.value.eq_ignore_ascii_case("not"));

    if let Some(m) = &mq.modifier
        && !m.value.eq_ignore_ascii_case("not")
        && !m.value.eq_ignore_ascii_case("only")
    {
        return MediaQuery::never();
    }

    let media_type = mq.media_type.as_ref().map(|mt| match mt {
        ast::MediaType::Ident(id) => match id.value.to_ascii_lowercase().as_str() {
            "all" => MediaTypePat::All,
            "screen" => MediaTypePat::Screen,
            "print" => MediaTypePat::Print,
            _ => MediaTypePat::Other,
        },
    });

    let condition = mq.condition.as_deref().map(convert_condition_type);

    if media_type.is_none() && condition.is_none() {
        return MediaQuery::never();
    }

    MediaQuery {
        negated,
        media_type,
        condition,
        invalid: false,
    }
}

fn convert_condition_type(ct: &ast::MediaConditionType) -> MediaCond {
    match ct {
        ast::MediaConditionType::All(mc) => convert_media_condition(mc),
        ast::MediaConditionType::WithoutOr(mc) => convert_media_condition_without_or(mc),
    }
}

fn convert_media_condition(mc: &ast::MediaCondition) -> MediaCond {
    if mc.conditions.is_empty() {
        return MediaCond::Unknown;
    }
    if mc.conditions.len() == 1 {
        return match &mc.conditions[0] {
            ast::MediaConditionAllType::Not(n) => {
                MediaCond::Not(Box::new(convert_in_parens(&n.condition)))
            }
            ast::MediaConditionAllType::MediaInParens(p) => convert_in_parens(p),
            ast::MediaConditionAllType::And(a) => convert_in_parens(&a.condition),
            ast::MediaConditionAllType::Or(o) => convert_in_parens(&o.condition),
        };
    }

    let mut is_or = false;
    let mut items = Vec::with_capacity(mc.conditions.len());
    for item in &mc.conditions {
        match item {
            ast::MediaConditionAllType::MediaInParens(p) => items.push(convert_in_parens(p)),
            ast::MediaConditionAllType::And(a) => items.push(convert_in_parens(&a.condition)),
            ast::MediaConditionAllType::Or(o) => {
                is_or = true;
                items.push(convert_in_parens(&o.condition));
            }
            ast::MediaConditionAllType::Not(n) => {
                items.push(MediaCond::Not(Box::new(convert_in_parens(&n.condition))));
            }
        }
    }
    if is_or {
        MediaCond::Or(items)
    } else {
        MediaCond::And(items)
    }
}

fn convert_media_condition_without_or(mc: &ast::MediaConditionWithoutOr) -> MediaCond {
    if mc.conditions.is_empty() {
        return MediaCond::Unknown;
    }
    if mc.conditions.len() == 1 {
        return match &mc.conditions[0] {
            ast::MediaConditionWithoutOrType::Not(n) => {
                MediaCond::Not(Box::new(convert_in_parens(&n.condition)))
            }
            ast::MediaConditionWithoutOrType::MediaInParens(p) => convert_in_parens(p),
            ast::MediaConditionWithoutOrType::And(a) => convert_in_parens(&a.condition),
        };
    }

    let items = mc
        .conditions
        .iter()
        .map(|item| match item {
            ast::MediaConditionWithoutOrType::MediaInParens(p) => convert_in_parens(p),
            ast::MediaConditionWithoutOrType::And(a) => convert_in_parens(&a.condition),
            ast::MediaConditionWithoutOrType::Not(n) => {
                MediaCond::Not(Box::new(convert_in_parens(&n.condition)))
            }
        })
        .collect();
    MediaCond::And(items)
}

fn convert_in_parens(p: &ast::MediaInParens) -> MediaCond {
    match p {
        ast::MediaInParens::MediaCondition(mc) => convert_media_condition(mc),
        ast::MediaInParens::Feature(f) => MediaCond::Feature(convert_feature(f)),
        ast::MediaInParens::GeneralEnclosed(_) => MediaCond::Unknown,
    }
}

fn feature_name_str(name: &ast::MediaFeatureName) -> Option<String> {
    match name {
        ast::MediaFeatureName::Ident(id) => Some(id.value.as_str().to_ascii_lowercase()),
        ast::MediaFeatureName::ExtensionName(_) => None,
    }
}

fn convert_feature(f: &ast::MediaFeature) -> FeatureExpr {
    match f {
        ast::MediaFeature::Boolean(b) => match feature_name_str(&b.name) {
            Some(name) => FeatureExpr::Boolean(name),
            None => FeatureExpr::Invalid,
        },
        ast::MediaFeature::Plain(p) => {
            let Some(name) = feature_name_str(&p.name) else {
                return FeatureExpr::Invalid;
            };
            FeatureExpr::Plain {
                name,
                value: convert_feature_value(&p.value),
            }
        }
        ast::MediaFeature::Range(r) => {
            let op = RangeOp::from_ast(r.comparison);
            match (&*r.left, &*r.right) {
                (ast::MediaFeatureValue::Ident(id), right)
                    if is_known_or_ident_feature(&id.value)
                        && !matches!(right, ast::MediaFeatureValue::Ident(_)) =>
                {
                    FeatureExpr::Range {
                        name: id.value.as_str().to_ascii_lowercase(),
                        op,
                        value: convert_feature_value(right),
                    }
                }
                (left, ast::MediaFeatureValue::Ident(id))
                    if !matches!(left, ast::MediaFeatureValue::Ident(_)) =>
                {
                    FeatureExpr::Range {
                        name: id.value.as_str().to_ascii_lowercase(),
                        op: op.flip(),
                        value: convert_feature_value(left),
                    }
                }
                (ast::MediaFeatureValue::Ident(left_id), ast::MediaFeatureValue::Ident(right_id)) =>
                {
                    // Discrete `=` like `(orientation = landscape)` or unknown range feature.
                    let l = left_id.value.as_str().to_ascii_lowercase();
                    let r_low = right_id.value.as_str().to_ascii_lowercase();
                    if is_range_or_discrete_feature_name(&l) {
                        FeatureExpr::Range {
                            name: l,
                            op,
                            value: FeatureVal::Ident(r_low),
                        }
                    } else {
                        FeatureExpr::Range {
                            name: r_low,
                            op: op.flip(),
                            value: FeatureVal::Ident(l),
                        }
                    }
                }
                _ => FeatureExpr::Invalid,
            }
        }
        ast::MediaFeature::RangeInterval(ri) => {
            let Some(name) = feature_name_str(&ri.name) else {
                return FeatureExpr::Invalid;
            };
            let left_op = RangeOp::from_ast(ri.left_comparison);
            let right_op = RangeOp::from_ast(ri.right_comparison);
            // Media Queries 4 §2.4.3: both operators in an interval must point
            // in the same direction (`<`/`<=` or `>`/`>=`).
            let valid_dir = (left_op.is_less() && right_op.is_less())
                || (left_op.is_greater() && right_op.is_greater());
            if !valid_dir {
                return FeatureExpr::Invalid;
            }
            FeatureExpr::Interval {
                name,
                left_val: convert_feature_value(&ri.left),
                left_op,
                right_op,
                right_val: convert_feature_value(&ri.right),
            }
        }
    }
}

fn is_known_or_ident_feature(_s: &str) -> bool {
    true
}

fn is_range_or_discrete_feature_name(name: &str) -> bool {
    matches!(
        name,
        "width"
            | "height"
            | "aspect-ratio"
            | "resolution"
            | "orientation"
            | "prefers-color-scheme"
            | "prefers-reduced-motion"
            | "hover"
            | "pointer"
            | "any-hover"
            | "any-pointer"
    )
}

fn convert_feature_value(v: &ast::MediaFeatureValue) -> FeatureVal {
    match v {
        ast::MediaFeatureValue::Number(n) => {
            if n.value == 0.0 {
                FeatureVal::Zero
            } else if n.value > 0.0 {
                FeatureVal::Ratio(n.value, 1.0)
            } else {
                FeatureVal::Invalid
            }
        }
        ast::MediaFeatureValue::Ident(id) => {
            FeatureVal::Ident(id.value.as_str().to_ascii_lowercase())
        }
        ast::MediaFeatureValue::Ratio(r) => {
            let num = r.left.value;
            let den = r.right.as_ref().map_or(1.0, |d| d.value);
            if num >= 0.0 && den > 0.0 {
                FeatureVal::Ratio(num, den)
            } else {
                FeatureVal::Invalid
            }
        }
        ast::MediaFeatureValue::Dimension(dim) => match dim {
            ast::Dimension::Length(l) => convert_length_unit(l.value.value as f32, &l.unit.value),
            ast::Dimension::Resolution(r) => {
                convert_resolution_unit(r.value.value as f32, &r.unit.value)
            }
            ast::Dimension::UnknownDimension(u) => {
                // `x` is a valid alias for `dppx` in Media Queries 4 / Values 4.
                if u.unit.value.eq_ignore_ascii_case("x") {
                    convert_resolution_unit(u.value.value as f32, "x")
                } else {
                    convert_length_unit(u.value.value as f32, &u.unit.value)
                }
            }
            _ => FeatureVal::Invalid,
        },
        ast::MediaFeatureValue::Function(_) => FeatureVal::Invalid,
    }
}

fn convert_length_unit(val: f32, unit: &str) -> FeatureVal {
    if !val.is_finite() || val < 0.0 {
        return FeatureVal::Invalid;
    }
    let px = match unit.to_ascii_lowercase().as_str() {
        "px" => val,
        // Media queries evaluate `em` and `rem` against the initial font size (16px).
        "em" | "rem" => val * 16.0,
        "pt" => val * (96.0 / 72.0),
        "pc" => val * 16.0,
        "in" => val * 96.0,
        "cm" => val * (96.0 / 2.54),
        "mm" => val * (96.0 / 25.4),
        "q" => val * (96.0 / 101.6),
        _ => return FeatureVal::Invalid,
    };
    FeatureVal::LengthPx(px)
}

fn convert_resolution_unit(val: f32, unit: &str) -> FeatureVal {
    if !val.is_finite() || val < 0.0 {
        return FeatureVal::Invalid;
    }
    let dppx = match unit.to_ascii_lowercase().as_str() {
        "dppx" | "x" => val,
        "dpi" => val / 96.0,
        "dpcm" => val * 2.54 / 96.0,
        _ => return FeatureVal::Invalid,
    };
    FeatureVal::ResolutionDppx(dppx)
}

impl MediaCond {
    fn eval(&self, viewport: &Viewport) -> Truth {
        match self {
            Self::Not(inner) => inner.eval(viewport).not(),
            Self::And(items) => {
                let mut acc = Truth::True;
                for item in items {
                    acc = acc.and(item.eval(viewport));
                    if acc == Truth::False {
                        break;
                    }
                }
                acc
            }
            Self::Or(items) => {
                let mut acc = Truth::False;
                for item in items {
                    acc = acc.or(item.eval(viewport));
                    if acc == Truth::True {
                        break;
                    }
                }
                acc
            }
            Self::Feature(f) => f.eval(viewport),
            Self::Unknown => Truth::Unknown,
        }
    }
}

impl FeatureExpr {
    fn eval(&self, viewport: &Viewport) -> Truth {
        match self {
            Self::Invalid => Truth::Unknown,
            Self::Boolean(name) => eval_boolean(name, viewport),
            Self::Plain { name, value } => eval_plain(name, value, viewport),
            Self::Range { name, op, value } => eval_range(name, *op, value, viewport),
            Self::Interval {
                name,
                left_val,
                left_op,
                right_op,
                right_val,
            } => {
                // `left_val left_op feature` <=> `feature left_op.flip() left_val`
                let left = eval_range(name, left_op.flip(), left_val, viewport);
                let right = eval_range(name, *right_op, right_val, viewport);
                left.and(right)
            }
        }
    }
}

fn eval_boolean(name: &str, viewport: &Viewport) -> Truth {
    match name {
        "width" => Truth::from(viewport.width > 0.0),
        "height" => Truth::from(viewport.height > 0.0),
        "aspect-ratio" => Truth::from(viewport.width > 0.0 && viewport.height > 0.0),
        "orientation" => Truth::from(viewport.width > 0.0 || viewport.height > 0.0),
        "prefers-color-scheme" => Truth::True,
        "prefers-reduced-motion" => Truth::from(viewport.prefers_reduced_motion),
        "hover" | "any-hover" => Truth::True,
        "pointer" | "any-pointer" => Truth::True,
        "resolution" => Truth::True,
        _ => Truth::Unknown,
    }
}

fn eval_plain(name: &str, value: &FeatureVal, viewport: &Viewport) -> Truth {
    match name {
        "width" => eval_range("width", RangeOp::Eq, value, viewport),
        "min-width" => eval_range("width", RangeOp::Ge, value, viewport),
        "max-width" => eval_range("width", RangeOp::Le, value, viewport),
        "height" => eval_range("height", RangeOp::Eq, value, viewport),
        "min-height" => eval_range("height", RangeOp::Ge, value, viewport),
        "max-height" => eval_range("height", RangeOp::Le, value, viewport),
        "aspect-ratio" => eval_range("aspect-ratio", RangeOp::Eq, value, viewport),
        "min-aspect-ratio" => eval_range("aspect-ratio", RangeOp::Ge, value, viewport),
        "max-aspect-ratio" => eval_range("aspect-ratio", RangeOp::Le, value, viewport),
        "resolution" => eval_range("resolution", RangeOp::Eq, value, viewport),
        "min-resolution" => eval_range("resolution", RangeOp::Ge, value, viewport),
        "max-resolution" => eval_range("resolution", RangeOp::Le, value, viewport),
        "orientation" => match value {
            FeatureVal::Ident(s) if s == "portrait" => {
                Truth::from(viewport.height >= viewport.width)
            }
            FeatureVal::Ident(s) if s == "landscape" => {
                Truth::from(viewport.width > viewport.height)
            }
            _ => Truth::Unknown,
        },
        "prefers-color-scheme" => match value {
            FeatureVal::Ident(s) if s == "dark" => Truth::from(viewport.prefers_dark),
            FeatureVal::Ident(s) if s == "light" => Truth::from(!viewport.prefers_dark),
            _ => Truth::Unknown,
        },
        "prefers-reduced-motion" => match value {
            FeatureVal::Ident(s) if s == "reduce" => Truth::from(viewport.prefers_reduced_motion),
            FeatureVal::Ident(s) if s == "no-preference" => {
                Truth::from(!viewport.prefers_reduced_motion)
            }
            _ => Truth::Unknown,
        },
        "hover" | "any-hover" => match value {
            FeatureVal::Ident(s) if s == "hover" => Truth::True,
            FeatureVal::Ident(s) if s == "none" => Truth::False,
            _ => Truth::Unknown,
        },
        "pointer" | "any-pointer" => match value {
            FeatureVal::Ident(s) if s == "fine" => Truth::True,
            FeatureVal::Ident(s) if s == "coarse" || s == "none" => Truth::False,
            _ => Truth::Unknown,
        },
        _ => Truth::Unknown,
    }
}

fn eval_range(name: &str, op: RangeOp, value: &FeatureVal, viewport: &Viewport) -> Truth {
    match name {
        "width" => {
            let Some(rhs) = value.as_length_px() else {
                return Truth::Unknown;
            };
            Truth::from(compare_f64(f64::from(viewport.width), op, f64::from(rhs)))
        }
        "height" => {
            let Some(rhs) = value.as_length_px() else {
                return Truth::Unknown;
            };
            Truth::from(compare_f64(f64::from(viewport.height), op, f64::from(rhs)))
        }
        "aspect-ratio" => {
            let Some((a, b)) = value.as_ratio() else {
                return Truth::Unknown;
            };
            if viewport.height <= 0.0 {
                return Truth::False;
            }
            // Cross-multiply `w / h OP a / b` => `w * b OP h * a` so exact
            // integer ratios like 16/9 on 1920x1080 do not suffer float drift.
            let lhs = f64::from(viewport.width) * b;
            let rhs = f64::from(viewport.height) * a;
            Truth::from(compare_f64(lhs, op, rhs))
        }
        "resolution" => {
            let Some(rhs) = value.as_resolution_dppx() else {
                return Truth::Unknown;
            };
            Truth::from(compare_f64(1.0, op, f64::from(rhs)))
        }
        _ => Truth::Unknown,
    }
}

fn compare_f64(lhs: f64, op: RangeOp, rhs: f64) -> bool {
    const EPS: f64 = 1e-4;
    match op {
        RangeOp::Eq => (lhs - rhs).abs() <= EPS,
        RangeOp::Lt => lhs < rhs - EPS,
        RangeOp::Le => lhs <= rhs + EPS,
        RangeOp::Gt => lhs > rhs + EPS,
        RangeOp::Ge => lhs >= rhs - EPS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(w: f32, h: f32) -> Viewport {
        Viewport {
            width: w,
            height: h,
            prefers_dark: false,
            prefers_reduced_motion: false,
        }
    }

    #[test]
    fn media_query_evaluation_table() {
        let desktop = vp(1280.0, 800.0);
        let mobile = Viewport {
            width: 375.0,
            height: 667.0,
            prefers_dark: true,
            prefers_reduced_motion: true,
        };

        let cases: &[(&str, &Viewport, bool)] = &[
            ("", &desktop, true),
            ("all", &desktop, true),
            ("screen", &desktop, true),
            ("only screen", &desktop, true),
            ("print", &desktop, false),
            ("not print", &desktop, true),
            ("not screen", &desktop, false),
            ("speech", &desktop, false),
            ("screen, print", &desktop, true),
            ("print, speech", &desktop, false),
            ("(min-width: 1024px)", &desktop, true),
            ("(min-width: 1024px)", &mobile, false),
            ("(max-width: 600px)", &desktop, false),
            ("(max-width: 600px)", &mobile, true),
            ("(width: 1280px)", &desktop, true),
            ("(height: 800px)", &desktop, true),
            ("(min-width: 80em)", &desktop, true),
            ("(min-width: 81rem)", &desktop, false),
            ("(width >= 600px)", &desktop, true),
            ("(width >= 600px)", &mobile, false),
            ("(600px <= width)", &desktop, true),
            ("(400px < width <= 1280px)", &desktop, true),
            ("(400px < width < 1280px)", &desktop, false),
            ("(400px < width <= 800px)", &mobile, false),
            ("(1400px >= width > 1000px)", &desktop, true),
            ("(orientation: landscape)", &desktop, true),
            ("(orientation: portrait)", &desktop, false),
            ("(orientation: portrait)", &mobile, true),
            ("(orientation: portrait)", &vp(800.0, 800.0), true),
            ("(aspect-ratio: 16/10)", &desktop, true),
            ("(aspect-ratio: 16 / 9)", &vp(1920.0, 1080.0), true),
            ("(min-aspect-ratio: 1/1)", &desktop, true),
            ("(max-aspect-ratio: 1/1)", &mobile, true),
            ("(prefers-color-scheme: light)", &desktop, true),
            ("(prefers-color-scheme: dark)", &desktop, false),
            ("(prefers-color-scheme: dark)", &mobile, true),
            ("(prefers-reduced-motion: no-preference)", &desktop, true),
            ("(prefers-reduced-motion: reduce)", &desktop, false),
            ("(prefers-reduced-motion: reduce)", &mobile, true),
            ("(prefers-reduced-motion)", &desktop, false),
            ("(prefers-reduced-motion)", &mobile, true),
            ("(hover: hover)", &desktop, true),
            ("(hover: none)", &desktop, false),
            ("(any-hover: hover)", &desktop, true),
            ("(pointer: fine)", &desktop, true),
            ("(pointer: coarse)", &desktop, false),
            ("(any-pointer: fine)", &desktop, true),
            ("(resolution: 1dppx)", &desktop, true),
            ("(resolution: 1x)", &desktop, true),
            ("(resolution: 96dpi)", &desktop, true),
            ("(min-resolution: 2dppx)", &desktop, false),
            ("screen and (min-width: 1000px) and (max-height: 900px)", &desktop, true),
            ("(max-width: 500px) or (min-width: 1200px)", &desktop, true),
            ("not (max-width: 500px)", &desktop, true),
            ("not screen and (min-width: 2000px)", &desktop, true),
            ("(unknown-feature: 123px)", &desktop, false),
            ("not (unknown-feature: 123px)", &desktop, false),
            ("(unknown-feature: 123px) or (min-width: 500px)", &desktop, true),
            ("(unknown-feature: 123px) and (min-width: 500px)", &desktop, false),
            ("bad$$$, (min-width: 500px)", &desktop, true),
            ("(400px < width > 200px)", &desktop, false),
            ("(min-width >= 600px)", &desktop, false),
            ("(width: -10px)", &desktop, false),
        ];

        for &(query, viewport, expected) in cases {
            assert_eq!(
                matches_str(query, viewport),
                expected,
                "failed media query `{query}` against {viewport:?}"
            );
        }
    }
}
