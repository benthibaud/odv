use crate::ast::DocumentAst;
use std::sync::Arc;

/// Snapshot-based Undo/Redo stack using structural sharing via `Arc<DocumentAst>`.
/// Guarantees exact rollback across complex multi-block edits and branch moves.
#[derive(Debug, Clone)]
pub struct DocumentHistory {
    past: Vec<Arc<DocumentAst>>,
    present: Arc<DocumentAst>,
    future: Vec<Arc<DocumentAst>>,
    max_depth: usize,
}

impl DocumentHistory {
    pub fn new(initial: DocumentAst) -> Self {
        Self {
            past: Vec::new(),
            present: Arc::new(initial),
            future: Vec::new(),
            max_depth: 200,
        }
    }

    pub fn current(&self) -> &DocumentAst {
        &self.present
    }

    pub fn current_arc(&self) -> Arc<DocumentAst> {
        Arc::clone(&self.present)
    }

    /// Apply a mutation to a cloned AST and push the previous state onto the undo stack.
    pub fn apply<F, R>(&mut self, mutate: F) -> R
    where
        F: FnOnce(&mut DocumentAst) -> R,
    {
        let mut next = (*self.present).clone();
        let result = mutate(&mut next);
        if next != *self.present {
            self.past.push(Arc::clone(&self.present));
            if self.past.len() > self.max_depth {
                self.past.remove(0);
            }
            self.present = Arc::new(next);
            self.future.clear();
        }
        result
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.past.pop() else {
            return false;
        };
        self.future.push(Arc::clone(&self.present));
        self.present = prev;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else {
            return false;
        };
        self.past.push(Arc::clone(&self.present));
        self.present = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BlockNode, InlineNode, Visibility};

    #[test]
    fn contract_undo_redo_across_branch_move_and_text_edit() {
        let mut doc = DocumentAst::new("History Contract", "en");
        let h_a = doc.push_heading(1, "Alpha", Some("Note A".into()));
        doc.push_paragraph("Alpha details", Visibility::All);
        let h_b = doc.push_heading(1, "Beta", Some("Note B".into()));
        doc.push_paragraph("Beta details", Visibility::All);

        let mut history = DocumentHistory::new(doc);
        assert!(!history.can_undo());

        // Move Beta branch before Alpha
        let moved = history.apply(|d| d.move_branch(h_b, 0));
        assert!(moved);
        assert!(history.can_undo());
        assert_eq!(history.current().mindmap_outline()[0].id, h_b);

        // Edit Alpha heading text
        history.apply(|d| {
            if let Some(BlockNode::Heading { children, .. }) = d.block_mut(h_a) {
                *children = vec![InlineNode::Text {
                    text: "Alpha Updated".into(),
                }];
            }
        });

        // Undo text edit -> Alpha restored, Beta still first
        assert!(history.undo());
        assert_eq!(history.current().mindmap_outline()[0].id, h_b);
        assert_eq!(history.current().mindmap_outline()[1].title, "Alpha");

        // Undo branch move -> original order restored
        assert!(history.undo());
        assert_eq!(history.current().mindmap_outline()[0].id, h_a);
        assert_eq!(history.current().mindmap_outline()[1].id, h_b);
        assert!(!history.can_undo());

        // Redo both
        assert!(history.redo());
        assert_eq!(history.current().mindmap_outline()[0].id, h_b);
        assert!(history.redo());
        assert_eq!(
            history.current().mindmap_outline()[1].title,
            "Alpha Updated"
        );
    }
}
