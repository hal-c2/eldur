//! Tokenisation of CSS declaration value strings via `swc_css_parser` and a
//! lightweight backtracking cursor over [`ComponentValue`] slices.

use swc_common::{BytePos, input::StringInput};
use swc_css_ast::{ComponentValue, Function, ListOfComponentValues, NumberType, SimpleBlock, Token};
use swc_css_parser::{error::ErrorKind, parse_string_input, parser::ParserConfig};

/// Maximum nesting depth of functions and parenthesised/bracketed blocks.
/// Protects recursive value parsers against stack overflow on hostile input.
pub(crate) const MAX_NESTING_DEPTH: usize = 32;

/// Parses a CSS value text (already `var()`-substituted, comments allowed) into
/// top-level [`ComponentValue`]s.
///
/// Rejects lexing/syntax errors, unmatched closing brackets, semicolons, and
/// excessive nesting so downstream property parsers only see well-formed trees.
pub(crate) fn parse_component_values(value: &str) -> Result<Vec<ComponentValue>, String> {
    let len = u32::try_from(value.len()).map_err(|_| "CSS value exceeds maximum length".to_owned())?;
    let input = StringInput::new(value, BytePos(1), BytePos(1 + len));
    let mut errors = Vec::new();
    let list: ListOfComponentValues =
        parse_string_input(input, None, ParserConfig::default(), &mut errors)
            .map_err(|e| format!("CSS parse error: {}", e.message()))?;

    for err in &errors {
        if *err.kind() != ErrorKind::Ignore {
            return Err(format!("CSS syntax error: {}", err.message()));
        }
    }

    validate_cvs(&list.children, 0)?;
    Ok(list.children)
}

fn validate_cvs(cvs: &[ComponentValue], depth: usize) -> Result<(), String> {
    if depth > MAX_NESTING_DEPTH {
        return Err("CSS value exceeds maximum nesting depth".to_owned());
    }
    for cv in cvs {
        match cv {
            ComponentValue::PreservedToken(ts) => match &ts.token {
                Token::BadString { .. } => return Err("bad string token".to_owned()),
                Token::BadUrl { .. } => return Err("bad url token".to_owned()),
                Token::Semi => return Err("unexpected ';' in property value".to_owned()),
                Token::RParen | Token::RBracket | Token::RBrace => {
                    return Err("unmatched closing bracket in property value".to_owned());
                }
                Token::CDO | Token::CDC => {
                    return Err("unexpected HTML comment delimiter in property value".to_owned());
                }
                _ => {}
            },
            ComponentValue::Function(func) => {
                validate_cvs(&func.value, depth + 1)?;
            }
            ComponentValue::SimpleBlock(block) => {
                if block.name.token == Token::LBrace {
                    return Err("unexpected '{...}' block in property value".to_owned());
                }
                validate_cvs(&block.value, depth + 1)?;
            }
            _ => return Err("unexpected AST node in component value list".to_owned()),
        }
    }
    Ok(())
}

/// Trims leading and trailing [`Token::WhiteSpace`] items from a slice.
pub(crate) fn trim_ws(mut cvs: &[ComponentValue]) -> &[ComponentValue] {
    while let Some(first) = cvs.first() {
        if is_ws(first) {
            cvs = &cvs[1..];
        } else {
            break;
        }
    }
    while let Some(last) = cvs.last() {
        if is_ws(last) {
            cvs = &cvs[..cvs.len() - 1];
        } else {
            break;
        }
    }
    cvs
}

/// Returns `true` when `cv` is a whitespace token.
#[inline]
pub(crate) fn is_ws(cv: &ComponentValue) -> bool {
    matches!(
        cv,
        ComponentValue::PreservedToken(ts) if matches!(ts.token, Token::WhiteSpace { .. })
    )
}

/// Returns `true` when `cv` is a top-level comma token `,`.
#[inline]
pub(crate) fn is_comma(cv: &ComponentValue) -> bool {
    matches!(
        cv,
        ComponentValue::PreservedToken(ts) if ts.token == Token::Comma
    )
}

