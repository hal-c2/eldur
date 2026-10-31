//! The html5ever [`TreeSink`]: an `Rc` tree while parsing, flattened into
//! the arena when parsing finishes.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElementFlags, NodeOrText, TreeSink};
use html5ever::{ParseOpts, QualName, local_name, ns};

use crate::{Attribute, Document, Element, Node, NodeData, NodeId, ParseError, QuirksMode};

#[derive(Debug)]
enum Kind {
    Document,
    Fragment,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element {
        name: QualName,
        attrs: RefCell<Vec<Attribute>>,
        line: u64,
        template_contents: Option<Handle>,
        mathml_annotation_xml_integration_point: bool,
    },
    Text(RefCell<String>),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

#[derive(Debug)]
struct SinkNode {
    kind: Kind,
    parent: RefCell<Option<Weak<SinkNode>>>,
    children: RefCell<Vec<Handle>>,
}

type Handle = Rc<SinkNode>;

fn new_node(kind: Kind) -> Handle {
    Rc::new(SinkNode {
        kind,
        parent: RefCell::new(None),
        children: RefCell::new(Vec::new()),
    })
}

fn convert_attrs(attrs: Vec<html5ever::Attribute>) -> Vec<Attribute> {
    attrs
        .into_iter()
        .map(|a| Attribute {
            name: a.name,
            value: a.value.to_string(),
        })
        .collect()
}

struct Sink {
    document: Handle,
    quirks_mode: Cell<QuirksMode>,
    line: Cell<u64>,
    errors: RefCell<Vec<ParseError>>,
}

impl Sink {
    fn new() -> Self {
        Self {
            document: new_node(Kind::Document),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
            line: Cell::new(1),
            errors: RefCell::new(Vec::new()),
        }
    }
}

fn detach(target: &Handle) {
    let parent = target.parent.borrow_mut().take();
    if let Some(parent) = parent.and_then(|w| w.upgrade()) {
        parent
            .children
            .borrow_mut()
            .retain(|c| !Rc::ptr_eq(c, target));
    }
}

fn append_child(parent: &Handle, child: Handle) {
    detach(&child);
    *child.parent.borrow_mut() = Some(Rc::downgrade(parent));
    parent.children.borrow_mut().push(child);
}

/// Appends text to `node`'s last child when that is a text node.
fn append_to_last_text(node: &Handle, text: &str) -> bool {
    if let Some(last) = node.children.borrow().last()
        && let Kind::Text(t) = &last.kind
    {
        t.borrow_mut().push_str(text);
        return true;
    }
    false
}

fn parent_of(node: &Handle) -> Option<Handle> {
    node.parent.borrow().as_ref().and_then(Weak::upgrade)
}

impl TreeSink for Sink {
    type Handle = Handle;
    type Output = Document;
    type ElemName<'a> = &'a QualName;

    fn finish(self) -> Document {
        let mut nodes = Vec::new();
        flatten(&self.document, None, &mut nodes);
        Document {
            nodes,
            quirks_mode: self.quirks_mode.get(),
            errors: self.errors.into_inner(),
        }
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        self.errors.borrow_mut().push(ParseError {
            line: self.line.get(),
            message: msg.into_owned(),
        });
    }

