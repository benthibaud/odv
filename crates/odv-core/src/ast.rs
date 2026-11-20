use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Stable node identifier independent of slice indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Target visibility scope for multi-support publishing and GDPR physical purge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    #[default]
    All,
    OnlyPrint,
    OnlyWeb,
    OnlyEpub,
    OnlySlide,
}

/// Target medium for an explicit page/screen break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BreakTarget {
    Print,
    Screen,
}

/// Semantic projection mode for the single-source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    MindMap,
    Document,
    Epub,
    Presentation,
    PresenterNotes,
    Web,
}

/// Semantic inline span inside a block (no arbitrary formatting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineNode {
    Text { text: String },
    Strong { children: Vec<InlineNode> },
    Emphasis { children: Vec<InlineNode> },
    Code { text: String },
}

/// Semantic block node allowed by the guard rails (no arbitrary div/span).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    Heading {
        id: NodeId,
        level: u8,
        children: Vec<InlineNode>,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        visibility: Visibility,
    },
    Paragraph {
        id: NodeId,
        children: Vec<InlineNode>,
        #[serde(default)]
        visibility: Visibility,
    },
    Blockquote {
        id: NodeId,
        children: Vec<InlineNode>,
        #[serde(default)]
        visibility: Visibility,
    },
    List {
        id: NodeId,
        ordered: bool,
        items: Vec<Vec<InlineNode>>,
        #[serde(default)]
        visibility: Visibility,
    },
    Aside {
        id: NodeId,
        kind: String,
        title: String,
        children: Vec<InlineNode>,
        #[serde(default)]
        visibility: Visibility,
    },
    PageBreak {
        id: NodeId,
        target: BreakTarget,
    },
    ThematicBreak {
        id: NodeId,
    },
}

impl BlockNode {
    pub fn id(&self) -> NodeId {
        match self {
            Self::Heading { id, .. }
            | Self::Paragraph { id, .. }
            | Self::Blockquote { id, .. }
            | Self::List { id, .. }
            | Self::Aside { id, .. }
            | Self::PageBreak { id, .. }
            | Self::ThematicBreak { id } => *id,
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Self::Heading { visibility, .. }
            | Self::Paragraph { visibility, .. }
            | Self::Blockquote { visibility, .. }
            | Self::List { visibility, .. }
            | Self::Aside { visibility, .. } => *visibility,
            Self::PageBreak { .. } => Visibility::All,
            Self::ThematicBreak { .. } => Visibility::All,
        }
    }
}

/// Lightweight derived node for Mind Map projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindMapItem {
    pub id: NodeId,
    pub level: u8,
    pub title: String,
    pub note: Option<String>,
    pub start_index: usize,
    pub end_index_exclusive: usize,
}

/// Root semantic document AST (Option A: flat ordered block list with stable NodeIds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAst {
    pub schema_version: u32,
    pub next_node_id: u64,
    pub title: String,
    pub language: String,
    pub blocks: Vec<BlockNode>,
}

impl DocumentAst {
    pub fn new(title: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            next_node_id: 1,
            title: title.into(),
            language: language.into(),
            blocks: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn push_heading(
        &mut self,
        level: u8,
        text: impl Into<String>,
        note: Option<String>,
    ) -> NodeId {
        let id = self.alloc_id();
        self.blocks.push(BlockNode::Heading {
            id,
            level: level.clamp(1, 5),
            children: vec![InlineNode::Text { text: text.into() }],
            note,
            visibility: Visibility::All,
        });
        id
    }

    pub fn push_paragraph(&mut self, text: impl Into<String>, visibility: Visibility) -> NodeId {
        let id = self.alloc_id();
        self.blocks.push(BlockNode::Paragraph {
            id,
            children: vec![InlineNode::Text { text: text.into() }],
            visibility,
        });
        id
    }

    pub fn push_page_break(&mut self, target: BreakTarget) -> NodeId {
        let id = self.alloc_id();
        self.blocks.push(BlockNode::PageBreak { id, target });
        id
    }

    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.blocks.iter().position(|b| b.id() == id)
    }

    pub fn block_mut(&mut self, id: NodeId) -> Option<&mut BlockNode> {
        self.blocks.iter_mut().find(|b| b.id() == id)
    }

