use crate::ast::{BlockNode, DocumentAst, InlineNode, NodeId, ViewMode, Visibility, escape_html};
use crate::history::DocumentHistory;
use crate::typo::{AutoTextCatalog, TypoProfile, apply_micro_typo};

/// Caret position inside a specific block of the AST (UTF-8 char-boundary offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub block_id: NodeId,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    code: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StyledRun {
    text: String,
    style: InlineStyle,
}

/// Interactive semantic editor state combining `DocumentHistory`, caret, and intra-block selection.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub history: DocumentHistory,
    pub cursor: Option<Cursor>,
    /// Selection anchor inside the same block as `cursor` when `Some(offset)`.
    pub selection_anchor: Option<usize>,
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
            selection_anchor: None,
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
            self.selection_anchor = None;
        }
    }

    /// Return ordered `(start, end)` byte range of the active intra-block selection, if non-empty.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let cur = self.cursor?;
        let anchor = self.selection_anchor?;
        if anchor == cur.offset {
            None
        } else {
            Some((anchor.min(cur.offset), anchor.max(cur.offset)))
        }
    }

    pub fn move_left(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            if let Some((start, _)) = self.selection_range() {
                cur.offset = start;
                self.cursor = Some(cur);
                self.selection_anchor = None;
                return;
            }
            self.selection_anchor = None;
        }
        if cur.offset > 0 {
            let prev = text[..cur.offset]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            cur.offset = prev;
            self.cursor = Some(cur);
        }
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            if let Some((_, end)) = self.selection_range() {
                cur.offset = end;
                self.cursor = Some(cur);
                self.selection_anchor = None;
                return;
            }
            self.selection_anchor = None;
        }
        if cur.offset < text.len() {
            let next = text[cur.offset..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| cur.offset + i)
                .unwrap_or(text.len());
            cur.offset = next;
            self.cursor = Some(cur);
        }
    }

    pub fn move_word_left(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        let mut pos = clamp_char_boundary(&text, cur.offset);
        while pos > 0 {
            let prev = text[..pos]
                .char_indices()
                .next_back()
                .map(|(i, c)| (i, c))
                .unwrap();
            if !prev.1.is_whitespace() {
                break;
            }
            pos = prev.0;
        }
        while pos > 0 {
            let prev = text[..pos]
                .char_indices()
                .next_back()
                .map(|(i, c)| (i, c))
                .unwrap();
            if prev.1.is_whitespace() {
                break;
            }
            pos = prev.0;
        }
        cur.offset = pos;
        self.cursor = Some(cur);
    }

    pub fn move_word_right(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        let mut pos = clamp_char_boundary(&text, cur.offset);
        while pos < text.len() {
            let ch = text[pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                break;
            }
            pos += ch.len_utf8();
        }
        while pos < text.len() {
            let ch = text[pos..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            pos += ch.len_utf8();
        }
        cur.offset = pos;
        self.cursor = Some(cur);
    }

    pub fn move_home(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        cur.offset = 0;
        self.cursor = Some(cur);
    }

    pub fn move_end(&mut self, extend_selection: bool) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(cur.offset);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        cur.offset = text.len();
        self.cursor = Some(cur);
    }

    pub fn select_all_in_block(&mut self) {
        let Some(mut cur) = self.cursor else { return };
        let Some(text) = self.active_block_text(cur.block_id) else {
            return;
        };
        self.selection_anchor = Some(0);
        cur.offset = text.len();
        self.cursor = Some(cur);
    }

    pub fn delete_forward(&mut self) {
        let Some(cur) = self.cursor else { return };
        if self.selection_range().is_some() {
            self.backspace();
            return;
        }
        self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return;
            };
            let Some(children) = block_children_mut(block) else {
                return;
            };
            let runs = inlines_to_runs(children);
            let plain = flatten_runs(&runs);
            let pos = clamp_char_boundary(&plain, cur.offset);
            if pos >= plain.len() {
                return;
            }
            let next = plain[pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| pos + i)
                .unwrap_or(plain.len());
            let updated = remove_range_from_runs(&runs, pos, next);
            *children = runs_to_inlines(&updated);
        });
    }

    pub fn move_up(&mut self, mode: ViewMode) {
        let Some(cur) = self.cursor else { return };
        let visible = self.visible_block_ids(mode);
        if let Some(pos) = visible.iter().position(|&id| id == cur.block_id)
            && pos > 0
        {
            self.focus_block(visible[pos - 1]);
        }
    }

    pub fn move_down(&mut self, mode: ViewMode) {
        let Some(cur) = self.cursor else { return };
        let visible = self.visible_block_ids(mode);
        if let Some(pos) = visible.iter().position(|&id| id == cur.block_id)
            && pos + 1 < visible.len()
        {
            self.focus_block(visible[pos + 1]);
        }
    }

    fn visible_block_ids(&self, mode: ViewMode) -> Vec<NodeId> {
        self.history
            .current()
            .blocks
            .iter()
            .filter(|b| match mode {
                ViewMode::MindMap | ViewMode::Presentation => {
                    matches!(b, BlockNode::Heading { .. })
                }
                _ => matches!(
                    b,
                    BlockNode::Heading { .. }
                        | BlockNode::Paragraph { .. }
                        | BlockNode::Blockquote { .. }
                        | BlockNode::Aside { .. }
                ),
            })
            .map(|b| b.id())
            .collect()
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(cur) = self.cursor else { return };
        let sel = self.selection_range();
        let inserted_len = text.len();
        let mut new_offset = cur.offset + inserted_len;

        let ok = self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return false;
            };
            let Some(children) = block_children_mut(block) else {
                return false;
            };
            let mut runs = inlines_to_runs(children);
            if let Some((start, end)) = sel {
                runs = remove_range_from_runs(&runs, start, end);
                let style = style_at_offset(&runs, start);
                runs = insert_into_runs(&runs, start, text, style);
                new_offset = start + inserted_len;
            } else {
                let style = style_at_offset(&runs, cur.offset);
                runs = insert_into_runs(&runs, cur.offset, text, style);
                new_offset = cur.offset + inserted_len;
            }
            *children = runs_to_inlines(&runs);
            true
        });

        if ok {
            self.cursor = Some(Cursor {
                block_id: cur.block_id,
                offset: new_offset,
            });
            self.selection_anchor = None;
        }
    }

    /// Insert text while applying automatic regional micro-typography based on document language.
    pub fn insert_text_with_typo(&mut self, text: &str) {
        let profile = TypoProfile::from_language_tag(&self.history.current().language);
        if profile == TypoProfile::None || self.selection_range().is_some() {
            self.insert_text(text);
            return;
        }
        let Some(cur) = self.cursor else { return };
        let Some(plain) = self.active_block_text(cur.block_id) else {
            self.insert_text(text);
            return;
        };
        let pos = clamp_char_boundary(&plain, cur.offset);
        let (erase_bytes, replacement) = apply_micro_typo(profile, &plain[..pos], text);
        if erase_bytes == 0 {
            self.insert_text(&replacement);
            return;
        }
        let erase_start = clamp_char_boundary(&plain, pos.saturating_sub(erase_bytes));
        self.selection_anchor = Some(erase_start);
        self.cursor = Some(Cursor {
            block_id: cur.block_id,
            offset: pos,
        });
        self.insert_text(&replacement);
    }

    /// Expand an AutoText shortcut (e.g. `focus` or `/focus`) immediately before the caret (`F3`).
    pub fn trigger_autotext(&mut self, catalog: &AutoTextCatalog) -> bool {
        let Some(cur) = self.cursor else {
            return false;
        };
        let Some(plain) = self.active_block_text(cur.block_id) else {
            return false;
        };
        let pos = clamp_char_boundary(&plain, cur.offset);
        let before = &plain[..pos];
        let token_start = before
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let token = &before[token_start..];
        if token.is_empty() {
            return false;
        }

        let mut expanded_id = None;
        let ok = self.history.apply(|doc| {
            let mut next_id = || doc.alloc_id();
            let Some(new_blocks) = catalog.materialize(token, &mut next_id) else {
                return false;
            };
            let Some(idx) = doc.index_of(cur.block_id) else {
                return false;
            };
            let remaining_is_empty = token_start == 0 && pos == plain.len();
            if let Some(first_new) = new_blocks.first() {
                expanded_id = Some(first_new.id());
            }

            if remaining_is_empty {
                doc.blocks.splice(idx..=idx, new_blocks);
            } else {
                if let Some(children) = block_children_mut(&mut doc.blocks[idx]) {
                    let runs = inlines_to_runs(children);
                    let updated = remove_range_from_runs(&runs, token_start, pos);
                    *children = runs_to_inlines(&updated);
                }
                doc.blocks.splice(idx + 1..idx + 1, new_blocks);
            }
            true
        });

        if ok {
            if let Some(id) = expanded_id {
                self.focus_block(id);
            }
            self.selection_anchor = None;
        }
        ok
    }

    pub fn backspace(&mut self) {
        let Some(cur) = self.cursor else { return };
        if let Some((start, end)) = self.selection_range() {
            let ok = self.history.apply(|doc| {
                let Some(block) = doc.block_mut(cur.block_id) else {
                    return false;
                };
                let Some(children) = block_children_mut(block) else {
                    return false;
                };
                let runs = inlines_to_runs(children);
                let updated = remove_range_from_runs(&runs, start, end);
                *children = runs_to_inlines(&updated);
                true
            });
            if ok {
                self.cursor = Some(Cursor {
                    block_id: cur.block_id,
                    offset: start,
                });
                self.selection_anchor = None;
            }
            return;
        }

        if cur.offset == 0 {
            return;
        }
        let mut new_offset = cur.offset;
        let ok = self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return false;
            };
            let Some(children) = block_children_mut(block) else {
                return false;
            };
            let runs = inlines_to_runs(children);
            let plain = flatten_runs(&runs);
            let pos = clamp_char_boundary(&plain, cur.offset);
            if pos == 0 {
                return false;
            }
            let prev = plain[..pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            let updated = remove_range_from_runs(&runs, prev, pos);
            *children = runs_to_inlines(&updated);
            new_offset = prev;
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
        let sel = self.selection_range();
        let new_id = self.history.apply(|doc| {
            let Some(idx) = doc.index_of(cur.block_id) else {
                return None;
            };
            let tail_inlines = {
                let block = &mut doc.blocks[idx];
                let children = block_children_mut(block)?;
                let mut runs = inlines_to_runs(children);
                let split_pos = if let Some((start, end)) = sel {
                    runs = remove_range_from_runs(&runs, start, end);
                    start
                } else {
                    cur.offset
                };
                let (head_runs, tail_runs) = split_runs_at(&runs, split_pos);
                *children = runs_to_inlines(&head_runs);
                runs_to_inlines(&tail_runs)
            };
            let new_id = doc.alloc_id();
            doc.blocks.insert(
                idx + 1,
                BlockNode::Paragraph {
                    id: new_id,
                    children: tail_inlines,
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
            self.selection_anchor = None;
        }
    }

    /// Toggle semantic `<strong>` on the active selection (or whole block if no selection).
    pub fn toggle_strong(&mut self) {
        self.toggle_style(|s| &mut s.bold);
    }

    /// Toggle semantic `<em>` on the active selection (or whole block if no selection).
    pub fn toggle_emphasis(&mut self) {
        self.toggle_style(|s| &mut s.italic);
    }

    fn toggle_style<F>(&mut self, field: F)
    where
        F: Fn(&mut InlineStyle) -> &mut bool,
    {
        let Some(cur) = self.cursor else { return };
        let sel = self.selection_range();
        self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return;
            };
            let Some(children) = block_children_mut(block) else {
                return;
            };
            let runs = inlines_to_runs(children);
            let total_len = flatten_runs(&runs).len();
            let (start, end) = sel.unwrap_or((0, total_len));
            if start >= end {
                return;
            }
            let updated = toggle_style_in_range(&runs, start, end, &field);
            *children = runs_to_inlines(&updated);
        });
    }

    /// Convert the active block between Heading (`Some(1..=5)`) and Paragraph (`None`).
    pub fn set_active_block_heading(&mut self, level: Option<u8>) {
        let Some(cur) = self.cursor else { return };
        self.history.apply(|doc| {
            let Some(block) = doc.block_mut(cur.block_id) else {
                return;
            };
            let id = block.id();
            let vis = block.visibility();
            let children = match block {
                BlockNode::Heading { children, .. }
                | BlockNode::Paragraph { children, .. }
                | BlockNode::Blockquote { children, .. } => std::mem::take(children),
                _ => return,
            };
            let note = match block {
                BlockNode::Heading { note, .. } => note.take(),
                _ => None,
            };
            *block = match level {
                Some(lvl) => BlockNode::Heading {
                    id,
                    level: lvl.clamp(1, 5),
                    children,
                    note,
                    visibility: vis,
                },
                None => BlockNode::Paragraph {
                    id,
                    children,
                    visibility: vis,
                },
            };
        });
    }

    /// Return the currently selected plain text, or the active block's plain text if no selection.
    pub fn copy_text(&self) -> Option<String> {
        let cur = self.cursor?;
        let text = self.active_block_text(cur.block_id)?;
        if let Some((start, end)) = self.selection_range() {
            let s = clamp_char_boundary(&text, start);
            let e = clamp_char_boundary(&text, end);
            Some(text[s..e].to_string())
        } else {
            Some(text)
        }
    }

    pub fn active_block_plain_text(&self) -> Option<String> {
        let cur = self.cursor?;
        self.active_block_text(cur.block_id)
    }

    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo();
        if ok {
            self.selection_anchor = None;
            self.clamp_cursor();
        }
        ok
    }

    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo();
        if ok {
            self.selection_anchor = None;
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
            .and_then(block_plain_text)
    }

    /// Render the inner HTML of `<main>` with `id="odv-<id>"`, selection highlight, and caret.
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
            let sel = if is_active {
                self.selection_range()
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
                    let content = render_inlines_interactive(children, caret_offset, sel);
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
                    let content = render_inlines_interactive(children, caret_offset, sel);
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
                    let content = render_inlines_interactive(children, caret_offset, sel);
                    out.push_str(&format!(
                        "<h{lvl} id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</h{lvl}>\n",
                        id.0
                    ));
                }
                (_, BlockNode::Paragraph { id, children, .. }) => {
                    let content = render_inlines_interactive(children, caret_offset, sel);
                    out.push_str(&format!(
                        "<p id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</p>\n",
                        id.0
                    ));
                }
                (_, BlockNode::Blockquote { id, children, .. }) => {
                    let content = render_inlines_interactive(children, caret_offset, sel);
                    out.push_str(&format!(
                        "<blockquote id=\"odv-{}\" class=\"odv-block{active_cls}\">{content}</blockquote>\n",
                        id.0
                    ));
                }
                (
                    _,
                    BlockNode::Aside {
                        id,
                        kind,
                        title,
                        children,
                        ..
                    },
                ) => {
                    let content = render_inlines_interactive(children, caret_offset, sel);
                    out.push_str(&format!(
                        "<aside id=\"odv-{}\" class=\"{} odv-block{active_cls}\"><h4>{}</h4><p>{content}</p></aside>\n",
                        id.0,
                        escape_html(kind),
                        escape_html(title)
                    ));
                }
                _ => {}
            }
        }
        out
    }
}

