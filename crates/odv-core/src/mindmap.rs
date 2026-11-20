use crate::ast::{DocumentAst, MindMapItem, TopicStatus, Visibility, escape_html};
use roxmltree::{Document as XmlDoc, Node as XmlNode};
use std::io::{Cursor, Read};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum MindMapError {
    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("Missing content.xml in .xmind archive")]
    MissingContentXml,
    #[error("Missing map.xml in .vym archive")]
    MissingVymMapXml,
    #[error("No root topic or map node found")]
    EmptyMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindMapStructure {
    LogicalRight,
    Balanced,
    OrgChart,
}

/// Import a FreeMind / Freeplane / Docear / VYM `.mm` XML map into a `DocumentAst`.
pub fn import_freemind_mm(xml: &str, language: &str) -> Result<DocumentAst, MindMapError> {
    let xml_doc = XmlDoc::parse(xml)?;
    let map_node = xml_doc
        .descendants()
        .find(|n| n.has_tag_name("map"))
        .ok_or(MindMapError::EmptyMap)?;
    let root_topic = map_node
        .children()
        .find(|n| n.has_tag_name("node"))
        .ok_or(MindMapError::EmptyMap)?;

    let title = root_topic.attribute("TEXT").unwrap_or("Mind Map").trim();
    let mut doc = DocumentAst::new(title, language);
    walk_mm_node(root_topic, 1, &mut doc);
    Ok(doc)
}

fn walk_mm_node(node: XmlNode<'_, '_>, level: u8, doc: &mut DocumentAst) {
    let title = node.attribute("TEXT").unwrap_or("").trim();
    if !title.is_empty() {
        let note = node
            .children()
            .find(|c| c.has_tag_name("richcontent") && c.attribute("TYPE") == Some("NOTE"))
            .map(|n| collect_xml_text(n).trim().to_string())
            .filter(|s| !s.is_empty());
        doc.push_heading(level.clamp(1, 5), title, note);
    }
    for child in node.children().filter(|c| c.has_tag_name("node")) {
        walk_mm_node(child, (level + 1).min(5), doc);
    }
}

/// Import an XMind 8 `.xmind` ZIP archive (`content.xml`) into a `DocumentAst`.
pub fn import_xmind_bytes(bytes: &[u8], language: &str) -> Result<DocumentAst, MindMapError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut content_xml = String::new();
    {
        let mut entry = archive
            .by_name("content.xml")
            .map_err(|_| MindMapError::MissingContentXml)?;
        entry.read_to_string(&mut content_xml)?;
    }
    import_xmind_content_xml(&content_xml, language)
}

pub fn import_xmind_content_xml(xml: &str, language: &str) -> Result<DocumentAst, MindMapError> {
    let xml_doc = XmlDoc::parse(xml)?;
    let root_topic = xml_doc
        .descendants()
        .find(|n| n.has_tag_name("sheet"))
        .and_then(|s| s.children().find(|n| n.has_tag_name("topic")))
        .ok_or(MindMapError::EmptyMap)?;

    let root_title = topic_title(root_topic);
    let mut doc = DocumentAst::new(
        if root_title.is_empty() {
            "XMind Import"
        } else {
            &root_title
        },
        language,
    );
    walk_xmind_topic(root_topic, 1, Visibility::All, &mut doc);
    Ok(doc)
}

fn topic_title(topic: XmlNode<'_, '_>) -> String {
    topic
        .children()
        .find(|c| c.has_tag_name("title"))
        .map(|t| collect_xml_text(t).trim().to_string())
        .unwrap_or_default()
}