    /// Derive the Mind Map outline: only headings (`h1`-`h5`) and their short notes.
    /// Page breaks and body paragraphs are invisible here, while each item tracks
    /// the full contiguous slice `[start_index..end_index_exclusive]` of attached blocks.
    pub fn mindmap_outline(&self) -> Vec<MindMapItem> {
        let mut items = Vec::new();
        for (idx, block) in self.blocks.iter().enumerate() {
            if let BlockNode::Heading {
                id,
                level,
                children,
                note,
                ..
            } = block
            {
                let mut end = self.blocks.len();
                for (j, next) in self.blocks.iter().enumerate().skip(idx + 1) {
                    if let BlockNode::Heading {
                        level: next_lvl, ..
                    } = next
                        && *next_lvl <= *level
                    {
                        end = j;
                        break;
                    }
                }
                items.push(MindMapItem {
                    id: *id,
                    level: *level,
                    title: plain_text_inlines(children),
                    note: note.clone(),
                    start_index: idx,
                    end_index_exclusive: end,
                });
            }
        }
        items
    }

    /// Move an entire Mind Map branch (heading + sub-headings + attached explanatory blocks
    /// and page breaks) to `target_block_index` in the flat block list.
    pub fn move_branch(&mut self, heading_id: NodeId, target_block_index: usize) -> bool {
        let Some(item) = self
            .mindmap_outline()
            .into_iter()
            .find(|it| it.id == heading_id)
        else {
            return false;
        };

        let start = item.start_index;
        let end = item.end_index_exclusive;
        if target_block_index >= start && target_block_index <= end {
            return false;
        }

        let branch: Vec<BlockNode> = self.blocks.drain(start..end).collect();
        let insert_at = if target_block_index > start {
            (target_block_index - branch.len()).min(self.blocks.len())
        } else {
            target_block_index.min(self.blocks.len())
        };
        self.blocks.splice(insert_at..insert_at, branch);
        true
    }

    /// Render semantic HTML for the requested view mode, enforcing GDPR physical purge
    /// for public/web/epub targets.
    pub fn to_semantic_html(&self, mode: ViewMode, title_label: &str, hint_label: &str) -> String {
        let mut body = String::new();

        for block in &self.blocks {
            if !is_visible_in_mode(block.visibility(), mode) {
                continue;
            }

            match (mode, block) {
                // MindMap: headings + short annotations only; page breaks and body text are invisible.
                (
                    ViewMode::MindMap,
                    BlockNode::Heading {
                        level,
                        children,
                        note,
                        ..
                    },
                ) => {
                    let lvl = (*level).clamp(1, 5);
                    body.push_str(&format!("<h{lvl}>{}</h{lvl}>\n", render_inlines(children)));
                    if let Some(n) = note {
                        body.push_str(&format!(
                            "<aside class=\"mindmap-note\">{}</aside>\n",
                            escape_html(n)
                        ));
                    }
                }
                (ViewMode::MindMap, _) => {}

                // Presentation main screen: headings + annotations (and OnlySlide blocks);
                // explanatory paragraphs act as presenter notes on the secondary screen.
                (
                    ViewMode::Presentation,
                    BlockNode::Heading {
                        level,
                        children,
                        note,
                        ..
                    },
                ) => {
                    let lvl = (*level).clamp(1, 5);
                    body.push_str(&format!("<h{lvl}>{}</h{lvl}>\n", render_inlines(children)));
                    if let Some(n) = note {
                        body.push_str(&format!(
                            "<p class=\"slide-keypoint\">{}</p>\n",
                            escape_html(n)
                        ));
                    }
                }
                (
                    ViewMode::Presentation,
                    BlockNode::PageBreak {
                        target: BreakTarget::Screen,
                        ..
                    },
                ) => {
                    body.push_str("<hr class=\"screen-break\" />\n");
                }
                (ViewMode::Presentation, b) if b.visibility() == Visibility::OnlySlide => {
                    append_standard_block(&mut body, b, mode);
                }
                (ViewMode::Presentation, _) => {}

                // Presenter notes (secondary screen): heading context + explanatory text.
                (ViewMode::PresenterNotes, BlockNode::Heading { level, children, .. }) => {
                    let lvl = (*level).clamp(1, 5);
                    body.push_str(&format!("<h{lvl}>{}</h{lvl}>\n", render_inlines(children)));
                }
                (ViewMode::PresenterNotes, b) => {
                    append_standard_block(&mut body, b, mode);
                }

                // Document / Epub / Web
                (_, b) => {
                    append_standard_block(&mut body, b, mode);
                }
            }
        }

        format!(
            r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<title>{title}</title>
</head>
<body>
<main>
<header class="odv-toolbar">{title_label} — {hint_label}</header>
{body}
</main>
</body>
</html>"#,
            lang = escape_html(&self.language),
            title = escape_html(&self.title),
            title_label = escape_html(title_label),
            hint_label = escape_html(hint_label),
        )
    }
}

