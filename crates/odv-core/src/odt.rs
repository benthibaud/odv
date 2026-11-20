use std::collections::HashMap;
use std::io::{Cursor, Read};

use roxmltree::Node;
use zip::ZipArchive;

use crate::ast::{BlockNode, BreakTarget, DocumentAst, InlineNode, Visibility};
use crate::document_view::plain_inlines;

#[derive(Debug, Clone, Copy, Default)]
struct OdtStyleProps {
    bold: bool,
    italic: bool,
    break_before_page: bool,
}

/// Import an OpenDocument Text (`.odt`) archive into a semantic `DocumentAst`.
pub fn import_odt_bytes(bytes: &[u8], default_language: &str) -> Result<DocumentAst, String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid ODT archive: {e}"))?;

    let mut title = String::new();
    let mut language = default_language.to_string();

    if let Ok(mut meta_file) = archive.by_name("meta.xml") {
        let mut meta_xml = String::new();
        if meta_file.read_to_string(&mut meta_xml).is_ok()
            && let Ok(meta_doc) = roxmltree::Document::parse(&meta_xml)
        {
            for node in meta_doc.descendants() {
                match node.tag_name().name() {
                    "title" => {
                        if let Some(t) = node.text()
                            && !t.trim().is_empty()
                        {
                            title = t.trim().to_string();
                        }
                    }
                    "language" => {
                        if let Some(l) = node.text()
                            && !l.trim().is_empty()
                        {
                            language = l.trim().to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let mut content_xml = String::new();
    {
        let mut content_file = archive
            .by_name("content.xml")
            .map_err(|e| format!("Missing content.xml in ODT: {e}"))?;
        content_file
            .read_to_string(&mut content_xml)
            .map_err(|e| format!("Failed to read content.xml: {e}"))?;
    }

    let xml = roxmltree::Document::parse(&content_xml)
        .map_err(|e| format!("Invalid content.xml in ODT: {e}"))?;

    let styles = parse_automatic_styles(&xml);
    let mut doc = DocumentAst::new(if title.is_empty() { "Document ODT" } else { &title }, language);

    let Some(office_text) = xml
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "text")
    else {
        return Err("Missing <office:text> in ODT content.xml".into());
    };

    for child in office_text.children().filter(|n| n.is_element()) {
        let style_name = attr_local(child, "style-name");
        if let Some(props) = style_name.and_then(|s| styles.get(s))
            && props.break_before_page
            && !doc.blocks.is_empty()
        {
            let pb_id = doc.alloc_id();
            doc.blocks.push(BlockNode::PageBreak {
                id: pb_id,
                target: BreakTarget::Print,
            });
        }

        match child.tag_name().name() {
            "h" => {
                let level = attr_local(child, "outline-level")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, 5);
                let inlines = collect_inlines(child, &styles, OdtStyleProps::default());
                if title.is_empty() && level == 1 {
                    let heading_plain = plain_inlines(&inlines);
                    if !heading_plain.is_empty() {
                        title = heading_plain.clone();
                        doc.title = heading_plain;
                    }
                }
                let id = doc.alloc_id();
                doc.blocks.push(BlockNode::Heading {
                    id,
                    level,
                    children: inlines,
                    note: None,
                    collapsed: false,
                    status: None,
                    visibility: Visibility::All,
                });
            }
            "p" => {
                let inlines = collect_inlines(child, &styles, OdtStyleProps::default());
                if !plain_inlines(&inlines).trim().is_empty() {
                    let id = doc.alloc_id();
                    doc.blocks.push(BlockNode::Paragraph {
                        id,
                        children: inlines,
                        visibility: Visibility::All,
                    });
                }
            }
            "list" => {
                let mut items = Vec::new();
                for li in child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "list-item")
                {
                    let mut item_inlines = Vec::new();
                    for p in li
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "p")
                    {
                        if !item_inlines.is_empty() {
                            item_inlines.push(InlineNode::Text { text: " ".into() });
                        }
                        item_inlines.extend(collect_inlines(p, &styles, OdtStyleProps::default()));
                    }
                    if !plain_inlines(&item_inlines).trim().is_empty() {
                        items.push(item_inlines);
                    }
                }
                if !items.is_empty() {
                    let id = doc.alloc_id();
                    doc.blocks.push(BlockNode::List {
                        id,
                        ordered: false,
                        items,
                        visibility: Visibility::All,
                    });
                }
            }
            _ => {}
        }
    }

    if doc.blocks.is_empty() {
        doc.push_paragraph("", Visibility::All);
    }

    Ok(doc)
}

fn parse_automatic_styles(xml: &roxmltree::Document<'_>) -> HashMap<String, OdtStyleProps> {
    let mut map = HashMap::new();
    for style_node in xml
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "style")
    {
        let Some(name) = attr_local(style_node, "name") else {
            continue;
        };
        let mut props = OdtStyleProps::default();
        for child in style_node.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "text-properties" => {
                    if attr_local(child, "font-weight") == Some("bold") {
                        props.bold = true;
                    }
                    if attr_local(child, "font-style") == Some("italic") {
                        props.italic = true;
                    }
                }
                "paragraph-properties" => {
                    if attr_local(child, "break-before") == Some("page") {
                        props.break_before_page = true;
                    }
                }
                _ => {}
            }
        }
        map.insert(name.to_string(), props);
    }
    map
}

