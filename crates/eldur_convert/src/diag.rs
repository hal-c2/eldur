//! Collects [`Diagnostic`]s from every stage.

use crate::{Diagnostic, Severity};

#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    /// `source` for diagnostics about the HTML document itself.
    pub document: String,
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub(crate) fn new(document: &str) -> Self {
        Self {
            document: document.to_owned(),
            items: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        // The same construct repeated on many elements is reported once per
        // line, which is what a reader acts on.
        if !self.items.contains(&diagnostic) {
            self.items.push(diagnostic);
        }
    }

    fn at(&mut self, severity: Severity, stage: &'static str, line: Option<u32>, message: String) {
        let source = self.document.clone();
        self.push(Diagnostic {
            severity,
            stage,
            source,
            line,
            message,
        });
    }

    pub(crate) fn info(&mut self, stage: &'static str, line: Option<u32>, message: String) {
        self.at(Severity::Info, stage, line, message);
    }

    pub(crate) fn warning(&mut self, stage: &'static str, line: Option<u32>, message: String) {
        self.at(Severity::Warning, stage, line, message);
    }

    pub(crate) fn error(&mut self, stage: &'static str, line: Option<u32>, message: String) {
        self.at(Severity::Error, stage, line, message);
    }

    pub(crate) fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}