/// Returns `true` when `cv` is a `/` delimiter token.
#[inline]
pub(crate) fn is_slash(cv: &ComponentValue) -> bool {
    matches!(
        cv,
        ComponentValue::PreservedToken(ts) if matches!(ts.token, Token::Delim { value: '/' })
    )
}

/// Splits `cvs` on top-level commas, trimming whitespace from each item and
/// rejecting empty items (leading, trailing, or consecutive commas, or empty input).
pub(crate) fn split_comma_separated(
    cvs: &[ComponentValue],
) -> Result<Vec<&[ComponentValue]>, String> {
    let trimmed = trim_ws(cvs);
    if trimmed.is_empty() {
        return Err("empty value".to_owned());
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, cv) in trimmed.iter().enumerate() {
        if is_comma(cv) {
            let segment = trim_ws(&trimmed[start..idx]);
            if segment.is_empty() {
                return Err("empty comma-separated item".to_owned());
            }
            parts.push(segment);
            start = idx + 1;
        }
    }
    let tail = trim_ws(&trimmed[start..]);
    if tail.is_empty() {
        return Err("trailing comma in value".to_owned());
    }
    parts.push(tail);
    Ok(parts)
}

/// Splits `cvs` on top-level `/` delimiters, trimming whitespace from each part
/// and rejecting empty segments around `/`.
pub(crate) fn split_slash_separated(
    cvs: &[ComponentValue],
) -> Result<Vec<&[ComponentValue]>, String> {
    let trimmed = trim_ws(cvs);
    if trimmed.is_empty() {
        return Err("empty value".to_owned());
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, cv) in trimmed.iter().enumerate() {
        if is_slash(cv) {
            let segment = trim_ws(&trimmed[start..idx]);
            if segment.is_empty() {
                return Err("empty '/' segment".to_owned());
            }
            parts.push(segment);
            start = idx + 1;
        }
    }
    let tail = trim_ws(&trimmed[start..]);
    if tail.is_empty() {
        return Err("trailing '/' in value".to_owned());
    }
    parts.push(tail);
    Ok(parts)
}

/// Splits `cvs` on top-level whitespace into non-whitespace component values.
pub(crate) fn non_ws_items(cvs: &[ComponentValue]) -> Vec<&ComponentValue> {
    cvs.iter().filter(|cv| !is_ws(cv)).collect()
}

/// Serialises a slice of [`ComponentValue`]s back to canonical, comment-free CSS text.
///
/// Adjacent non-whitespace tokens that were separated only by a CSS comment in
/// the original source (for example `1px/*c*/solid`) get a separating space so
/// re-tokenising the result preserves token boundaries.
pub(crate) fn serialize_cvs(cvs: &[ComponentValue]) -> String {
    let mut out = String::new();
    write_cvs(trim_ws(cvs), &mut out, 0);
    out
}

/// Serialises a single [`ComponentValue`] to CSS text.
pub(crate) fn serialize_cv(cv: &ComponentValue) -> String {
    let mut out = String::new();
    write_cv(cv, &mut out, 0);
    out
}

fn needs_separator_before(prev_was_wordlike: bool, next: &ComponentValue) -> bool {
    if !prev_was_wordlike {
        return false;
    }
    match next {
        ComponentValue::Function(_) => true,
        ComponentValue::SimpleBlock(_) => false,
        ComponentValue::PreservedToken(ts) => matches!(
            ts.token,
            Token::Ident { .. }
                | Token::Function { .. }
                | Token::AtKeyword { .. }
                | Token::Hash { .. }
                | Token::Url { .. }
                | Token::Number { .. }
                | Token::Percentage { .. }
                | Token::Dimension { .. }
                | Token::Delim {
                    value: '-' | '+' | '.' | '#' | '@' | '%'
                }
        ),
        _ => false,
    }
}

fn is_wordlike_end(cv: &ComponentValue) -> bool {
    match cv {
        ComponentValue::Function(_) | ComponentValue::SimpleBlock(_) => false,
        ComponentValue::PreservedToken(ts) => matches!(
            ts.token,
            Token::Ident { .. }
                | Token::AtKeyword { .. }
                | Token::Hash { .. }
                | Token::Number { .. }
                | Token::Percentage { .. }
                | Token::Dimension { .. }
                | Token::Delim {
                    value: '-' | '+' | '.' | '#' | '@'
                }
        ),
        _ => false,
    }
}