fn block_children_mut(block: &mut BlockNode) -> Option<&mut Vec<InlineNode>> {
    match block {
        BlockNode::Heading { children, .. }
        | BlockNode::Paragraph { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::Aside { children, .. } => Some(children),
        _ => None,
    }
}

fn block_plain_text(block: &BlockNode) -> Option<String> {
    let children = match block {
        BlockNode::Heading { children, .. }
        | BlockNode::Paragraph { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::Aside { children, .. } => children,
        _ => return None,
    };
    Some(flatten_runs(&inlines_to_runs(children)))
}

fn block_plain_len(block: &BlockNode) -> usize {
    block_plain_text(block).map(|s| s.len()).unwrap_or(0)
}

fn inlines_to_runs(nodes: &[InlineNode]) -> Vec<StyledRun> {
    let mut out = Vec::new();
    collect_runs(nodes, InlineStyle::default(), &mut out);
    normalize_runs(out)
}

fn collect_runs(nodes: &[InlineNode], style: InlineStyle, out: &mut Vec<StyledRun>) {
    for node in nodes {
        match node {
            InlineNode::Text { text } => {
                if !text.is_empty() {
                    out.push(StyledRun {
                        text: text.clone(),
                        style,
                    });
                }
            }
            InlineNode::Code { text } => {
                if !text.is_empty() {
                    let mut next = style;
                    next.code = true;
                    out.push(StyledRun {
                        text: text.clone(),
                        style: next,
                    });
                }
            }
            InlineNode::Strong { children } => {
                let mut next = style;
                next.bold = true;
                collect_runs(children, next, out);
            }
            InlineNode::Emphasis { children } => {
                let mut next = style;
                next.italic = true;
                collect_runs(children, next, out);
            }
        }
    }
}

fn normalize_runs(runs: Vec<StyledRun>) -> Vec<StyledRun> {
    let mut out: Vec<StyledRun> = Vec::new();
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut()
            && last.style == run.style
        {
            last.text.push_str(&run.text);
        } else {
            out.push(run);
        }
    }
    out
}

fn runs_to_inlines(runs: &[StyledRun]) -> Vec<InlineNode> {
    if runs.is_empty() {
        return vec![InlineNode::Text {
            text: String::new(),
        }];
    }
    runs.iter()
        .map(|run| {
            let mut node = if run.style.code {
                InlineNode::Code {
                    text: run.text.clone(),
                }
            } else {
                InlineNode::Text {
                    text: run.text.clone(),
                }
            };
            if run.style.italic {
                node = InlineNode::Emphasis {
                    children: vec![node],
                };
            }
            if run.style.bold {
                node = InlineNode::Strong {
                    children: vec![node],
                };
            }
            node
        })
        .collect()
}

fn flatten_runs(runs: &[StyledRun]) -> String {
    let mut s = String::new();
    for r in runs {
        s.push_str(&r.text);
    }
    s
}

fn style_at_offset(runs: &[StyledRun], offset: usize) -> InlineStyle {
    let mut pos = 0;
    for r in runs {
        let end = pos + r.text.len();
        if offset <= end {
            return r.style;
        }
        pos = end;
    }
    runs.last().map(|r| r.style).unwrap_or_default()
}

fn insert_into_runs(
    runs: &[StyledRun],
    offset: usize,
    text: &str,
    style: InlineStyle,
) -> Vec<StyledRun> {
    let (mut head, tail) = split_runs_at(runs, offset);
    head.push(StyledRun {
        text: text.to_string(),
        style,
    });
    head.extend(tail);
    normalize_runs(head)
}

fn remove_range_from_runs(runs: &[StyledRun], start: usize, end: usize) -> Vec<StyledRun> {
    if start >= end {
        return runs.to_vec();
    }
    let (head, _) = split_runs_at(runs, start);
    let (_, tail) = split_runs_at(runs, end);
    let mut out = head;
    out.extend(tail);
    normalize_runs(out)
}

fn split_runs_at(runs: &[StyledRun], offset: usize) -> (Vec<StyledRun>, Vec<StyledRun>) {
    let plain = flatten_runs(runs);
    let target = clamp_char_boundary(&plain, offset);
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let mut cursor = 0;

    for r in runs {
        let len = r.text.len();
        let end = cursor + len;
        if end <= target {
            head.push(r.clone());
        } else if cursor >= target {
            tail.push(r.clone());
        } else {
            let local = clamp_char_boundary(&r.text, target - cursor);
            if local > 0 {
                head.push(StyledRun {
                    text: r.text[..local].to_string(),
                    style: r.style,
                });
            }
            if local < len {
                tail.push(StyledRun {
                    text: r.text[local..].to_string(),
                    style: r.style,
                });
            }
        }
        cursor = end;
    }
    (normalize_runs(head), normalize_runs(tail))
}

fn toggle_style_in_range<F>(
    runs: &[StyledRun],
    start: usize,
    end: usize,
    field: &F,
) -> Vec<StyledRun>
where
    F: Fn(&mut InlineStyle) -> &mut bool,
{
    let (head, rest) = split_runs_at(runs, start);
    let (mid, tail) = split_runs_at(&rest, end.saturating_sub(start));

    let all_set = !mid.is_empty()
        && mid.iter().all(|r| {
            let mut copy = r.style;
            *field(&mut copy)
        });
    let target_val = !all_set;

    let mut out = head;
    for mut r in mid {
        *field(&mut r.style) = target_val;
        out.push(r);
    }
    out.extend(tail);
    normalize_runs(out)
}

fn clamp_char_boundary(s: &str, offset: usize) -> usize {
    let mut pos = offset.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn render_inlines_interactive(
    nodes: &[InlineNode],
    caret: Option<usize>,
    selection: Option<(usize, usize)>,
) -> String {
    let runs = inlines_to_runs(nodes);
    let total_len = flatten_runs(&runs).len();
    let caret_pos = caret.map(|c| c.min(total_len));
    let mut out = String::new();
    let mut cursor = 0;

    if runs.is_empty() && caret_pos == Some(0) {
        return "<mark class=\"odv-caret\">|</mark>".to_string();
    }

    for r in &runs {
        let len = r.text.len();
        let run_start = cursor;
        let run_end = cursor + len;

        // Collect split points within [run_start..=run_end]
        let mut cuts = vec![0usize, len];
        if let Some(cp) = caret_pos
            && cp >= run_start
            && cp <= run_end
        {
            cuts.push(clamp_char_boundary(&r.text, cp - run_start));
        }
        if let Some((s, e)) = selection {
            if s > run_start && s < run_end {
                cuts.push(clamp_char_boundary(&r.text, s - run_start));
            }
            if e > run_start && e < run_end {
                cuts.push(clamp_char_boundary(&r.text, e - run_start));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();

        for win in cuts.windows(2) {
            let a = win[0];
            let b = win[1];
            let global_a = run_start + a;
            if caret_pos == Some(global_a) && a == b {
                continue;
            }
            if caret_pos == Some(global_a) {
                out.push_str("<mark class=\"odv-caret\">|</mark>");
            }
            let slice = escape_html(&r.text[a..b]);
            let in_sel = selection.is_some_and(|(s, e)| global_a >= s && (run_start + b) <= e);
            let mut html = wrap_style(&slice, r.style);
            if in_sel {
                html = format!("<mark class=\"odv-selection\">{html}</mark>");
            }
            out.push_str(&html);
        }

        if caret_pos == Some(run_end) && run_end == total_len {
            out.push_str("<mark class=\"odv-caret\">|</mark>");
        }

        cursor = run_end;
    }

    out
}

fn wrap_style(escaped_text: &str, style: InlineStyle) -> String {
    let mut out = if style.code {
        format!("<code>{escaped_text}</code>")
    } else {
        escaped_text.to_string()
    };
    if style.italic {
        out = format!("<em>{out}</em>");
    }
    if style.bold {
        out = format!("<strong>{out}</strong>");
    }
    out
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

        editor.toggle_strong();
        let html = editor.render_main_inner_html(ViewMode::Document, "ODV", "F2");
        assert!(html.contains("<strong>Bonjour à tous</strong><mark class=\"odv-caret\">|</mark>"));

        editor.split_block();
        assert_eq!(editor.document().blocks.len(), 2);

        assert!(editor.undo());
        assert_eq!(editor.document().blocks.len(), 1);
    }

    #[test]
    fn contract_intrablock_selection_bold_italic_and_copy() {
        let mut doc = DocumentAst::new("Selection", "fr");
        let p_id = doc.push_paragraph("Hello semantic world", Visibility::All);
        let mut editor = EditorState::new(doc);
        editor.focus_block(p_id);

        // Move back 6 bytes (" world"), then select "semantic" (8 bytes)
        for _ in 0..6 {
            editor.move_left(false);
        }
        for _ in 0..8 {
            editor.move_left(true);
        }

        assert_eq!(editor.copy_text().as_deref(), Some("semantic"));

        // Apply bold + italic only to "semantic"
        editor.toggle_strong();
        editor.toggle_emphasis();

        let html = editor.render_main_inner_html(ViewMode::Document, "ODV", "F2");
        assert!(html.contains("Hello "));
        assert!(html.contains("<mark class=\"odv-caret\">|</mark><mark class=\"odv-selection\"><strong><em>semantic</em></strong></mark>"));
        assert!(html.contains(" world"));

        // Convert paragraph to h2 and back
        editor.set_active_block_heading(Some(2));
        assert!(matches!(
            editor.document().blocks[0],
            BlockNode::Heading { level: 2, .. }
        ));
        editor.set_active_block_heading(None);
        assert!(matches!(
            editor.document().blocks[0],
            BlockNode::Paragraph { .. }
        ));
    }

    #[test]
    fn contract_autotext_f3_expansion_and_micro_typo() {
        let mut doc = DocumentAst::new("AutoText", "fr");
        let p_id = doc.push_paragraph("", Visibility::All);
        let mut editor = EditorState::new(doc);
        editor.focus_block(p_id);

        for ch in ["o", "e", "u", "f", " ", "!", " "] {
            editor.insert_text_with_typo(ch);
        }
        assert_eq!(
            editor.active_block_plain_text().as_deref(),
            Some("œuf\u{00A0}! ")
        );

        for ch in ["f", "o", "c", "u", "s"] {
            editor.insert_text_with_typo(ch);
        }
        let catalog = AutoTextCatalog::default_catalog();
        assert!(editor.trigger_autotext(&catalog));
        assert!(matches!(
            editor.document().blocks.last(),
            Some(BlockNode::Aside { kind, .. }) if kind == "encadre-focus"
        ));
    }

    #[test]
    fn contract_word_navigation_home_end_select_all_and_delete() {
        let mut doc = DocumentAst::new("Nav", "fr");
        let p_id = doc.push_paragraph("Alpha Beta Gamma", Visibility::All);
        let mut editor = EditorState::new(doc);
        editor.focus_block(p_id);

        editor.move_home(false);
        assert_eq!(editor.cursor.unwrap().offset, 0);

        editor.move_word_right(true);
        assert_eq!(editor.copy_text().as_deref(), Some("Alpha "));

        editor.move_end(false);
        editor.move_word_left(true);
        assert_eq!(editor.copy_text().as_deref(), Some("Gamma"));

        editor.select_all_in_block();
        assert_eq!(editor.copy_text().as_deref(), Some("Alpha Beta Gamma"));

        editor.move_home(false);
        editor.delete_forward();
        assert_eq!(
            editor.active_block_plain_text().as_deref(),
            Some("lpha Beta Gamma")
        );
    }
}