fn collect_inlines(
    node: Node<'_, '_>,
    styles: &HashMap<String, OdtStyleProps>,
    inherited: OdtStyleProps,
) -> Vec<InlineNode> {
    let mut out = Vec::new();
    for child in node.children() {
        if child.is_text() {
            let txt = child.text().unwrap_or_default();
            if !txt.is_empty() {
                push_styled_text(&mut out, txt, inherited);
            }
        } else if child.is_element() {
            match child.tag_name().name() {
                "span" => {
                    let mut next_style = inherited;
                    if let Some(sname) = attr_local(child, "style-name")
                        && let Some(sprops) = styles.get(sname)
                    {
                        next_style.bold |= sprops.bold;
                        next_style.italic |= sprops.italic;
                    }
                    out.extend(collect_inlines(child, styles, next_style));
                }
                "s" => {
                    let count = attr_local(child, "c")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 32);
                    push_styled_text(&mut out, &" ".repeat(count), inherited);
                }
                "tab" => push_styled_text(&mut out, " ", inherited),
                "line-break" => push_styled_text(&mut out, "\n", inherited),
                "soft-page-break" => {}
                _ => out.extend(collect_inlines(child, styles, inherited)),
            }
        }
    }
    if out.is_empty() {
        out.push(InlineNode::Text {
            text: String::new(),
        });
    }
    out
}

fn push_styled_text(out: &mut Vec<InlineNode>, txt: &str, style: OdtStyleProps) {
    let leaf = InlineNode::Text {
        text: txt.to_string(),
    };
    let wrapped = match (style.bold, style.italic) {
        (true, true) => InlineNode::Strong {
            children: vec![InlineNode::Emphasis {
                children: vec![leaf],
            }],
        },
        (true, false) => InlineNode::Strong {
            children: vec![leaf],
        },
        (false, true) => InlineNode::Emphasis {
            children: vec![leaf],
        },
        (false, false) => leaf,
    };
    out.push(wrapped);
}

fn attr_local<'a>(node: Node<'a, '_>, local: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == local)
        .map(|a| a.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_view::{PaperFormat, paginate_document};
    use crate::pdf::export_document_pdf;

    #[test]
    fn contract_import_real_test_odt_preserves_headings_and_paragraphs() {
        let odt_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Test.odt");
        let bytes = std::fs::read(odt_path).expect("Test.odt should exist in workspace root");
        let doc = import_odt_bytes(&bytes, "fr").expect("Test.odt should parse cleanly");

        assert!(doc.title.contains("Performance énergétique"));
        let headings = doc
            .blocks
            .iter()
            .filter(|b| matches!(b, BlockNode::Heading { .. }))
            .count();
        let paragraphs = doc
            .blocks
            .iter()
            .filter(|b| matches!(b, BlockNode::Paragraph { .. }))
            .count();
        assert_eq!(headings, 6);
        assert_eq!(paragraphs, 15);

        let pages = paginate_document(&doc, PaperFormat::A4);
        assert!(
            pages.len() >= 2,
            "Test.odt should paginate into at least 2 A4 pages, got {}",
            pages.len()
        );
        let pdf = export_document_pdf(&doc, PaperFormat::A4);
        assert!(pdf.starts_with(b"%PDF-1.7"));
    }
}