fn write_cvs(cvs: &[ComponentValue], out: &mut String, depth: usize) {
    if depth > MAX_NESTING_DEPTH {
        return;
    }
    let mut prev_ws = false;
    let mut prev_wordlike = false;
    for cv in cvs {
        if is_ws(cv) {
            if !prev_ws && !out.is_empty() {
                out.push(' ');
                prev_ws = true;
                prev_wordlike = false;
            }
            continue;
        }
        if !prev_ws && needs_separator_before(prev_wordlike, cv) {
            out.push(' ');
        }
        write_cv(cv, out, depth);
        prev_ws = false;
        prev_wordlike = is_wordlike_end(cv);
    }
}

fn write_cv(cv: &ComponentValue, out: &mut String, depth: usize) {
    if depth > MAX_NESTING_DEPTH {
        return;
    }
    match cv {
        ComponentValue::Function(func) => {
            write_function(func, out, depth);
        }
        ComponentValue::SimpleBlock(block) => {
            write_simple_block(block, out, depth);
        }
        ComponentValue::PreservedToken(ts) => match &ts.token {
            Token::Ident { value, .. } => {
                write_escaped_ident(value, out);
            }
            Token::Function { value, .. } => {
                write_escaped_ident(value, out);
                out.push('(');
            }
            Token::AtKeyword { value, .. } => {
                out.push('@');
                write_escaped_ident(value, out);
            }
            Token::Hash { value, .. } => {
                out.push('#');
                out.push_str(value);
            }
            Token::String { value, .. } => {
                write_css_string(value, out);
            }
            Token::Url { value, .. } => {
                out.push_str("url(");
                write_css_string(value, out);
                out.push(')');
            }
            Token::Delim { value } => {
                out.push(*value);
            }
            Token::Number { value, raw, .. } => {
                if !raw.is_empty() {
                    out.push_str(raw);
                } else {
                    out.push_str(&value.to_string());
                }
            }
            Token::Percentage { value, raw } => {
                if !raw.is_empty() {
                    out.push_str(raw);
                } else {
                    out.push_str(&value.to_string());
                }
                out.push('%');
            }
            Token::Dimension { dimension } => {
                if !dimension.raw_value.is_empty() {
                    out.push_str(&dimension.raw_value);
                } else {
                    out.push_str(&dimension.value.to_string());
                }
                out.push_str(&dimension.unit);
            }
            Token::WhiteSpace { .. } => {
                out.push(' ');
            }
            Token::Colon => out.push(':'),
            Token::Semi => out.push(';'),
            Token::Comma => out.push(','),
            Token::LBracket => out.push('['),
            Token::RBracket => out.push(']'),
            Token::LParen => out.push('('),
            Token::RParen => out.push(')'),
            Token::LBrace => out.push('{'),
            Token::RBrace => out.push('}'),
            Token::BadString { .. } | Token::BadUrl { .. } | Token::CDO | Token::CDC => {}
        },
        _ => {}
    }
}

fn write_function(func: &Function, out: &mut String, depth: usize) {
    match &func.name {
        swc_css_ast::FunctionName::Ident(id) => write_escaped_ident(&id.value, out),
        swc_css_ast::FunctionName::DashedIdent(id) => {
            out.push_str("--");
            write_escaped_ident(&id.value, out);
        }
    }
    out.push('(');
    write_cvs(trim_ws(&func.value), out, depth + 1);
    out.push(')');
}

fn write_simple_block(block: &SimpleBlock, out: &mut String, depth: usize) {
    let close = match block.name.token {
        Token::LBracket => {
            out.push('[');
            ']'
        }
        Token::LParen => {
            out.push('(');
            ')'
        }
        Token::LBrace => {
            out.push('{');
            '}'
        }
        _ => return,
    };
    write_cvs(trim_ws(&block.value), out, depth + 1);
    out.push(close);
}

