use crate::ast::{BlockNode, DocumentAst, InlineNode, NodeId, ViewMode, Visibility};
use crate::history::DocumentHistory;

/// Caret position inside a specific block of the AST (UTF-8 char-boundary offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub block_id: NodeId,
    pub offset: usize,
}

/// Interactive semantic editor state combining `DocumentHistory` and `Cursor`.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub history: DocumentHistory,
    pub cursor: Option<Cursor>,
}

impl EditorState {
    pub fn new(doc: DocumentAst) -> Self {
        let first_block = doc.blocks.first().map(|b| Cursor {
            block_id: b.id(),
            offset: 0,
        });
        Self {
            history: DocumentHistory::new(doc),
            cursor: first_block,
        }
    }

    pub fn document(&self) -> &DocumentAst {
        self.history.current()
    }

    pub fn focus_block(&mut self, block_id: NodeId) {
        if let Some(block) = self
            .history
            .current()
            .blocks
            .iter()
            .find(|b| b.id() == block_id)
        {
            let len = block_plain_len(block);
            self.cursor = Some(Cursor {
                block_id,
                offset: len,
            });
        }
    }

    pub fn move_left(&mut self) {
        let Some(mut cur) = self.cursor else { return };
        if let Some(text) = self.active_block_text(cur.block_id)
            && cur.offset > 0
        {
            let prev = text[..cur.offset]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            cur.offset = prev;
            self.cursor = Some(cur);
        }
    }

