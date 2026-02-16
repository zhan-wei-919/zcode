use super::markdown::MarkdownDocument;

/// View-side cache for markdown semantic state.
pub struct MarkdownViewState {
    doc: MarkdownDocument,
    version: u64,
}

impl MarkdownViewState {
    pub fn new(rope: &ropey::Rope) -> Self {
        Self {
            doc: MarkdownDocument::new(rope),
            version: 0,
        }
    }

    pub fn ensure_current(&mut self, rope: &ropey::Rope, edit_version: u64) {
        if self.version == edit_version {
            return;
        }
        self.doc.reparse(rope, edit_version);
        self.version = edit_version;
    }

    pub fn doc(&self) -> &MarkdownDocument {
        &self.doc
    }
}
