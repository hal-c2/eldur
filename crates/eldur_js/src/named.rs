//! HTML's named access on the window: a name no script declares reaches the element
//! with that id. Written back into the source as the lookup it means, so that
//! the analysis and the lowering see one thing and no use of the name can slip past
//! them to a QML id, which is a raw item and not always the page's element.

use std::collections::{BTreeSet, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, IdentifierReference, ObjectProperty, UnaryExpression};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{IsGlobalReference, Scoping};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::UnaryOperator;

use crate::parse::{ParsedScript, parse_script};
use crate::resolve::{collect_window_property_writes, unwrap_parens};
use crate::{Analysis, ScriptInput, ScriptKind};

/// What the global object has before named access is asked: these win over an
/// element of that name.
pub(crate) const WINDOW_NAMES: &[&str] = &[
    "AbortController",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Blob",
    "Boolean",
    "CSS",
    "CSSStyleSheet",
    "CustomEvent",
    "DOMParser",
    "DataView",
    "Date",
    "Document",
    "DocumentFragment",
    "Element",
    "Error",
    "EvalError",
    "Event",
    "EventSource",
    "EventTarget",
    "File",
    "FileReader",
    "Float32Array",
    "Float64Array",
    "FocusEvent",
    "FormData",
    "Function",
    "HTMLElement",
    "Headers",
    "Image",
    "InputEvent",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "IntersectionObserver",
    "Intl",
    "JSON",
    "KeyboardEvent",
    "Map",
    "Math",
    "MouseEvent",
    "MutationObserver",
    "NaN",
    "Infinity",
    "Node",
    "NodeList",
    "Notification",
    "Number",
    "Object",
    "Option",
    "PointerEvent",
    "Promise",
    "Proxy",
    "Qt",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Request",
    "ResizeObserver",
    "Response",
    "Set",
    "SharedArrayBuffer",
    "Storage",
    "String",
    "Symbol",
    "SyntaxError",
    "Text",
    "TextDecoder",
    "TextEncoder",
    "TypeError",
    "URIError",
    "URL",
    "URLSearchParams",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "WebSocket",
    "WheelEvent",
    "Window",
    "Worker",
    "XMLHttpRequest",
    "alert",
    "arguments",
    "atob",
    "blur",
    "btoa",
    "caches",
    "cancelAnimationFrame",
    "cancelIdleCallback",
    "clearInterval",
    "clearTimeout",
    "close",
    "closed",
    "confirm",
    "console",
    "crypto",
    "customElements",
    "decodeURI",
    "decodeURIComponent",
    "devicePixelRatio",
    "document",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "eval",
    "event",
    "external",
    "fetch",
    "focus",
    "frames",
    "getComputedStyle",
    "getSelection",
    "globalThis",
    "history",
    "indexedDB",
    "innerHeight",
    "innerWidth",
    "isFinite",
    "isNaN",
    "length",
    "localStorage",
    "location",
    "matchMedia",
    "module",
    "moveBy",
    "moveTo",
    "name",
    "navigator",
    "open",
    "opener",
    "origin",
    "outerHeight",
    "outerWidth",
    "pageXOffset",
    "pageYOffset",
    "parent",
    "parseFloat",
    "parseInt",
    "performance",
    "postMessage",
    "print",
    "prompt",
    "qsTr",
    "queueMicrotask",
    "requestAnimationFrame",
    "requestIdleCallback",
    "resizeBy",
    "resizeTo",
    "screen",
    "screenLeft",
    "screenTop",
    "screenX",
    "screenY",
    "scroll",
    "scrollBy",
    "scrollTo",
    "scrollX",
    "scrollY",
    "self",
    "sessionStorage",
    "setInterval",
    "setTimeout",
    "speechSynthesis",
    "status",
    "stop",
    "structuredClone",
    "top",
    "undefined",
    "unescape",
    "visualViewport",
    "window",
];

struct NameUses<'s> {
    scoping: &'s Scoping,
    names: &'s HashSet<String>,
    edits: Vec<(Span, String)>,
    source: &'s str,
}