    pub fn move_right(&mut self) {
        let Some(mut cur) = self.cursor else { return };
        if let Some(text) = self.active_block_text(cur.block_id)
            && cur.offset < text.len()
        {
            let next = text[cur.offset..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| cur.offset + i)
                .unwrap_or(text.len());
            cur.offset = next;
            self.cursor = Some(cur);
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(cur) = self.cursor else { return };
        let inserted_len = text.len();
        let ok = self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return false;
            };
            let Some(plain) = mutate_block_plain_text(block) else {
                return false;
            };
            let pos = clamp_char_boundary(&plain, cur.offset);
            let mut updated = String::with_capacity(plain.len() + inserted_len);
            updated.push_str(&plain[..pos]);
            updated.push_str(text);
            updated.push_str(&plain[pos..]);
            set_block_plain_text(block, updated);
            true
        });
        if ok {
            self.cursor = Some(Cursor {
                block_id: cur.block_id,
                offset: cur.offset + inserted_len,
            });
        }
    }

    pub fn backspace(&mut self) {
        let Some(cur) = self.cursor else { return };
        if cur.offset == 0 {
            return;
        }
        let mut new_offset = cur.offset;
        let ok = self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return false;
            };
            let Some(plain) = mutate_block_plain_text(block) else {
                return false;
            };
            let pos = clamp_char_boundary(&plain, cur.offset);
            if pos == 0 {
                return false;
            }
            let prev = plain[..pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            let mut updated = String::with_capacity(plain.len());
            updated.push_str(&plain[..prev]);
            updated.push_str(&plain[pos..]);
            new_offset = prev;
            set_block_plain_text(block, updated);
            true
        });
        if ok {
            self.cursor = Some(Cursor {
                block_id: cur.block_id,
                offset: new_offset,
            });
        }
    }

    pub fn split_block(&mut self) {
        let Some(cur) = self.cursor else { return };
        let new_id = self.history.apply(|doc| {
            let Some(idx) = doc.index_of(cur.block_id) else {
                return None;
            };
            let tail = {
                let block = &mut doc.blocks[idx];
                let Some(plain) = mutate_block_plain_text(block) else {
                    return None;
                };
                let pos = clamp_char_boundary(&plain, cur.offset);
                let head = plain[..pos].to_string();
                let tail = plain[pos..].to_string();
                set_block_plain_text(block, head);
                tail
            };
            let new_id = doc.alloc_id();
            doc.blocks.insert(
                idx + 1,
                BlockNode::Paragraph {
                    id: new_id,
                    children: vec![InlineNode::Text { text: tail }],
                    visibility: Visibility::All,
                },
            );
            Some(new_id)
        });
        if let Some(id) = new_id {
            self.cursor = Some(Cursor {
                block_id: id,
                offset: 0,
            });
        }
    }

    /// Toggle semantic `<strong>` on the active block (`Ctrl+B` demo).
    pub fn toggle_strong_block(&mut self) {
        let Some(cur) = self.cursor else { return };
        self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return;
            };
            let children = match block {
                BlockNode::Heading { children, .. }
                | BlockNode::Paragraph { children, .. }
                | BlockNode::Blockquote { children, .. }
                | BlockNode::Aside { children, .. } => children,
                _ => return,
            };
            if children.len() == 1
                && let InlineNode::Strong {
                    children: inner, ..
                } = &children[0]
            {
                *children = inner.clone();
            } else {
                let prev = std::mem::take(children);
                *children = vec![InlineNode::Strong { children: prev }];
            }
        });
    }

    pub fn active_block_plain_text(&self) -> Option<String> {
        let cur = self.cursor?;
        self.active_block_text(cur.block_id)
    }

    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo();
        if ok {
            self.clamp_cursor();
        }
        ok
    }

    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo();
        if ok {
            self.clamp_cursor();
        }
        ok
    }

    fn clamp_cursor(&mut self) {
        if let Some(mut cur) = self.cursor {
            if let Some(text) = self.active_block_text(cur.block_id) {
                cur.offset = clamp_char_boundary(&text, cur.offset);
                self.cursor = Some(cur);
            } else {
                self.cursor = self.history.current().blocks.first().map(|b| Cursor {
                    block_id: b.id(),
                    offset: 0,
                });
            }
        }
    }

    fn active_block_text(&self, id: NodeId) -> Option<String> {
        self.history
            .current()
            .blocks
            .iter()
            .find(|b| b.id() == id)
            .and_then(mutate_block_plain_text)
    }

    /// Render the inner HTML of `<main>` with `data-odv-id` attributes and a visible caret.
    pub fn render_main_inner_html(
        &self,
        mode: ViewMode,
        title_label: &str,
        hint_label: &str,
    ) -> String {
        let doc = self.history.current();
        let mut out = format!(
            "<header class=\"odv-toolbar\">{} — {}</header>\n",
            escape_html(title_label),
            escape_html(hint_label)
        );

        for block in &doc.blocks {
            let is_active = self.cursor.is_some_and(|c| c.block_id == block.id());
            let caret_offset = if is_active {
                self.cursor.map(|c| c.offset)
            } else {
                None
            };
            let active_cls = if is_active { " odv-active-block" } else { "" };

            match (mode, block) {
                (
                    ViewMode::MindMap,
                    BlockNode::Heading {
                        id,
                        level,
                        children,
                        note,
                        ..
                    },
                ) => {
                    let lvl = (*level).clamp(1, 5);
                    let content = render_inlines_with_caret(children, caret_offset);
                    out.push_str(&format!(
                        "<h{lvl} id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</h{lvl}>\n",
                        id.0
                    ));
                    if let Some(n) = note {
                        out.push_str(&format!(
                            "<aside class=\"mindmap-note\">{}</aside>\n",
                            escape_html(n)
                        ));
                    }
                }
                (ViewMode::MindMap, _) => {}
                (
                    ViewMode::Presentation,
                    BlockNode::Heading {
                        id,
                        level,
                        children,
                        note,
                        ..
                    },
                ) => {
                    let lvl = (*level).clamp(1, 5);
                    let content = render_inlines_with_caret(children, caret_offset);
                    out.push_str(&format!(
                        "<h{lvl} id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</h{lvl}>\n",
                        id.0
                    ));
                    if let Some(n) = note {
                        out.push_str(&format!(
                            "<p class=\"slide-keypoint\">{}</p>\n",
                            escape_html(n)
                        ));
                    }
                }
                (ViewMode::Presentation, _) => {}
                (_, BlockNode::Heading { id, level, children, .. }) => {
                    let lvl = (*level).clamp(1, 5);
                    let content = render_inlines_with_caret(children, caret_offset);
                    out.push_str(&format!(
                        "<h{lvl} id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</h{lvl}>\n",
                        id.0
                    ));
                }
                (_, BlockNode::Paragraph { id, children, .. }) => {
                    let content = render_inlines_with_caret(children, caret_offset);
                    out.push_str(&format!(
                        "<p id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</p>\n",
                        id.0
                    ));
                }
                (_, BlockNode::Blockquote { id, children, .. }) => {
                    let content = render_inlines_with_caret(children, caret_offset);
                    out.push_str(&format!(
                        "<blockquote id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</blockquote>\n",
                        id.0
                    ));
                }
                _ => {}
            }
        }
        out
    }
}

