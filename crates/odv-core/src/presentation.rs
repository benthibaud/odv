use crate::ast::{BlockNode, BreakTarget, DocumentAst, InlineNode, NodeId, Visibility, escape_html};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenRatio {
    Desktop16x9,
    MobilePortrait,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    pub index: usize,
    pub heading_id: Option<NodeId>,
    pub title: String,
    pub level: u8,
    pub keypoints: Vec<String>,
    pub sub_items: Vec<(u8, String, Option<String>)>,
    pub slide_blocks_html: Vec<String>,
    pub presenter_notes: Vec<String>,
}

/// Split a single-source `DocumentAst` into presentation screens (`Slide`),
/// keeping headings + annotations on the main screen and explanatory prose in `presenter_notes`.
pub fn build_slides(doc: &DocumentAst) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();

    for block in &doc.blocks {
        let vis = block.visibility();
        // GDPR & MindMap purge: OnlyPrint and OnlyMindMap never enter Presentation
        if matches!(vis, Visibility::OnlyPrint | Visibility::OnlyMindMap) {
            continue;
        }

        match block {
            BlockNode::PageBreak {
                target: BreakTarget::Screen,
                ..
            } => {
                // Force next content to open a fresh slide if current has content
                if slides.last().is_some_and(|s| !s.title.is_empty() || !s.slide_blocks_html.is_empty()) {
                    slides.push(Slide {
                        index: slides.len(),
                        heading_id: None,
                        title: String::new(),
                        level: 2,
                        keypoints: Vec::new(),
                        sub_items: Vec::new(),
                        slide_blocks_html: Vec::new(),
                        presenter_notes: Vec::new(),
                    });
                }
            }
            BlockNode::Heading {
                id,
                level,
                children,
                note,
                ..
            } if *level <= 2 => {
                let title = plain_inlines(children);
                let should_reuse = slides.last().is_some_and(|s| s.title.is_empty() && s.slide_blocks_html.is_empty());
                if should_reuse {
                    if let Some(last) = slides.last_mut() {
                        last.heading_id = Some(*id);
                        last.title = title;
                        last.level = *level;
                        if let Some(n) = note {
                            last.keypoints.push(n.clone());
                        }
                    }
                } else {
                    let mut keypoints = Vec::new();
                    if let Some(n) = note {
                        keypoints.push(n.clone());
                    }
                    slides.push(Slide {
                        index: slides.len(),
                        heading_id: Some(*id),
                        title,
                        level: *level,
                        keypoints,
                        sub_items: Vec::new(),
                        slide_blocks_html: Vec::new(),
                        presenter_notes: Vec::new(),
                    });
                }
            }
            BlockNode::Heading {
                level,
                children,
                note,
                ..
            } => {
                ensure_slide(&mut slides, &doc.title);
                if let Some(last) = slides.last_mut() {
                    last.sub_items
                        .push((*level, plain_inlines(children), note.clone()));
                }
            }
            b => {
                ensure_slide(&mut slides, &doc.title);
                if let Some(last) = slides.last_mut() {
                    if vis == Visibility::OnlySlide {
                        if let Some(html) = render_slide_block_html(b) {
                            last.slide_blocks_html.push(html);
                        }
                    } else if let Some(note_text) = block_as_plain_text(b)
                        && !note_text.is_empty()
                    {
                        last.presenter_notes.push(note_text);
                    }
                }
            }
        }
    }

    if slides.is_empty() {
        slides.push(Slide {
            index: 0,
            heading_id: None,
            title: doc.title.clone(),
            level: 1,
            keypoints: Vec::new(),
            sub_items: Vec::new(),
            slide_blocks_html: Vec::new(),
            presenter_notes: Vec::new(),
        });
    }
    slides
}

fn ensure_slide(slides: &mut Vec<Slide>, fallback_title: &str) {
    if slides.is_empty() {
        slides.push(Slide {
            index: 0,
            heading_id: None,
            title: fallback_title.to_string(),
            level: 1,
            keypoints: Vec::new(),
            sub_items: Vec::new(),
            slide_blocks_html: Vec::new(),
            presenter_notes: Vec::new(),
        });
    }
}