    fn get_document(&self) -> Handle {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, target: &'a Handle) -> &'a QualName {
        match &target.kind {
            Kind::Element { name, .. } => name,
            _ => panic!("elem_name called on a non-element"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<html5ever::Attribute>,
        flags: ElementFlags,
    ) -> Handle {
        let template_contents = flags.template.then(|| new_node(Kind::Fragment));
        new_node(Kind::Element {
            name,
            attrs: RefCell::new(convert_attrs(attrs)),
            line: self.line.get(),
            template_contents,
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        })
    }

    fn create_comment(&self, text: StrTendril) -> Handle {
        new_node(Kind::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Handle {
        new_node(Kind::ProcessingInstruction {
            target: target.to_string(),
            data: data.to_string(),
        })
    }

    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        match child {
            NodeOrText::AppendNode(node) => append_child(parent, node),
            NodeOrText::AppendText(text) => {
                if !append_to_last_text(parent, &text) {
                    append_child(parent, new_node(Kind::Text(RefCell::new(text.to_string()))));
                }
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Handle,
        prev_element: &Handle,
        child: NodeOrText<Handle>,
    ) {
        if parent_of(element).is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        append_child(
            &self.document,
            new_node(Kind::Doctype {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            }),
        );
    }

    fn get_template_contents(&self, target: &Handle) -> Handle {
        match &target.kind {
            Kind::Element {
                template_contents: Some(c),
                ..
            } => c.clone(),
            _ => panic!("get_template_contents called on a non-template"),
        }
    }

    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        Rc::ptr_eq(x, y)
    }

    fn set_quirks_mode(&self, mode: html5ever::tree_builder::QuirksMode) {
        use html5ever::tree_builder::QuirksMode as Q;
        self.quirks_mode.set(match mode {
            Q::NoQuirks => QuirksMode::NoQuirks,
            Q::LimitedQuirks => QuirksMode::LimitedQuirks,
            Q::Quirks => QuirksMode::Quirks,
        });
    }

    fn append_before_sibling(&self, sibling: &Handle, new_node_or_text: NodeOrText<Handle>) {
        let parent = parent_of(sibling).expect("append_before_sibling: sibling has no parent");
        let index = parent
            .children
            .borrow()
            .iter()
            .position(|c| Rc::ptr_eq(c, sibling))
            .expect("sibling is among its parent's children");
        let child = match new_node_or_text {
            NodeOrText::AppendText(text) => {
                if index > 0 {
                    let prev = parent.children.borrow()[index - 1].clone();
                    if let Kind::Text(t) = &prev.kind {
                        t.borrow_mut().push_str(&text);
                        return;
                    }
                }
                new_node(Kind::Text(RefCell::new(text.to_string())))
            }
            NodeOrText::AppendNode(node) => {
                detach(&node);
                node
            }
        };
        // Detaching may have moved the sibling when the node came from the
        // same parent, so look it up again.
        let index = parent
            .children
            .borrow()
            .iter()
            .position(|c| Rc::ptr_eq(c, sibling))
            .expect("sibling is among its parent's children");
        *child.parent.borrow_mut() = Some(Rc::downgrade(&parent));
        parent.children.borrow_mut().insert(index, child);
    }

    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<html5ever::Attribute>) {
        let Kind::Element {
            attrs: existing, ..
        } = &target.kind
        else {
            panic!("add_attrs_if_missing called on a non-element");
        };
        let mut existing = existing.borrow_mut();
        for attr in convert_attrs(attrs) {
            if !existing.iter().any(|a| a.name == attr.name) {
                existing.push(attr);
            }
        }
    }

    fn remove_from_parent(&self, target: &Handle) {
        detach(target);
    }

    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        let children = std::mem::take(&mut *node.children.borrow_mut());
        for child in children {
            *child.parent.borrow_mut() = Some(Rc::downgrade(new_parent));
            new_parent.children.borrow_mut().push(child);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Handle) -> bool {
        matches!(
            handle.kind,
            Kind::Element {
                mathml_annotation_xml_integration_point: true,
                ..
            }
        )
    }

    fn set_current_line(&self, line_number: u64) {
        self.line.set(line_number);
    }
}

fn flatten(node: &Handle, parent: Option<NodeId>, nodes: &mut Vec<Node>) -> NodeId {
    let id = NodeId::new(nodes.len());
    // Reserve the slot so ids are in document order; fill data below.
    nodes.push(Node {
        parent,
        children: Vec::new(),
        data: NodeData::Document,
    });
    let data = match &node.kind {
        Kind::Document => NodeData::Document,
        Kind::Fragment => NodeData::Fragment,
        Kind::Doctype {
            name,
            public_id,
            system_id,
        } => NodeData::Doctype {
            name: name.clone(),
            public_id: public_id.clone(),
            system_id: system_id.clone(),
        },
        Kind::Element {
            name, attrs, line, ..
        } => NodeData::Element(Element {
            name: name.clone(),
            attrs: attrs.borrow().clone(),
            line: *line,
            template_contents: None,
        }),
        Kind::Text(t) => NodeData::Text(t.borrow().clone()),
        Kind::Comment(c) => NodeData::Comment(c.clone()),
        Kind::ProcessingInstruction { target, data } => NodeData::ProcessingInstruction {
            target: target.clone(),
            data: data.clone(),
        },
    };
    nodes[id.index()].data = data;
    let children: Vec<NodeId> = node
        .children
        .borrow()
        .iter()
        .map(|c| flatten(c, Some(id), nodes))
        .collect();
    nodes[id.index()].children = children;
    if let Kind::Element {
        template_contents: Some(contents),
        ..
    } = &node.kind
    {
        // The fragment hangs off the template, not among its children.
        let fragment = flatten(contents, Some(id), nodes);
        if let NodeData::Element(e) = &mut nodes[id.index()].data {
            e.template_contents = Some(fragment);
        }
    }
    id
}

pub(crate) fn parse_document(html: &str) -> Document {
    html5ever::parse_document(Sink::new(), ParseOpts::default()).one(html)
}

pub(crate) fn parse_fragment(html: &str) -> Document {
    html5ever::parse_fragment(
        Sink::new(),
        ParseOpts::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        Vec::new(),
        false,
    )
    .one(html)
}