fn block_plain_len(block: &BlockNode) -> usize {
    mutate_block_plain_text(block)
        .map(|s| s.len())
        .unwrap_or(0)
}

fn mutate_block_plain_text(block: &BlockNode) -> Option<String> {
    let children = match block {
        BlockNode::Heading { children, .. }
        | BlockNode::Paragraph { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::Aside { children, .. } => children,
        _ => return None,
    };
    Some(flatten_inlines(children))
}

fn flatten_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } | InlineNode::Code { text } => out.push_str(text),
            InlineNode::Strong { children } | InlineNode::Emphasis { children } => {
                out.push_str(&flatten_inlines(children));
            }
        }
    }
    out
}

fn set_block_plain_text(block: &mut BlockNode, text: String) {
    let children = match block {
        BlockNode::Heading { children, .. }
        | BlockNode::Paragraph { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::Aside { children, .. } => children,
        _ => return,
    };
    let is_strong = children.len() == 1 && matches!(children[0], InlineNode::Strong { .. });
    if is_strong {
        *children = vec![InlineNode::Strong {
            children: vec![InlineNode::Text { text }],
        }];
    } else {
        *children = vec![InlineNode::Text { text }];
    }
}

fn clamp_char_boundary(s: &str, offset: usize) -> usize {
    let mut pos = offset.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn render_inlines_with_caret(nodes: &[InlineNode], caret: Option<usize>) -> String {
    let Some(caret_pos) = caret else {
        return render_inlines_plain(nodes);
    };
    let plain = flatten_inlines(nodes);
    let pos = clamp_char_boundary(&plain, caret_pos);
    let is_strong = nodes.len() == 1 && matches!(nodes[0], InlineNode::Strong { .. });
    let before = escape_html(&plain[..pos]);
    let after = escape_html(&plain[pos..]);
    let inner = format!("{before}<mark class=\"odv-caret\">|</mark>{after}");
    if is_strong {
        format!("<strong>{inner}</strong>")
    } else {
        inner
    }
}

fn render_inlines_plain(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } => out.push_str(&escape_html(text)),
            InlineNode::Strong { children } => {
                out.push_str(&format!("<strong>{}</strong>", render_inlines_plain(children)));
            }
            InlineNode::Emphasis { children } => {
                out.push_str(&format!("<em>{}</em>", render_inlines_plain(children)));
            }
            InlineNode::Code { text } => {
                out.push_str(&format!("<code>{}</code>", escape_html(text)));
            }
        }
    }
    out
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_editor_typing_split_bold_and_undo() {
        let mut doc = DocumentAst::new("Editor", "fr");
        let p_id = doc.push_paragraph("Bonjour", Visibility::All);
        let mut editor = EditorState::new(doc);
        editor.focus_block(p_id);

        editor.insert_text(" à tous");
        assert_eq!(
            editor.active_block_plain_text().as_deref(),
            Some("Bonjour à tous")
        );

        editor.toggle_strong_block();
        let html = editor.render_main_inner_html(ViewMode::Document, "ODV", "F2");
        assert!(html.contains("<strong>Bonjour à tous<mark class=\"odv-caret\">|</mark></strong>"));

        editor.split_block();
        assert_eq!(editor.document().blocks.len(), 2);

        assert!(editor.undo());
        assert_eq!(editor.document().blocks.len(), 1);
    }
}
