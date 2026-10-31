use oxc_span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub span: Span,
    pub replacement: String,
}

/// Compositional span-based text editor over an immutable JS source slice.
///
/// Inner AST nodes record their edits first. When an outer AST node needs to
/// wrap or relocate a child node's text, [`Self::extract_and_consume`] applies
/// and removes any edits nested inside the child's span, preventing overlapping
/// edits while preserving author formatting and comments inside sub-expressions.
#[derive(Debug)]
pub(crate) struct SpanEditor<'a> {
    source: &'a str,
    edits: Vec<TextEdit>,
}

impl<'a> SpanEditor<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            edits: Vec::new(),
        }
    }

    pub(crate) fn slice(&self, span: Span) -> &'a str {
        self.source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }

    /// Replaces `span` with `replacement`, discarding any previously queued
    /// edits contained within `span` so outer replacements supersede inner ones.
    pub(crate) fn replace(&mut self, span: Span, replacement: impl Into<String>) {
        if !span.is_empty() {
            self.edits.retain(|e| !is_nested_inside(e.span, span));
        }
        self.edits.push(TextEdit {
            span,
            replacement: replacement.into(),
        });
    }

    /// Removes a standalone statement `span`, including its own line's leading
    /// indentation and trailing newline when nothing else shares that line.
    pub(crate) fn remove_statement_line(&mut self, span: Span) {
        let bytes = self.source.as_bytes();
        let mut l = (span.start as usize).min(bytes.len());
        while l > 0 && matches!(bytes[l - 1], b' ' | b'\t') {
            l -= 1;
        }
        if l == 0 || bytes[l - 1] == b'\n' {
            let mut r = (span.end as usize).min(bytes.len());
            while r < bytes.len() && matches!(bytes[r], b' ' | b'\t') {
                r += 1;
            }
            if r < bytes.len() && bytes[r] == b'\r' {
                r += 1;
            }
            if r < bytes.len() && bytes[r] == b'\n' {
                r += 1;
                if let (Ok(ls), Ok(rs)) = (u32::try_from(l), u32::try_from(r)) {
                    self.replace(Span::new(ls, rs), "");
                    return;
                }
            } else if r == bytes.len()
                && let (Ok(ls), Ok(rs)) = (u32::try_from(l), u32::try_from(r))
            {
                self.replace(Span::new(ls, rs), "");
                return;
            }
        }
        self.replace(span, "");
    }

    /// Renders `span` with all edits currently queued inside it applied, and
    /// removes those consumed edits from the editor.
    pub(crate) fn extract_and_consume(&mut self, span: Span) -> String {
        let mut inner = Vec::new();
        let mut remaining = Vec::with_capacity(self.edits.len());
        for edit in self.edits.drain(..) {
            if is_nested_inside(edit.span, span) {
                inner.push(edit);
            } else {
                remaining.push(edit);
            }
        }
        self.edits = remaining;
        apply_edits_in_range(self.source, span, inner)
    }

    /// Applies all remaining edits to the entire source string.
    pub(crate) fn finish(self) -> String {
        let full = Span::new(0, u32::try_from(self.source.len()).unwrap_or(u32::MAX));
        apply_edits_in_range(self.source, full, self.edits)
    }
}

fn is_nested_inside(inner: Span, outer: Span) -> bool {
    if outer.is_empty() {
        return inner == outer;
    }
    if inner.is_empty() {
        // Keep boundary point-insertions attached to outer range only when strictly interior.
        inner.start > outer.start && inner.end < outer.end
    } else {
        outer.start <= inner.start && inner.end <= outer.end
    }
}

fn apply_edits_in_range(source: &str, range: Span, mut edits: Vec<TextEdit>) -> String {
    let start = (range.start as usize).min(source.len());
    let end = (range.end as usize).min(source.len()).max(start);
    if edits.is_empty() {
        return source.get(start..end).unwrap_or("").to_string();
    }

    // Stable sort preserves insertion order for adjacent zero-length insertions.
    edits.sort_by_key(|e| (e.span.start, e.span.end));

    let mut out = String::with_capacity(end - start);
    let mut cursor = start;
    for edit in edits {
        let s = edit.span.start as usize;
        let e = edit.span.end as usize;
        if s < cursor || e > end || s > e {
            debug_assert!(
                false,
                "overlapping or out-of-range span edit: {s}..{e} (cursor={cursor}, end={end})"
            );
            continue;
        }
        if let Some(prefix) = source.get(cursor..s) {
            out.push_str(prefix);
        }
        out.push_str(&edit.replacement);
        cursor = e;
    }
    if let Some(tail) = source.get(cursor..end) {
        out.push_str(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_extract_and_consume_preserves_formatting_and_comments() {
        let src = "el.style.color = /* keep */ document.getElementById('x').value;";
        let mut ed = SpanEditor::new(src);
        // Replace `document.getElementById('x').value` (bytes 28..62) with `x.text`.
        ed.replace(Span::new(28, 62), "x.text");
        let rhs = ed.extract_and_consume(Span::new(17, 62));
        assert_eq!(rhs, "/* keep */ x.text");
        ed.replace(
            Span::new(0, 62),
            format!("Eldur.setStyle(el, \"color\", {rhs})"),
        );
        assert_eq!(
            ed.finish(),
            "Eldur.setStyle(el, \"color\", /* keep */ x.text);"
        );
    }
}