fn lookup(name: &str) -> String {
    format!("document.getElementById(\"{name}\")")
}

impl<'a> Visit<'a> for NameUses<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.is_global_reference(self.scoping) && self.names.contains(it.name.as_str()) {
            self.edits.push((it.span, lookup(&it.name)));
        }
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        // `typeof` of a name nothing answers to is "undefined", not "object".
        if it.operator == UnaryOperator::Typeof
            && let Expression::Identifier(ident) = unwrap_parens(&it.argument)
            && ident.is_global_reference(self.scoping)
            && self.names.contains(ident.name.as_str())
        {
            let inner = it.argument.span();
            self.edits
                .push((inner, format!("({} || undefined)", lookup(&ident.name))));
            return;
        }
        walk::walk_unary_expression(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.shorthand
            && let Expression::Identifier(ident) = &it.value
            && ident.is_global_reference(self.scoping)
            && self.names.contains(ident.name.as_str())
        {
            let span = ident.span;
            let key = &self.source[span.start as usize..span.end as usize];
            self.edits
                .push((span, format!("{key}: {}", lookup(&ident.name))));
            return;
        }
        walk::walk_object_property(self, it);
    }
}

/// Writes every use of a name meaning an element as its lookup; says whether any
/// source changed, and so whether `analysis` has to be taken again.
///
/// A name means an element when nothing declares or assigns it, the window has no
/// such property (its own or one a script sets), and an element can have it as an id: `ids` is what the page
/// wrote, and the script may write more. When a script computes an id, any such
/// name can be one, so an undeclared name reads as null rather than throwing.
pub fn resolve_named_elements(
    scripts: &mut [ScriptInput],
    ids: &BTreeSet<String>,
    analysis: &Analysis,
) -> bool {
    let allocator = Allocator::default();
    let parsed: Vec<Option<ParsedScript<'_>>> = scripts
        .iter()
        .map(|s| parse_script(&allocator, s).ok())
        .collect();
    let mut taken: HashSet<String> = WINDOW_NAMES.iter().map(|&n| n.to_owned()).collect();
    let mut free: HashSet<String> = HashSet::new();
    for (script, parsed) in scripts.iter().zip(&parsed) {
        let Some(parsed) = parsed else { continue };
        let scoping = &parsed.scoping;
        for (name, refs) in scoping.root_unresolved_references() {
            free.insert(name.to_string());
            if refs.iter().any(|&r| scoping.get_reference(r).is_write()) {
                taken.insert(name.to_string());
            }
        }
        if matches!(script.kind, ScriptKind::Classic) {
            for sym in scoping.iter_bindings_in(scoping.root_scope_id()) {
                taken.insert(scoping.symbol_name(sym).to_string());
            }
        }
        // The window's own properties come before named access.
        collect_window_property_writes(&parsed.program, scoping, &mut taken);
    }
    let names: HashSet<String> = free
        .into_iter()
        .filter(|n| {
            !taken.contains(n)
                && (ids.contains(n)
                    || analysis.written_ids.contains(n)
                    || analysis.writes_computed_id)
        })
        .collect();
    if names.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(scripts.len());
    for (script, parsed) in scripts.iter().zip(&parsed) {
        let Some(parsed) = parsed else {
            rewritten.push(None);
            continue;
        };
        let mut uses = NameUses {
            scoping: &parsed.scoping,
            names: &names,
            edits: Vec::new(),
            source: &script.source,
        };
        uses.visit_program(&parsed.program);
        uses.edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        let mut source = script.source.clone();
        for (span, text) in &uses.edits {
            source.replace_range(span.start as usize..span.end as usize, text);
        }
        changed |= !uses.edits.is_empty();
        rewritten.push(Some(source));
    }
    drop(parsed);
    for (script, source) in scripts.iter_mut().zip(rewritten) {
        if let Some(source) = source {
            script.source = source;
        }
    }
    changed
}