/// Render a single slide for the interactive `odv-ui` Blitz viewport.
pub fn render_slide_stage_html(
    slides: &[Slide],
    active_index: usize,
    ratio: ScreenRatio,
    show_presenter_notes: bool,
) -> String {
    let total = slides.len().max(1);
    let idx = active_index.min(total - 1);
    let slide = &slides[idx];

    let ratio_cls = match ratio {
        ScreenRatio::Desktop16x9 => "slide-16x9",
        ScreenRatio::MobilePortrait => "slide-portrait",
    };

    let mut out = format!(
        "<section class=\"slide-deck\">\n<article class=\"slide-card {ratio_cls}\">\n"
    );
    out.push_str(&format!(
        "<p class=\"slide-counter\">{} / {}</p>\n",
        idx + 1,
        total
    ));
    if !slide.title.is_empty() {
        let id_attr = slide
            .heading_id
            .map(|id| format!(" id=\"odv-{}\"", id.0))
            .unwrap_or_default();
        out.push_str(&format!(
            "<h{}{id_attr} class=\"odv-block\">{}</h{}>\n",
            slide.level.clamp(1, 5),
            escape_html(&slide.title),
            slide.level.clamp(1, 5),
        ));
    }
    for kp in &slide.keypoints {
        out.push_str(&format!(
            "<p class=\"slide-keypoint\">{}</p>\n",
            escape_html(kp)
        ));
    }
    if !slide.sub_items.is_empty() {
        out.push_str("<ul class=\"slide-subitems\">\n");
        for (lvl, text, note) in &slide.sub_items {
            let clamped = (*lvl).clamp(3, 5);
            out.push_str(&format!(
                "<li class=\"slide-sub-lvl-{clamped}\">{}",
                escape_html(text)
            ));
            if let Some(n) = note {
                out.push_str(&format!(" — <em>{}</em>", escape_html(n)));
            }
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    }
    for html in &slide.slide_blocks_html {
        out.push_str(html);
    }
    out.push_str("</article>\n");

    if show_presenter_notes {
        out.push_str("<aside class=\"presenter-notes\">\n<h4>Notes présentateur</h4>\n");
        if slide.presenter_notes.is_empty() {
            out.push_str("<p class=\"presenter-empty\">(Aucune note explicative pour cet écran)</p>\n");
        } else {
            for note in &slide.presenter_notes {
                out.push_str(&format!("<p>{}</p>\n", escape_html(note)));
            }
        }
        out.push_str("</aside>\n");
    }

    out.push_str("</section>\n");
    out
}

/// Export a self-contained standalone HTML presentation (< 30 KB runtime)
/// with GDPR physical purge (`OnlyPrint` and `OnlyMindMap` stripped).
pub fn export_standalone_html(doc: &DocumentAst, ratio: ScreenRatio) -> String {
    let slides = build_slides(doc);
    let ratio_cls = match ratio {
        ScreenRatio::Desktop16x9 => "slide-16x9",
        ScreenRatio::MobilePortrait => "slide-portrait",
    };

    let mut slides_html = String::new();
    for (i, slide) in slides.iter().enumerate() {
        let active = if i == 0 { " active" } else { "" };
        slides_html.push_str(&format!(
            "<section class=\"slide-card {ratio_cls}{active}\" data-idx=\"{i}\">\n"
        ));
        slides_html.push_str(&format!(
            "<p class=\"slide-counter\">{} / {}</p>\n",
            i + 1,
            slides.len()
        ));
        if !slide.title.is_empty() {
            slides_html.push_str(&format!(
                "<h{lvl}>{}</h{lvl}>\n",
                escape_html(&slide.title),
                lvl = slide.level.clamp(1, 5)
            ));
        }
        for kp in &slide.keypoints {
            slides_html.push_str(&format!(
                "<p class=\"slide-keypoint\">{}</p>\n",
                escape_html(kp)
            ));
        }
        if !slide.sub_items.is_empty() {
            slides_html.push_str("<ul class=\"slide-subitems\">\n");
            for (lvl, text, note) in &slide.sub_items {
                let clamped = (*lvl).clamp(3, 5);
                slides_html.push_str(&format!(
                    "<li class=\"slide-sub-lvl-{clamped}\">{}",
                    escape_html(text)
                ));
                if let Some(n) = note {
                    slides_html.push_str(&format!(" — <em>{}</em>", escape_html(n)));
                }
                slides_html.push_str("</li>\n");
            }
            slides_html.push_str("</ul>\n");
        }
        for html in &slide.slide_blocks_html {
            slides_html.push_str(html);
        }
        if !slide.presenter_notes.is_empty() {
            slides_html.push_str("<aside class=\"presenter-notes\" hidden>\n");
            for note in &slide.presenter_notes {
                slides_html.push_str(&format!("<p>{}</p>\n", escape_html(note)));
            }
            slides_html.push_str("</aside>\n");
        }
        slides_html.push_str("</section>\n");
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
:root{{--bg:#0f172a;--surface:#1e293b;--ink:#f8fafc;--accent:#38bdf8}}
*{{box-sizing:border-box}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font-family:system-ui,sans-serif;padding:1.5rem}}
.slide-card{{display:none;flex-direction:column;justify-content:center;padding:2.5rem;background:var(--surface);border:2px solid var(--accent);border-radius:12px}}
.slide-card.active{{display:flex}}
.slide-16x9{{width:min(92vw,960px);aspect-ratio:16/9}}
.slide-portrait{{width:min(92vw,420px);aspect-ratio:9/16}}
.slide-counter{{font-size:.85rem;opacity:.65;margin:0 0 1rem}}
h1,h2{{color:var(--accent);margin:0 0 1rem;line-height:1.2}}
.slide-keypoint{{font-size:1.25rem;border-left:4px solid var(--accent);padding-left:1rem}}
.slide-subitems{{margin-top:1rem;padding-left:1.2rem;line-height:1.4}}
.slide-sub-lvl-3{{margin-left:0;font-size:1.15rem;font-weight:600;margin-top:.45rem}}
.slide-sub-lvl-4{{margin-left:1.5rem;font-size:1.02rem;font-weight:500;opacity:.92;margin-top:.25rem}}
.slide-sub-lvl-5{{margin-left:3rem;font-size:.92rem;font-weight:400;opacity:.82;list-style-type:circle;margin-top:.15rem}}
.presenter-notes{{margin-top:1.5rem;padding:1rem;background:rgba(0,0,0,.3);border-radius:8px;font-size:.95rem}}
.nav-hint{{position:fixed;bottom:10px;right:16px;font-size:.8rem;opacity:.6}}
</style>
</head>
<body>
{slides_html}
<div class="nav-hint">← / → : navigation | P : notes présentateur</div>
<script>
(()=>{{const s=[...document.querySelectorAll('.slide-card')];let i=0,n=false;
const show=k=>{{i=Math.max(0,Math.min(s.length-1,k));s.forEach((e,j)=>e.classList.toggle('active',j===i))}};
addEventListener('keydown',e=>{{if(e.key==='ArrowRight'||e.key===' ')show(i+1);else if(e.key==='ArrowLeft')show(i-1);else if(e.key==='p'||e.key==='P'){{n=!n;document.querySelectorAll('.presenter-notes').forEach(x=>x.hidden=!n)}}}})}})();
</script>
</body>
</html>"#,
        lang = escape_html(&doc.language),
        title = escape_html(&doc.title),
    )
}

fn plain_inlines(nodes: &[InlineNode]) -> String {
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

fn block_as_plain_text(block: &BlockNode) -> Option<String> {
    match block {
        BlockNode::Paragraph { children, .. } | BlockNode::Blockquote { children, .. } => {
            Some(plain_inlines(children))
        }
        BlockNode::Aside {
            title, children, ..
        } => Some(format!("{title} : {}", plain_inlines(children))),
        BlockNode::List { items, .. } => {
            let joined: Vec<String> = items.iter().map(|it| plain_inlines(it)).collect();
            Some(joined.join(" • "))
        }
        _ => None,
    }
}

fn render_slide_block_html(block: &BlockNode) -> Option<String> {
    match block {
        BlockNode::Paragraph { children, .. } => {
            Some(format!("<p>{}</p>\n", escape_html(&plain_inlines(children))))
        }
        BlockNode::Blockquote { children, .. } => Some(format!(
            "<blockquote>{}</blockquote>\n",
            escape_html(&plain_inlines(children))
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_slide_splitting_and_presenter_notes_mapping() {
        let mut doc = DocumentAst::new("Keynote", "fr");
        doc.push_heading(1, "Slide 1", Some("Point clé 1".into()));
        doc.push_paragraph("Explication orateur 1", Visibility::All);
        doc.push_heading(3, "Sous-point 1.1", Some("Détail".into()));
        doc.push_heading(2, "Slide 2", Some("Point clé 2".into()));
        doc.push_paragraph("Explication orateur 2", Visibility::All);

        let slides = build_slides(&doc);
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].title, "Slide 1");
        assert_eq!(slides[0].keypoints, vec!["Point clé 1"]);
        assert_eq!(slides[0].sub_items.len(), 1);
        assert_eq!(slides[0].presenter_notes, vec!["Explication orateur 1"]);
        assert_eq!(slides[1].title, "Slide 2");
        assert_eq!(slides[1].presenter_notes, vec!["Explication orateur 2"]);

        let mut doc_sub = DocumentAst::new("Hierarchy", "fr");
        doc_sub.push_heading(1, "Titre", None);
        doc_sub.push_heading(3, "Niv 3", None);
        doc_sub.push_heading(4, "Niv 4", None);
        doc_sub.push_heading(5, "Niv 5", None);
        let slides_sub = build_slides(&doc_sub);
        let stage = render_slide_stage_html(&slides_sub, 0, ScreenRatio::Desktop16x9, false);
        assert!(stage.contains("class=\"slide-sub-lvl-3\""));
        assert!(stage.contains("class=\"slide-sub-lvl-4\""));
        assert!(stage.contains("class=\"slide-sub-lvl-5\""));
    }

    #[test]
    fn contract_standalone_html_export_under_30kb_and_gdpr_purged() {
        let mut doc = DocumentAst::new("Confidential Pitch", "fr");
        doc.push_heading(1, "Offre Publique", Some("Synthèse".into()));
        doc.push_paragraph("SECRET_POSTAL_ADDRESS_LRAR", Visibility::OnlyPrint);
        doc.push_heading_with_visibility(2, "BROUILLON_FLOTTANT", None, Visibility::OnlyMindMap);
        doc.push_paragraph("Discours présentateur public.", Visibility::All);

        let html = export_standalone_html(&doc, ScreenRatio::Desktop16x9);
        assert!(html.len() < 30 * 1024, "Standalone HTML runtime must stay < 30 KB");
        assert!(html.contains("Offre Publique"));
        assert!(html.contains("Discours présentateur public."));
        assert!(!html.contains("SECRET_POSTAL_ADDRESS_LRAR"));
        assert!(!html.contains("BROUILLON_FLOTTANT"));
    }
}