fn write_css_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\a "),
            '\r' => out.push_str("\\d "),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_escaped_ident(s: &str, out: &mut String) {
    let mut chars = s.chars().peekable();
    let mut first = true;
    while let Some(ch) = chars.next() {
        let safe = if first {
            ch.is_ascii_alphabetic()
                || ch == '_'
                || ch > '\x7f'
                || (ch == '-'
                    && chars
                        .peek()
                        .is_some_and(|&next| {
                            next.is_ascii_alphabetic() || next == '_' || next == '-' || next > '\x7f'
                        }))
        } else {
            ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch > '\x7f'
        };
        if safe {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
        first = false;
    }
}

/// Returns `true` if `ident` is a CSS-wide keyword (which is never allowed as a `<custom-ident>`).
pub(crate) fn is_css_wide_keyword(ident: &str) -> bool {
    ident.eq_ignore_ascii_case("inherit")
        || ident.eq_ignore_ascii_case("initial")
        || ident.eq_ignore_ascii_case("unset")
        || ident.eq_ignore_ascii_case("revert")
        || ident.eq_ignore_ascii_case("revert-layer")
}

/// Backtracking cursor over a slice of [`ComponentValue`]s.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'a> {
    items: &'a [ComponentValue],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(items: &'a [ComponentValue]) -> Self {
        Self { items, pos: 0 }
    }

    #[inline]
    pub(crate) fn save(&self) -> usize {
        self.pos
    }

    #[inline]
    pub(crate) fn restore(&mut self, saved: usize) {
        self.pos = saved;
    }

    #[inline]
    pub(crate) fn remaining(&mut self) -> &'a [ComponentValue] {
        self.skip_ws();
        &self.items[self.pos..]
    }

    #[inline]
    pub(crate) fn skip_ws(&mut self) {
        while let Some(cv) = self.items.get(self.pos) {
            if is_ws(cv) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    #[inline]
    pub(crate) fn is_eof(&mut self) -> bool {
        self.skip_ws();
        self.pos >= self.items.len()
    }

    pub(crate) fn expect_eof(&mut self) -> Result<(), String> {
        if self.is_eof() {
            Ok(())
        } else {
            Err("unexpected trailing tokens".to_owned())
        }
    }

    /// Peeks the next non-whitespace [`ComponentValue`].
    #[inline]
    pub(crate) fn peek(&mut self) -> Option<&'a ComponentValue> {
        self.skip_ws();
        self.items.get(self.pos)
    }

    /// Peeks without skipping whitespace (used where CSS grammar is whitespace-sensitive, e.g. `calc()`).
    #[inline]
    pub(crate) fn peek_raw(&self) -> Option<&'a ComponentValue> {
        self.items.get(self.pos)
    }

    /// Consumes and returns the next non-whitespace [`ComponentValue`].
    #[inline]
    pub(crate) fn next_cv(&mut self) -> Option<&'a ComponentValue> {
        self.skip_ws();
        let cv = self.items.get(self.pos)?;
        self.pos += 1;
        Some(cv)
    }

    pub(crate) fn eat_slash(&mut self) -> bool {
        self.eat_delim('/')
    }

    pub(crate) fn eat_delim(&mut self, expected: char) -> bool {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && matches!(ts.token, Token::Delim { value } if value == expected)
        {
            self.pos += 1;
            return true;
        }
        false
    }

    pub(crate) fn eat_ident(&mut self) -> Option<&'a str> {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::Ident { value, .. } = &ts.token
        {
            self.pos += 1;
            return Some(value.as_ref());
        }
        None
    }

    pub(crate) fn eat_ident_eq(&mut self, expected_ascii_lower: &str) -> bool {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::Ident { value, .. } = &ts.token
            && value.eq_ignore_ascii_case(expected_ascii_lower)
        {
            self.pos += 1;
            return true;
        }
        false
    }

    /// Consumes a `<custom-ident>` (an identifier that is not a CSS-wide keyword).
    pub(crate) fn eat_custom_ident(&mut self) -> Option<&'a str> {
        let saved = self.save();
        if let Some(id) = self.eat_ident() {
            if !is_css_wide_keyword(id) {
                return Some(id);
            }
            self.restore(saved);
        }
        None
    }

    pub(crate) fn eat_string(&mut self) -> Option<&'a str> {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::String { value, .. } = &ts.token
        {
            self.pos += 1;
            return Some(value.as_ref());
        }
        None
    }

    /// Consumes a CSS `url(...)` token (either unquoted `Token::Url` or quoted `Function("url")`).
    ///
    /// Returns `Ok(None)` if the next token is not a `url(...)`, `Ok(Some(url))`
    /// if valid, or `Err` if it is a malformed `url(...)` function call.
    pub(crate) fn eat_url(&mut self) -> Result<Option<String>, String> {
        self.skip_ws();
        match self.items.get(self.pos) {
            Some(ComponentValue::PreservedToken(ts)) => {
                if let Token::Url { value, .. } = &ts.token {
                    self.pos += 1;
                    return Ok(Some(value.to_string()));
                }
                Ok(None)
            }
            Some(ComponentValue::Function(func)) if func.name.as_str().eq_ignore_ascii_case("url") => {
                self.pos += 1;
                let mut inner = Cursor::new(&func.value);
                if inner.is_eof() {
                    return Ok(Some(String::new()));
                }
                let s = inner
                    .eat_string()
                    .ok_or_else(|| "url() function requires a string or raw URL".to_owned())?;
                // Optional CSS Values 4 url-modifiers (idents or functions) may follow.
                while !inner.is_eof() {
                    if inner.eat_ident().is_none() && inner.eat_function().is_none() {
                        return Err("invalid modifier in url()".to_owned());
                    }
                }
                Ok(Some(s.to_owned()))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn eat_number_raw(&mut self) -> Option<(f64, NumberType)> {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::Number {
                value, type_flag, ..
            } = &ts.token
            && value.is_finite()
        {
            self.pos += 1;
            return Some((*value, *type_flag));
        }
        None
    }

    pub(crate) fn eat_number(&mut self) -> Option<f32> {
        let saved = self.save();
        if let Some((v, _)) = self.eat_number_raw() {
            let f = v as f32;
            if f.is_finite() {
                return Some(f);
            }
            self.restore(saved);
        }
        None
    }

    pub(crate) fn eat_integer_token(&mut self) -> Option<i32> {
        let saved = self.save();
        if let Some((v, NumberType::Integer)) = self.eat_number_raw() {
            let rounded = v.round();
            if (v - rounded).abs() < 1e-6 && rounded >= f64::from(i32::MIN) && rounded <= f64::from(i32::MAX) {
                return Some(rounded as i32);
            }
            self.restore(saved);
        }
        None
    }

    pub(crate) fn eat_percentage(&mut self) -> Option<f32> {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::Percentage { value, .. } = &ts.token
        {
            let f = *value as f32;
            if f.is_finite() {
                self.pos += 1;
                return Some(f);
            }
        }
        None
    }

    pub(crate) fn eat_dimension(&mut self) -> Option<(f32, &'a str, NumberType)> {
        self.skip_ws();
        if let Some(ComponentValue::PreservedToken(ts)) = self.items.get(self.pos)
            && let Token::Dimension { dimension } = &ts.token
        {
            let f = dimension.value as f32;
            if f.is_finite() {
                self.pos += 1;
                return Some((f, dimension.unit.as_ref(), dimension.type_flag));
            }
        }
        None
    }

    pub(crate) fn eat_function(&mut self) -> Option<(&'a str, &'a [ComponentValue])> {
        self.skip_ws();
        if let Some(ComponentValue::Function(func)) = self.items.get(self.pos) {
            self.pos += 1;
            return Some((func.name.as_str(), &func.value));
        }
        None
    }

    pub(crate) fn eat_bracket_block(&mut self) -> Option<&'a [ComponentValue]> {
        self.skip_ws();
        if let Some(ComponentValue::SimpleBlock(block)) = self.items.get(self.pos)
            && block.name.token == Token::LBracket
        {
            self.pos += 1;
            return Some(&block.value);
        }
        None
    }

    pub(crate) fn eat_paren_block(&mut self) -> Option<&'a [ComponentValue]> {
        self.skip_ws();
        if let Some(ComponentValue::SimpleBlock(block)) = self.items.get(self.pos)
            && block.name.token == Token::LParen
        {
            self.pos += 1;
            return Some(&block.value);
        }
        None
    }
}