fn is_visible_in_mode(vis: Visibility, mode: ViewMode) -> bool {
    match (vis, mode) {
        (Visibility::All, _) => true,
        // GDPR physical purge: OnlyPrint blocks are stripped from Web and Epub
        (Visibility::OnlyPrint, ViewMode::Document) => true,
        (Visibility::OnlyPrint, _) => false,
        (Visibility::OnlyWeb, ViewMode::Web) => true,
        (Visibility::OnlyWeb, _) => false,
        (Visibility::OnlyEpub, ViewMode::Epub) => true,
        (Visibility::OnlyEpub, _) => false,
        (Visibility::OnlySlide, ViewMode::Presentation) => true,
        (Visibility::OnlySlide, _) => false,
    }
}

fn append_standard_block(body: &mut String, block: &BlockNode, mode: ViewMode) {
    match block {
        BlockNode::Heading {
            level, children, ..
        } => {
            let lvl = (*level).clamp(1, 5);
            body.push_str(&format!("<h{lvl}>{}</h{lvl}>\n", render_inlines(children)));
        }
        BlockNode::Paragraph { children, .. } => {
            body.push_str(&format!("<p>{}</p>\n", render_inlines(children)));
        }
        BlockNode::Blockquote { children, .. } => {
            body.push_str(&format!(
                "<blockquote>{}</blockquote>\n",
                render_inlines(children)
            ));
        }
        BlockNode::List { ordered, items, .. } => {
            let tag = if *ordered { "ol" } else { "ul" };
            body.push_str(&format!("<{tag}>\n"));
            for item in items {
                body.push_str(&format!("<li>{}</li>\n", render_inlines(item)));
            }
            body.push_str(&format!("</{tag}>\n"));
        }
        BlockNode::Aside {
            kind,
            title,
            children,
            ..
        } => {
            body.push_str(&format!(
                "<aside class=\"{}\"><h4>{}</h4><p>{}</p></aside>\n",
                escape_html(kind),
                escape_html(title),
                render_inlines(children)
            ));
        }
        BlockNode::PageBreak { target, .. } => match (target, mode) {
            (BreakTarget::Print, ViewMode::Document) => {
                body.push_str("<hr class=\"page-break\" />\n");
            }
            (BreakTarget::Screen, ViewMode::Presentation) => {
                body.push_str("<hr class=\"screen-break\" />\n");
            }
            _ => {}
        },
        BlockNode::ThematicBreak { .. } => {
            body.push_str("<hr />\n");
        }
    }
}

fn plain_text_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } | InlineNode::Code { text } => out.push_str(text),
            InlineNode::Strong { children } | InlineNode::Emphasis { children } => {
                out.push_str(&plain_text_inlines(children));
            }
        }
    }
    out
}

fn render_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } => out.push_str(&escape_html(text)),
            InlineNode::Strong { children } => {
                out.push_str(&format!("<strong>{}</strong>", render_inlines(children)));
            }
            InlineNode::Emphasis { children } => {
                out.push_str(&format!("<em>{}</em>", render_inlines(children)));
            }
            InlineNode::Code { text } => {
                out.push_str(&format!("<code>{}</code>", escape_html(text)));
            }
        }
    }
    out
}

