//! White space processing (CSS Text 3 §4): collapsing, segment breaks,
//! and removing spaces at line starts and ends, across inline boxes.
//!
//! Inline content is flattened into a token stream first because the rules
//! look across element boundaries (`a <b> b</b>` keeps one space, not two).

use eldur_css::{ComputedStyle, WhiteSpace};
use eldur_dom::NodeId;

use super::{Inline, LayoutBox, Span};

/// Inline content as it comes from the DOM.
#[derive(Debug, Clone)]
pub(crate) enum Raw<'a> {
    Text {
        text: String,
        white_space: WhiteSpace,
    },
    LineBreak,
    Span {
        node: Option<NodeId>,
        style: &'a ComputedStyle,
        children: Vec<Raw<'a>>,
    },
    Atomic(Box<LayoutBox<'a>>),
}

enum Token<'a> {
    Open(Option<NodeId>, &'a ComputedStyle),
    Close,
    /// `collapsible` marks text whose spaces may be removed at line edges.
    Text {
        text: String,
        collapsible: bool,
    },
    Break,
    Atomic(Box<LayoutBox<'a>>),
}

/// Collapses white space in an inline formatting context. Returns no items
/// when nothing but collapsible white space remains.
pub(crate) fn collapse(raw: Vec<Raw<'_>>) -> Vec<Inline<'_>> {
    let mut tokens = Vec::new();
    flatten(raw, &mut tokens);
    let mut tokens = remove_line_edge_spaces(tokens);
    // A run that is only empty spans renders nothing.
    if !tokens
        .iter()
        .any(|t| matches!(t, Token::Text { .. } | Token::Break | Token::Atomic(_)))
    {
        tokens.clear();
    }
    // Between boxes an empty span still has a place on the line: a script can
    // measure it or give it text, and each run keeps its own model of them.
    let keep_empty = tokens.iter().any(|t| matches!(t, Token::Atomic(_)));
    rebuild(tokens, keep_empty)
}

fn flatten<'a>(raw: Vec<Raw<'a>>, out: &mut Vec<Token<'a>>) {
    for item in raw {
        match item {
            Raw::Text { text, white_space } => segment(&text, white_space, out),
            Raw::LineBreak => out.push(Token::Break),
            Raw::Atomic(b) => out.push(Token::Atomic(b)),
            Raw::Span {
                node,
                style,
                children,
            } => {
                out.push(Token::Open(node, style));
                flatten(children, out);
                out.push(Token::Close);
            }
        }
    }
}

/// Phase I of CSS Text 3 §4.1.1 for one text node: collapses within the
/// node and turns preserved segment breaks into [`Token::Break`].
fn segment(text: &str, white_space: WhiteSpace, out: &mut Vec<Token<'_>>) {
    let collapse_spaces = matches!(
        white_space,
        WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine
    );
    let preserve_breaks = !matches!(white_space, WhiteSpace::Normal | WhiteSpace::NoWrap);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let mut line = line.to_owned();
        if !preserve_breaks && lines.peek().is_some() {
            // A segment break in collapsible text becomes a space.
            line.push(' ');
        }
        let piece = if collapse_spaces {
            collapse_run(&line)
        } else {
            expand_tabs(&line)
        };
        if !piece.is_empty() {
            out.push(Token::Text {
                text: piece,
                collapsible: collapse_spaces,
            });
        }
        if preserve_breaks && lines.peek().is_some() {
            out.push(Token::Break);
        }
    }
}

fn is_collapsible_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{c}')
}

fn collapse_run(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for c in text.chars() {
        if is_collapsible_space(c) {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

/// Tabs advance to the next multiple of 8 columns (`tab-size: 8`).
fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let n = 8 - column % 8;
            out.extend(std::iter::repeat_n(' ', n));
            column += n;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// Phase I across nodes and phase II: a collapsible space after another
/// space, at the start of a line or at the end of one, is removed.
fn remove_line_edge_spaces(tokens: Vec<Token<'_>>) -> Vec<Token<'_>> {
    let keep_empty = tokens.iter().any(|t| matches!(t, Token::Atomic(_)));
    let mut out: Vec<Token<'_>> = Vec::with_capacity(tokens.len());
    // Line start counts as "after a space".
    let mut after_space = true;
    for token in tokens {
        match token {
            Token::Text {
                mut text,
                collapsible,
            } => {
                if collapsible && after_space && text.starts_with(' ') {
                    text.remove(0);
                }
                if text.is_empty() {
                    continue;
                }
                after_space = text.ends_with(' ');
                out.push(Token::Text { text, collapsible });
            }
            Token::Break => {
                trim_trailing_space(&mut out);
                after_space = true;
                out.push(Token::Break);
            }
            // Like a glyph: a space after it is kept.
            Token::Atomic(b) => {
                after_space = false;
                out.push(Token::Atomic(b));
            }
            // The model keeps this space for when the span gets text; the
            // `Text` drops it again while the span is empty.
            Token::Close if keep_empty && matches!(out.last(), Some(Token::Open(Some(_), _))) => {
                after_space = false;
                out.push(Token::Close);
            }
            other => out.push(other),
        }
    }
    trim_trailing_space(&mut out);
    out
}

/// Removes a collapsible space at the end of the line built so far.
fn trim_trailing_space(out: &mut Vec<Token<'_>>) {
    for i in (0..out.len()).rev() {
        match &mut out[i] {
            Token::Open(..) | Token::Close => {}
            Token::Break | Token::Atomic(_) => return,
            Token::Text { text, collapsible } => {
                if *collapsible && text.ends_with(' ') {
                    text.pop();
                    if text.is_empty() {
                        out.remove(i);
                    }
                }
                return;
            }
        }
    }
}

fn rebuild(tokens: Vec<Token<'_>>, keep_empty: bool) -> Vec<Inline<'_>> {
    let mut stack: Vec<Span<'_>> = Vec::new();
    let mut root: Vec<Inline<'_>> = Vec::new();
    for token in tokens {
        let item = match token {
            Token::Open(node, style) => {
                stack.push(Span {
                    node,
                    style,
                    children: Vec::new(),
                });
                continue;
            }
            Token::Close => match stack.pop() {
                // Empty inline boxes have no effect on the text.
                Some(span) if span.children.is_empty() && !(keep_empty && span.node.is_some()) => {
                    continue;
                }
                Some(span) => Inline::Span(span),
                None => continue,
            },
            Token::Text { text, .. } => Inline::Text(text),
            Token::Break => Inline::LineBreak,
            Token::Atomic(b) => Inline::Atomic(b),
        };
        let parent = stack.last_mut().map_or(&mut root, |s| &mut s.children);
        // Adjacent text (from split text nodes) is merged.
        if let (Inline::Text(new), Some(Inline::Text(prev))) = (&item, parent.last_mut()) {
            prev.push_str(new);
            continue;
        }
        parent.push(item);
    }
    root
}
