//! Prints a [`Document`] as formatted QML.

use std::fmt::Write as _;

use crate::{Document, Member, Object, Value};

const INDENT: &str = "    ";

/// Quotes `s` as a QML/JavaScript double-quoted string literal.
#[must_use]
pub fn quote_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_number(n: f64) -> String {
    if !n.is_finite() {
        // QML has no literal for these; the expression keeps the binding valid.
        return if n.is_nan() {
            "NaN".to_owned()
        } else if n > 0.0 {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    if n == n.trunc() && n.abs() < 1e15 {
        // `-0` prints as `0`.
        return format!("{}", n as i64);
    }
    // Shortest representation that round-trips, then trim float noise
    // from CSS arithmetic (0.30000000000000004 -> 0.3) at 6 decimals.
    let rounded = (n * 1e6).round() / 1e6;
    let s = format!("{rounded}");
    if s.contains('e') { format!("{n}") } else { s }
}

/// Prints the whole document.
#[must_use]
pub fn write_document(doc: &Document) -> String {
    let mut out = String::new();
    for line in &doc.header {
        if line.is_empty() {
            out.push_str("//\n");
        } else {
            let _ = writeln!(out, "// {line}");
        }
    }
    if !doc.header.is_empty() {
        out.push('\n');
    }
    for pragma in &doc.pragmas {
        let _ = writeln!(out, "pragma {pragma}");
    }
    if !doc.pragmas.is_empty() {
        out.push('\n');
    }
    let mut imports = doc.imports.clone();
    // Modules before paths (quoted), each alphabetical.
    imports.sort_by(|a, b| {
        (a.uri.starts_with('"'), &a.uri, &a.version, &a.alias).cmp(&(
            b.uri.starts_with('"'),
            &b.uri,
            &b.version,
            &b.alias,
        ))
    });
    imports.dedup();
    for import in &imports {
        out.push_str("import ");
        out.push_str(&import.uri);
        if let Some(v) = &import.version {
            out.push(' ');
            out.push_str(v);
        }
        if let Some(a) = &import.alias {
            out.push_str(" as ");
            out.push_str(a);
        }
        out.push('\n');
    }
    if !imports.is_empty() {
        out.push('\n');
    }
    write_object(&mut out, &doc.root, 0);
    out.push('\n');
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str(INDENT);
    }
}

/// Order of member kinds inside an object body.
fn rank(m: &Member) -> u8 {
    match m {
        Member::PropertyDecl { .. } => 0,
        Member::Signal { .. } => 1,
        Member::Property { .. } => 2,
        Member::Behavior { .. } => 3,
        Member::Function { .. } => 4,
        Member::Handler { .. } => 5,
        // Comments travel with the children they annotate.
        Member::Child(_) | Member::Comment(_) => 6,
    }
}

fn write_object(out: &mut String, obj: &Object, level: usize) {
    out.push_str(&obj.type_name);
    if obj.id.is_none() && obj.members.is_empty() {
        out.push_str(" {}");
        return;
    }
    out.push_str(" {\n");
    if let Some(id) = &obj.id {
        indent(out, level + 1);
        let _ = writeln!(out, "id: {id}");
    }
    let mut members: Vec<&Member> = obj.members.iter().collect();
    members.sort_by_key(|m| rank(m)); // stable: keeps given order per kind
    let mut prev_rank = None;
    let mut after_comment = false;
    for m in members {
        let r = rank(m);
        // A blank line between kinds, and before every child object, as
        // hand-written QML does; a comment takes the blank line of the
        // child it annotates.
        let blank = match prev_rank {
            None => obj.id.is_some() && r >= 3,
            Some(_) if after_comment => false,
            Some(p) => {
                p != r
                    || matches!(
                        m,
                        Member::Child(_) | Member::Function { .. } | Member::Comment(_)
                    )
            }
        };
        if blank {
            out.push('\n');
        }
        prev_rank = Some(r);
        after_comment = matches!(m, Member::Comment(_));
        write_member(out, m, level + 1);
    }
    indent(out, level);
    out.push('}');
}

fn write_member(out: &mut String, m: &Member, level: usize) {
    indent(out, level);
    match m {
        Member::PropertyDecl {
            ty,
            name,
            value,
            readonly,
            required,
        } => {
            if *required {
                out.push_str("required ");
            }
            if *readonly {
                out.push_str("readonly ");
            }
            let _ = write!(out, "property {ty} {name}");
            if let Some(v) = value {
                out.push_str(": ");
                write_value(out, v, level);
            }
            out.push('\n');
        }
        Member::Signal { name, params } => {
            let params: Vec<String> = params.iter().map(|(t, n)| format!("{t} {n}")).collect();
            let _ = writeln!(out, "signal {name}({})", params.join(", "));
        }
        Member::Property { name, value } => {
            let _ = write!(out, "{name}: ");
            write_value(out, value, level);
            out.push('\n');
        }
        Member::Behavior {
            property,
            animation,
        } => {
            let _ = writeln!(out, "Behavior on {property} {{");
            indent(out, level + 1);
            write_object(out, animation, level + 1);
            out.push('\n');
            indent(out, level);
            out.push_str("}\n");
        }
        Member::Function { name, params, body } => {
            let _ = write!(out, "function {name}({}) ", params.join(", "));
            write_block(out, body, level);
            out.push('\n');
        }
        Member::Handler { name, params, body } => {
            let _ = write!(out, "{name}: ");
            if params.is_empty() && body.len() == 1 && !body[0].trim_end().ends_with(';') {
                // A single expression: `onClicked: root.toggle()`.
                out.push_str(body[0].trim());
            } else if params.is_empty() {
                write_block(out, body, level);
            } else {
                let _ = write!(out, "({}) => ", params.join(", "));
                write_block(out, body, level);
            }
            out.push('\n');
        }
        Member::Child(child) => {
            write_object(out, child, level);
            out.push('\n');
        }
        Member::Comment(text) => {
            let _ = writeln!(out, "// {text}");
        }
    }
}

/// `{`, the lines re-indented one level deeper, `}`. Lines keep their own
/// relative indentation (the common leading whitespace is removed).
fn write_block(out: &mut String, body: &[String], level: usize) {
    if body.iter().all(|l| l.trim().is_empty()) {
        out.push_str("{}");
        return;
    }
    out.push_str("{\n");
    let lines: Vec<&str> = body.iter().flat_map(|l| l.lines()).collect();
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    for line in lines {
        if line.trim().is_empty() {
            out.push('\n');
            continue;
        }
        indent(out, level + 1);
        out.push_str(line.get(common..).unwrap_or(line).trim_end());
        out.push('\n');
    }
    indent(out, level);
    out.push('}');
}

fn write_value(out: &mut String, v: &Value, level: usize) {
    match v {
        Value::Expr(e) => out.push_str(e),
        Value::String(s) => out.push_str(&quote_string(s)),
        Value::Number(n) => out.push_str(&format_number(*n)),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Color(c) => out.push_str(&quote_string(&c.to_qml_string())),
        Value::List(items) => {
            let simple = items
                .iter()
                .all(|i| !matches!(i, Value::Object(_) | Value::Block(_)));
            if simple {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_value(out, item, level);
                }
                out.push(']');
            } else {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    indent(out, level + 1);
                    write_value(out, item, level + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, level);
                out.push(']');
            }
        }
        Value::Object(o) => write_object(out, o, level),
        Value::Block(lines) => write_block(out, lines, level),
    }
}