pub(crate) fn escape_html(input: &str) -> String {
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
    fn renders_semantic_html_without_arbitrary_divs() {
        let mut doc = DocumentAst::new("Sample", "en");
        doc.push_heading(1, "Chapter 1", None);
        let p_id = doc.alloc_id();
        doc.blocks.push(BlockNode::Paragraph {
            id: p_id,
            children: vec![InlineNode::Strong {
                children: vec![InlineNode::Text {
                    text: "Semantic".into(),
                }],
            }],
            visibility: Visibility::All,
        });

        let html = doc.to_semantic_html(ViewMode::Document, "ODV", "F2");
        assert!(html.contains("<h1>Chapter 1</h1>"));
        assert!(html.contains("<p><strong>Semantic</strong></p>"));
        assert!(!html.contains("<div"));
        assert!(!html.contains("<span"));
    }

    #[test]
    fn contract_mindmap_branch_move_preserves_attached_explanations_and_ignores_pagebreaks() {
        let mut doc = DocumentAst::new("Branch Move", "fr");
        let h1_a = doc.push_heading(1, "Part A", Some("Note A".into()));
        doc.push_paragraph("Explanation A", Visibility::All);
        doc.push_page_break(BreakTarget::Print);

        let h1_b = doc.push_heading(1, "Part B", Some("Note B".into()));
        doc.push_heading(2, "Sub B1", None);
        doc.push_paragraph("Explanation B1", Visibility::All);

        // Mind map must ignore page breaks and body paragraphs
        let mindmap_html = doc.to_semantic_html(ViewMode::MindMap, "ODV", "F2");
        assert!(!mindmap_html.contains("page-break"));
        assert!(!mindmap_html.contains("Explanation A"));
        assert!(mindmap_html.contains("Part A"));
        assert!(mindmap_html.contains("Note B"));

        // Move Part B (including Sub B1 + Explanation B1) before Part A
        assert!(doc.move_branch(h1_b, 0));

        let outline = doc.mindmap_outline();
        assert_eq!(outline.len(), 3);
        assert_eq!(outline[0].id, h1_b);
        assert_eq!(outline[1].title, "Sub B1");
        assert_eq!(outline[2].id, h1_a);

        // Explanatory text and page break stayed attached to their respective branches
        let doc_html = doc.to_semantic_html(ViewMode::Document, "ODV", "F2");
        let pos_b1 = doc_html.find("Explanation B1").unwrap();
        let pos_a = doc_html.find("Explanation A").unwrap();
        let pos_pb = doc_html.find("page-break").unwrap();
        assert!(pos_b1 < pos_a);
        assert!(pos_a < pos_pb);
    }

    #[test]
    fn contract_presentation_and_presenter_notes_separation() {
        let mut doc = DocumentAst::new("Keynote", "fr");
        doc.push_heading(1, "Vision", Some("3 key points on screen".into()));
        doc.push_paragraph("Detailed speaker script for secondary monitor.", Visibility::All);

        let slide_html = doc.to_semantic_html(ViewMode::Presentation, "ODV", "F2");
        assert!(slide_html.contains("<h1>Vision</h1>"));
        assert!(slide_html.contains("3 key points on screen"));
        assert!(!slide_html.contains("Detailed speaker script"));

        let notes_html = doc.to_semantic_html(ViewMode::PresenterNotes, "ODV", "F2");
        assert!(notes_html.contains("<h1>Vision</h1>"));
        assert!(notes_html.contains("Detailed speaker script for secondary monitor."));
    }

    #[test]
    fn contract_gdpr_physical_purge_on_web_export() {
        let mut doc = DocumentAst::new("Letter", "fr");
        doc.push_heading(1, "Notice", None);
        doc.push_paragraph("PRIVATE_HOME_ADDRESS_LRAR_12345", Visibility::OnlyPrint);
        doc.push_paragraph("Public summary", Visibility::All);

        let print_html = doc.to_semantic_html(ViewMode::Document, "ODV", "F2");
        assert!(print_html.contains("PRIVATE_HOME_ADDRESS_LRAR_12345"));

        let web_html = doc.to_semantic_html(ViewMode::Web, "ODV", "F2");
        assert!(!web_html.contains("PRIVATE_HOME_ADDRESS_LRAR_12345"));
        assert!(web_html.contains("Public summary"));

        let epub_html = doc.to_semantic_html(ViewMode::Epub, "ODV", "F2");
        assert!(!epub_html.contains("PRIVATE_HOME_ADDRESS_LRAR_12345"));
    }
}
