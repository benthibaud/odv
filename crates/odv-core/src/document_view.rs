use crate::ast::{BlockNode, BreakTarget, DocumentAst, InlineNode, Visibility, escape_html};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperFormat {
    A4,
    Letter,
}

impl PaperFormat {
    pub fn dimensions_pt(self) -> (f32, f32) {
        match self {
            Self::A4 => (595.28, 841.89),
            Self::Letter => (612.0, 792.0),
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::A4 => "paper-a4",
            Self::Letter => "paper-letter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintedPage {
    pub page_number: usize,
    pub blocks: Vec<BlockNode>,
    pub dl_window_address: Option<String>,
}

/// Paginate `DocumentAst` for Print/ODT (`A4` or `Letter`), keeping `OnlyPrint` blocks
/// (such as DL envelope window addresses) and stripping MindMap/Web/Epub-only blocks.
pub fn paginate_document(doc: &DocumentAst, paper: PaperFormat) -> Vec<PrintedPage> {
    let (w, h) = paper.dimensions_pt();
    let max_text_w = w - 2.0 * 56.7;
    let printable_height = h - 122.0; // 20mm top/bottom margins + header/footer clearance
    let mut pages: Vec<PrintedPage> = vec![PrintedPage {
        page_number: 1,
        blocks: Vec::new(),
        dl_window_address: None,
    }];
    let mut used_h: f32 = 0.0;

    for block in &doc.blocks {
        let vis = block.visibility();
        if !matches!(vis, Visibility::All | Visibility::OnlyPrint) {
            continue;
        }

        if let BlockNode::PageBreak {
            target: BreakTarget::Print,
            ..
        } = block
        {
            if !pages.last().unwrap().blocks.is_empty() || pages.last().unwrap().dl_window_address.is_some() {
                pages.push(PrintedPage {
                    page_number: pages.len() + 1,
                    blocks: Vec::new(),
                    dl_window_address: None,
                });
                used_h = 0.0;
            }
            continue;
        }

        // Ignore screen-only breaks in print mode
        if let BlockNode::PageBreak {
            target: BreakTarget::Screen,
            ..
        } = block
        {
            continue;
        }

        // DL window address block on page 1
        if let BlockNode::Aside {
            kind, children, ..
        } = block
            && kind == "destinataire-fenetre"
            && pages.len() == 1
            && pages[0].dl_window_address.is_none()
        {
            pages[0].dl_window_address = Some(plain_inlines(children));
            used_h = used_h.max(170.0);
            continue;
        }

        let est_h = estimate_block_height_pt(block, max_text_w);
        if used_h > 0.0 && used_h + est_h > printable_height {
            pages.push(PrintedPage {
                page_number: pages.len() + 1,
                blocks: Vec::new(),
                dl_window_address: None,
            });
            used_h = 0.0;
        }

        pages.last_mut().unwrap().blocks.push(block.clone());
        used_h += est_h;
    }

    pages
}

fn count_wrapped_lines(text: &str, max_width_pt: f32, font_size_pt: f32) -> f32 {
    let avg_char_w = font_size_pt * 0.48;
    let max_chars = ((max_width_pt / avg_char_w).floor() as usize).max(12);
    let mut lines = 0usize;
    for paragraph in text.lines() {
        let mut cur_len = 0usize;
        for word in paragraph.split_whitespace() {
            let wlen = word.chars().count();
            if cur_len == 0 {
                cur_len = wlen;
            } else if cur_len + 1 + wlen <= max_chars {
                cur_len += 1 + wlen;
            } else {
                lines += 1;
                cur_len = wlen;
            }
        }
        if cur_len > 0 {
            lines += 1;
        }
    }
    (lines.max(1)) as f32
}

fn estimate_block_height_pt(block: &BlockNode, max_text_w: f32) -> f32 {
    match block {
        BlockNode::Heading { level, children, .. } => {
            let sz: f32 = match level {
                1 => 20.0,
                2 => 16.0,
                3 => 13.5,
                _ => 12.0,
            };
            let lines = count_wrapped_lines(&plain_inlines(children), max_text_w, sz);
            lines * sz * 1.3 + 6.0
        }
        BlockNode::Paragraph { children, .. } | BlockNode::Blockquote { children, .. } => {
            let lines = count_wrapped_lines(&plain_inlines(children), max_text_w, 11.0);
            lines * 15.0 + 6.0
        }
        BlockNode::Aside { children, .. } => {
            let lines = count_wrapped_lines(&plain_inlines(children), max_text_w * 0.85, 11.0);
            lines * 15.0 + 24.0
        }
        BlockNode::List { items, .. } => (items.len() as f32) * 16.0 + 6.0,
        BlockNode::ThematicBreak { .. } => 14.0,
        BlockNode::PageBreak { .. } => 0.0,
    }
}

/// Render paginated HTML sheets for `ViewMode::Document`.
pub fn render_paginated_document_html(doc: &DocumentAst, paper: PaperFormat) -> String {
    let pages = paginate_document(doc, paper);
    let total = pages.len();
    let paper_cls = paper.css_class();
    let mut out = String::new();

    for page in pages {
        out.push_str(&format!("<section class=\"paper-sheet {paper_cls}\">\n"));
        out.push_str(&format!(
            "<header class=\"paper-header\">{}</header>\n",
            escape_html(&doc.title)
        ));
        if let Some(addr) = &page.dl_window_address {
            let formatted = escape_html(addr).replace('\n', "<br />");
            out.push_str(&format!(
                "<address class=\"destinataire-fenetre only-print\">{formatted}</address>\n"
            ));
        }
        for block in &page.blocks {
            out.push_str(&render_print_block_html(block));
        }
        out.push_str(&format!(
            "<footer class=\"paper-footer\">Page {} / {}</footer>\n",
            page.page_number, total
        ));
        out.push_str("</section>\n");
    }
    out
}

fn render_print_block_html(block: &BlockNode) -> String {
    match block {
        BlockNode::Heading {
            level, children, ..
        } => {
            let lvl = (*level).clamp(1, 5);
            format!("<h{lvl}>{}</h{lvl}>\n", escape_html(&plain_inlines(children)))
        }
        BlockNode::Paragraph { children, .. } => {
            format!("<p>{}</p>\n", escape_html(&plain_inlines(children)))
        }
        BlockNode::Blockquote { children, .. } => {
            format!(
                "<blockquote>{}</blockquote>\n",
                escape_html(&plain_inlines(children))
            )
        }
        BlockNode::List { ordered, items, .. } => {
            let tag = if *ordered { "ol" } else { "ul" };
            let mut s = format!("<{tag}>\n");
            for item in items {
                s.push_str(&format!("<li>{}</li>\n", escape_html(&plain_inlines(item))));
            }
            s.push_str(&format!("</{tag}>\n"));
            s
        }
        BlockNode::Aside {
            kind,
            title,
            children,
            ..
        } => {
            format!(
                "<aside class=\"{}\"><h4>{}</h4><p>{}</p></aside>\n",
                escape_html(kind),
                escape_html(title),
                escape_html(&plain_inlines(children))
            )
        }
        BlockNode::ThematicBreak { .. } => "<hr />\n".to_string(),
        BlockNode::PageBreak { .. } => String::new(),
    }
}

pub(crate) fn plain_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } | InlineNode::Code { text } => out.push_str(text),
            InlineNode::Strong { children } | InlineNode::Emphasis { children } => {
                out.push_str(&plain_inlines(children));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_document_pagination_print_breaks_and_dl_window() {
        let mut doc = DocumentAst::new("Courrier Officiel", "fr");
        let aside_id = doc.alloc_id();
        doc.blocks.push(BlockNode::Aside {
            id: aside_id,
            kind: "destinataire-fenetre".into(),
            title: "Destinataire".into(),
            children: vec![InlineNode::Text {
                text: "M. Dupont\n10 rue de la Paix\n75002 PARIS".into(),
            }],
            visibility: Visibility::OnlyPrint,
        });
        doc.push_heading(1, "Objet : Résiliation", Some("NOTE_MINDMAP_PRIVEE".into()));
        doc.push_paragraph("Corps page 1.", Visibility::All);
        doc.push_heading_with_visibility(2, "BROUILLON_FLOTTANT", None, Visibility::OnlyMindMap);
        doc.push_paragraph("CONTENU_WEB_UNIQUEMENT", Visibility::OnlyWeb);
        doc.push_page_break(BreakTarget::Print);
        doc.push_paragraph("Corps page 2.", Visibility::All);

        let pages = paginate_document(&doc, PaperFormat::A4);
        assert_eq!(pages.len(), 2);
        assert!(pages[0].dl_window_address.as_deref().unwrap().contains("75002 PARIS"));

        let html = render_paginated_document_html(&doc, PaperFormat::A4);
        assert!(html.contains("Page 1 / 2"));
        assert!(html.contains("Page 2 / 2"));
        assert!(html.contains("destinataire-fenetre"));
        assert!(!html.contains("NOTE_MINDMAP_PRIVEE"));
        assert!(!html.contains("BROUILLON_FLOTTANT"));
        assert!(!html.contains("CONTENU_WEB_UNIQUEMENT"));
    }
}