fn topic_note(topic: XmlNode<'_, '_>) -> Option<String> {
    topic
        .children()
        .find(|c| c.has_tag_name("notes"))
        .map(|n| collect_xml_text(n).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn walk_xmind_topic(
    topic: XmlNode<'_, '_>,
    level: u8,
    visibility: Visibility,
    doc: &mut DocumentAst,
) {
    let title = topic_title(topic);
    if !title.is_empty() {
        doc.push_heading_with_visibility(level.clamp(1, 5), title, topic_note(topic), visibility);
    }
    for children_el in topic.children().filter(|c| c.has_tag_name("children")) {
        for topics_el in children_el.children().filter(|c| c.has_tag_name("topics")) {
            let topic_type = topics_el.attribute("type").unwrap_or("attached");
            let child_vis = if topic_type == "detached" {
                Visibility::OnlyMindMap
            } else {
                visibility
            };
            let child_lvl = if topic_type == "detached" {
                2
            } else {
                (level + 1).min(5)
            };
            for sub in topics_el.children().filter(|c| c.has_tag_name("topic")) {
                walk_xmind_topic(sub, child_lvl, child_vis, doc);
            }
        }
    }
}

/// Import a VYM (`.vym` ZIP containing `map.xml`, or raw `.xml`) map into a `DocumentAst`.
/// Additional `<mapcenter>` roots are imported as floating Mind Map nodes (`Visibility::OnlyMindMap`).
pub fn import_vym_bytes(bytes: &[u8], language: &str) -> Result<DocumentAst, MindMapError> {
    let xml = if bytes.starts_with(b"PK") {
        let mut archive = ZipArchive::new(Cursor::new(bytes))?;
        let mut map_xml = String::new();
        let mut entry = archive
            .by_name("map.xml")
            .map_err(|_| MindMapError::MissingVymMapXml)?;
        entry.read_to_string(&mut map_xml)?;
        map_xml
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    import_vym_xml(&xml, language)
}

pub fn import_vym_xml(xml: &str, language: &str) -> Result<DocumentAst, MindMapError> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let xml_doc = XmlDoc::parse_with_options(xml, opts)?;
    let vymmap = xml_doc
        .descendants()
        .find(|n| n.has_tag_name("vymmap"))
        .ok_or(MindMapError::EmptyMap)?;
    let centers: Vec<XmlNode<'_, '_>> = vymmap
        .children()
        .filter(|n| n.has_tag_name("mapcenter"))
        .collect();
    if centers.is_empty() {
        return Err(MindMapError::EmptyMap);
    }

    let first_title = vym_heading(centers[0]);
    let mut doc = DocumentAst::new(
        if first_title.is_empty() {
            "VYM Import"
        } else {
            &first_title
        },
        language,
    );
    for (idx, center) in centers.into_iter().enumerate() {
        let vis = if idx == 0 {
            Visibility::All
        } else {
            Visibility::OnlyMindMap
        };
        walk_vym_branch(center, 1, vis, &mut doc);
    }
    Ok(doc)
}

fn vym_heading(node: XmlNode<'_, '_>) -> String {
    node.children()
        .find(|c| c.has_tag_name("heading"))
        .map(|h| collect_xml_text(h).trim().to_string())
        .unwrap_or_default()
}

fn vym_note(node: XmlNode<'_, '_>) -> Option<String> {
    node.children()
        .find(|c| c.has_tag_name("note") || c.has_tag_name("htmlnote"))
        .map(|n| collect_xml_text(n).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn walk_vym_branch(
    node: XmlNode<'_, '_>,
    level: u8,
    visibility: Visibility,
    doc: &mut DocumentAst,
) {
    let title = vym_heading(node);
    if !title.is_empty() {
        doc.push_heading_with_visibility(level.clamp(1, 5), title, vym_note(node), visibility);
    }
    for branch in node.children().filter(|c| c.has_tag_name("branch")) {
        walk_vym_branch(branch, (level + 1).min(5), visibility, doc);
    }
}

fn collect_xml_text(node: XmlNode<'_, '_>) -> String {
    let mut out = String::new();
    for desc in node.descendants() {
        if desc.is_text()
            && let Some(t) = desc.text()
        {
            out.push_str(t);
        }
    }
    out
}

/// Render a CSS-driven Mind Map tree with inline status badges and collapsed-branch hiding.
pub fn render_mindmap_tree_html(
    outline: &[MindMapItem],
    active_id: Option<crate::ast::NodeId>,
    structure: MindMapStructure,
) -> String {
    let struct_cls = match structure {
        MindMapStructure::LogicalRight => "mm-logical-right",
        MindMapStructure::Balanced => "mm-balanced",
        MindMapStructure::OrgChart => "mm-org-chart",
    };

    let mut out = format!("<section class=\"mm-canvas {struct_cls}\">\n");
    let mut collapsed_until_level: Option<u8> = None;

    for item in outline {
        if let Some(lim) = collapsed_until_level {
            if item.level > lim {
                continue;
            }
            collapsed_until_level = None;
        }
        if item.collapsed {
            collapsed_until_level = Some(item.level);
        }

        let is_active = active_id == Some(item.id);
        let active_cls = if is_active { " odv-active-block" } else { "" };
        let collapse_badge = if item.collapsed { " [+]" } else { "" };
        let status_badge = match item.status {
            Some(TopicStatus::Draft) => " <span class=\"mm-badge mm-draft\">DRAFT</span>",
            Some(TopicStatus::Review) => " <span class=\"mm-badge mm-review\">REVIEW</span>",
            Some(TopicStatus::Done) => " <span class=\"mm-badge mm-done\">DONE</span>",
            None => "",
        };

        out.push_str(&format!(
            "<article id=\"odv-{}\" class=\"mm-node mm-level-{} odv-block{active_cls}\">",
            item.id.0, item.level
        ));
        out.push_str(&format!(
            "<h{lvl}>{}{}{}</h{lvl}>",
            escape_html(&item.title),
            collapse_badge,
            status_badge,
            lvl = item.level.clamp(1, 5)
        ));
        if let Some(note) = &item.note {
            out.push_str(&format!(
                "<aside class=\"mindmap-note\">{}</aside>",
                escape_html(note)
            ));
        }
        out.push_str("</article>\n");
    }

    out.push_str("</section>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    #[test]
    fn contract_import_freemind_mm_preserves_hierarchy() {
        let mm = r#"<map version="1.0.1">
            <node TEXT="Projet ODV">
                <node TEXT="Architecture">
                    <richcontent TYPE="NOTE"><html><body><p>Note courte</p></body></html></richcontent>
                    <node TEXT="Rust Core"/>
                </node>
                <node TEXT="Vues"/>
            </node>
        </map>"#;

        let doc = import_freemind_mm(mm, "fr").unwrap();
        let outline = doc.mindmap_outline();
        assert_eq!(outline.len(), 4);
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].title, "Projet ODV");
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].note.as_deref(), Some("Note courte"));
        assert_eq!(outline[2].level, 3);
        assert_eq!(outline[2].title, "Rust Core");
        assert_eq!(outline[3].level, 2);
        assert_eq!(outline[3].title, "Vues");
    }

    #[test]
    fn contract_import_xmind_8_archive_preserves_topics_and_notes() {
        let content_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
        <xmap-content xmlns="urn:xmind:xmap:xmlns:content:2.0">
            <sheet id="s1">
                <topic id="t1">
                    <title>Sujet Central</title>
                    <notes><plain>Annotation centrale</plain></notes>
                    <children>
                        <topics type="attached">
                            <topic id="t2"><title>Branche 1</title></topic>
                            <topic id="t3"><title>Branche 2</title></topic>
                        </topics>
                    </children>
                </topic>
            </sheet>
        </xmap-content>"#;

        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("content.xml", opts).unwrap();
            zip.write_all(content_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let doc = import_xmind_bytes(&buf.into_inner(), "fr").unwrap();
        let outline = doc.mindmap_outline();
        assert_eq!(outline.len(), 3);
        assert_eq!(outline[0].title, "Sujet Central");
        assert_eq!(outline[0].note.as_deref(), Some("Annotation centrale"));
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].title, "Branche 1");
        assert_eq!(outline[2].level, 2);
        assert_eq!(outline[2].title, "Branche 2");
    }

    #[test]
    fn contract_xmind_detached_floating_topics_are_mindmap_only() {
        let content_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xmap-content xmlns="urn:xmind:xmap:xmlns:content:2.0">
            <sheet id="s1">
                <topic id="t1">
                    <title>Racine</title>
                    <children>
                        <topics type="attached"><topic id="t2"><title>Chapitre 1</title></topic></topics>
                        <topics type="detached"><topic id="t3"><title>Idee Flottante</title></topic></topics>
                    </children>
                </topic>
            </sheet>
        </xmap-content>"#;

        let doc = import_xmind_content_xml(content_xml, "fr").unwrap();
        let mm_html = doc.to_semantic_html(crate::ast::ViewMode::MindMap, "ODV", "F2");
        assert!(mm_html.contains("Idee Flottante"));

        let doc_html = doc.to_semantic_html(crate::ast::ViewMode::Document, "ODV", "F2");
        assert!(doc_html.contains("Chapitre 1"));
        assert!(!doc_html.contains("Idee Flottante"));
    }

    #[test]
    fn contract_import_real_effinergie_xmind_and_steve_jobs_vym() {
        let xmind_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../EFFINERGIE.xmind");
        if xmind_path.is_file() {
            let bytes = std::fs::read(xmind_path).unwrap();
            let doc = import_xmind_bytes(&bytes, "fr").unwrap();
            let outline = doc.mindmap_outline();
            assert_eq!(outline[0].title, "EFFINERGIE");
            assert!(outline.len() >= 5);
        }

        let vym_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../12 lessons from Steve Jobs.vym");
        if vym_path.is_file() {
            let bytes = std::fs::read(vym_path).unwrap();
            let doc = import_vym_bytes(&bytes, "en").unwrap();
            let outline = doc.mindmap_outline();
            assert_eq!(outline[0].title, "12 lessons from Steve Jobs");
            assert!(outline.iter().any(|it| it.title == "Youtube"));
            assert!(outline.iter().any(|it| it.title.contains("Design counts")));
        }
    }
}
