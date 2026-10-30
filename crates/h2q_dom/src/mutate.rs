//! The mutation subset of the DOM: enough for a script-built element tree.
//!
//! Names and signatures follow `docs/design/live-dom.md` §2.2 so live mode
//! extends this rather than replacing it. No mutation log is kept and the
//! arena never reuses a slot, so every existing [`NodeId`] and traversal
//! order stays valid.

use crate::{Attribute, Document, Element, LocalName, Node, NodeData, NodeId, QualName, ns};

/// Why a mutation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomError {
    #[error("stale or destroyed node {0}")]
    StaleNode(NodeId),
    #[error("hierarchy request error: cannot insert {child} under {parent}")]
    HierarchyRequest { parent: NodeId, child: NodeId },
    #[error("reference node {reference} is not a child of {parent}")]
    NotFound { parent: NodeId, reference: NodeId },
    #[error("incompatible node kind at {0}")]
    InvalidNodeType(NodeId),
}

impl Document {
    /// A detached element in the HTML namespace, as `document.createElement`
    /// makes it. The name is lower-cased, as the DOM does for HTML documents.
    pub fn create_html_element(&mut self, local: &str) -> NodeId {
        let name = QualName::new(None, ns!(html), LocalName::from(local.to_ascii_lowercase()));
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(Node {
            parent: None,
            children: Vec::new(),
            data: NodeData::Element(Element {
                name,
                attrs: Vec::new(),
                line: 0,
                template_contents: None,
            }),
        });
        id
    }

    /// Sets an attribute. An existing attribute of the same name keeps its
    /// position and gets the new value; a new one goes last.
    ///
    /// # Errors
    /// [`DomError::InvalidNodeType`] when `node` is not an element.
    pub fn set_attribute(
        &mut self,
        node: NodeId,
        name: QualName,
        value: impl Into<String>,
    ) -> Result<(), DomError> {
        let value = value.into();
        let Some(NodeData::Element(el)) = self.nodes.get_mut(node.index()).map(|n| &mut n.data)
        else {
            return Err(DomError::InvalidNodeType(node));
        };
        match el
            .attrs
            .iter_mut()
            .find(|a| a.name.ns == name.ns && a.name.local == name.local)
        {
            Some(existing) => existing.value = value,
            None => el.attrs.push(Attribute { name, value }),
        }
        Ok(())
    }

    /// Appends `child` as the last child of `parent`, moving it first if it
    /// already has a parent. A document fragment is replaced by its children.
    ///
    /// # Errors
    /// [`DomError::HierarchyRequest`] where the DOM throws
    /// `HierarchyRequestError`: `parent` is not a document, fragment or
    /// element; `child` is a document or an ancestor of `parent` (or
    /// `parent` itself); a doctype or text goes where it may not; a second
    /// element would go under a document.
    /// [`DomError::StaleNode`] for an id outside the arena.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<NodeId, DomError> {
        for id in [parent, child] {
            if id.index() >= self.nodes.len() {
                return Err(DomError::StaleNode(id));
            }
        }
        let refuse = DomError::HierarchyRequest { parent, child };
        let parent_is_document = matches!(self.node(parent).data, NodeData::Document);
        if !parent_is_document
            && !matches!(
                self.node(parent).data,
                NodeData::Fragment | NodeData::Element(_)
            )
        {
            return Err(refuse);
        }
        if child == parent || self.ancestors(parent).any(|a| a == child) {
            return Err(refuse);
        }
        let incoming: Vec<NodeId> = match self.node(child).data {
            NodeData::Document => return Err(refuse),
            NodeData::Fragment => self.node(child).children.clone(),
            _ => vec![child],
        };
        for &n in &incoming {
            let ok = match self.node(n).data {
                NodeData::Doctype { .. } => parent_is_document,
                NodeData::Text(_) => !parent_is_document,
                NodeData::Element(_) => {
                    let elements = incoming.iter().filter(|&&m| self.element(m).is_some());
                    !parent_is_document
                        || (elements.count() == 1 && self.element_children(parent).all(|e| e == n))
                }
                _ => true,
            };
            if !ok {
                return Err(refuse);
            }
        }
        for n in incoming {
            self.detach(n);
            self.nodes[n.index()].parent = Some(parent);
            self.nodes[parent.index()].children.push(n);
        }
        Ok(child)
    }

    fn detach(&mut self, id: NodeId) {
        if let Some(old) = self.nodes[id.index()].parent.take() {
            self.nodes[old.index()].children.retain(|&c| c != id);
        }
    }
}
