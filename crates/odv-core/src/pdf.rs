use crate::ast::DocumentAst;
use crate::presentation::{ScreenRatio, build_slides};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfFormat {
    Screen16x9,
    ScreenMobile,
    A4,
    Letter,
}

impl PdfFormat {
    pub fn dimensions_pt(self) -> (f32, f32) {
        match self {
            Self::Screen16x9 => (960.0, 540.0),
            Self::ScreenMobile => (390.0, 693.0),
            Self::A4 => (595.28, 841.89),
            Self::Letter => (612.0, 792.0),
        }
    }
}

impl From<ScreenRatio> for PdfFormat {
    fn from(ratio: ScreenRatio) -> Self {
        match ratio {
            ScreenRatio::Desktop16x9 => Self::Screen16x9,
            ScreenRatio::MobilePortrait => Self::ScreenMobile,
        }
    }
}

/// Export a full-bleed screen PDF (`16:9` desktop or `9:16` smartphone portrait),
/// applying the same GDPR physical purge as `export_standalone_html`.
pub fn export_presentation_pdf(doc: &DocumentAst, ratio: ScreenRatio) -> Vec<u8> {
    let format: PdfFormat = ratio.into();
    let (page_w, page_h) = format.dimensions_pt();
    let slides = build_slides(doc);

    let mut pdf = Pdf::new();
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let font_regular_id = Ref::new(3);
    let font_bold_id = Ref::new(4);

    pdf.catalog(catalog_id).pages(page_tree_id);

    // Standard Type1 Helvetica & Helvetica-Bold with WinAnsiEncoding (supports French accents + œ + « »)
    pdf.type1_font(font_regular_id)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(font_bold_id)
        .base_font(Name(b"Helvetica-Bold"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    let mut next_id = 5_i32;
    let mut page_ids = Vec::with_capacity(slides.len());

    let margin = match ratio {
        ScreenRatio::Desktop16x9 => 36.0_f32,
        ScreenRatio::MobilePortrait => 20.0_f32,
    };
    let card_w = page_w - 2.0 * margin;
    let card_h = page_h - 2.0 * margin;
    let inner_pad = match ratio {
        ScreenRatio::Desktop16x9 => 32.0_f32,
        ScreenRatio::MobilePortrait => 18.0_f32,
    };
    let max_text_w = card_w - 2.0 * inner_pad;

    for (idx, slide) in slides.iter().enumerate() {
        let page_id = Ref::new(next_id);
        let content_id = Ref::new(next_id + 1);
        next_id += 2;
        page_ids.push(page_id);

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
        page.parent(page_tree_id);
        page.contents(content_id);
        {
            let mut resources = page.resources();
            let mut fonts = resources.fonts();
            fonts.pair(Name(b"F1"), font_regular_id);
            fonts.pair(Name(b"F2"), font_bold_id);
        }
        page.finish();

        let mut content = Content::new();

        // Full-bleed dark screen background (#0f172a)
        content.save_state();
        content.set_fill_rgb(0.059, 0.090, 0.165);
        content.rect(0.0, 0.0, page_w, page_h);
        content.fill_nonzero();
        content.restore_state();

        // Slide card surface (#1e293b) with accent border (#38bdf8)
        content.save_state();
        content.set_fill_rgb(0.118, 0.161, 0.231);
        content.rect(margin, margin, card_w, card_h);
        content.fill_nonzero();
        content.set_stroke_rgb(0.220, 0.741, 0.973);
        content.set_line_width(2.0);
        content.rect(margin, margin, card_w, card_h);
        content.stroke();
        content.restore_state();

        let text_x = margin + inner_pad;
        let mut cursor_y = page_h - margin - inner_pad - 12.0;

        // Slide counter
        let counter = format!("{} / {}", idx + 1, slides.len());
        draw_text_line(
            &mut content,
            Name(b"F1"),
            11.0,
            text_x,
            cursor_y,
            (0.58, 0.64, 0.72),
            &counter,
        );
        cursor_y -= 28.0;

        // Slide title
        let title_size = match ratio {
            ScreenRatio::Desktop16x9 => 28.0,
            ScreenRatio::MobilePortrait => 22.0,
        };
        for line in wrap_text(&slide.title, max_text_w, title_size) {
            if cursor_y < margin + inner_pad {
                break;
            }
            draw_text_line(
                &mut content,
                Name(b"F2"),
                title_size,
                text_x,
                cursor_y,
                (0.220, 0.741, 0.973),
                &line,
            );
            cursor_y -= title_size * 1.25;
        }
        cursor_y -= 10.0;

        // Keypoints / MindMap annotations
        let kp_size = match ratio {
            ScreenRatio::Desktop16x9 => 17.0,
            ScreenRatio::MobilePortrait => 14.5,
        };
        for kp in &slide.keypoints {
            for line in wrap_text(kp, max_text_w, kp_size) {
                if cursor_y < margin + inner_pad {
                    break;
                }
                draw_text_line(
                    &mut content,
                    Name(b"F1"),
                    kp_size,
                    text_x,
                    cursor_y,
                    (0.97, 0.98, 0.99),
                    &line,
                );
                cursor_y -= kp_size * 1.35;
            }
            cursor_y -= 6.0;
        }

        // Sub-items (differentiated indentation & font size for levels 3, 4, 5)
        let base_item_size = match ratio {
            ScreenRatio::Desktop16x9 => 15.5,
            ScreenRatio::MobilePortrait => 13.5,
        };
        for (lvl, text, note) in &slide.sub_items {
            let depth = lvl.saturating_sub(3) as f32;
            let indent = 8.0 + depth * 18.0;
            let item_size = (base_item_size - depth * 1.5).max(10.5);
            let marker = match lvl {
                3 => "•",
                4 => "-",
                _ => "›",
            };
            let bullet = match note {
                Some(n) => format!("{marker} {text} ({n})"),
                None => format!("{marker} {text}"),
            };
            let font_name = if *lvl <= 3 { Name(b"F2") } else { Name(b"F1") };
            for line in wrap_text(&bullet, (max_text_w - indent).max(80.0), item_size) {
                if cursor_y < margin + inner_pad {
                    break;
                }
                draw_text_line(
                    &mut content,
                    font_name,
                    item_size,
                    text_x + indent,
                    cursor_y,
                    (0.89, 0.91, 0.94),
                    &line,
                );
                cursor_y -= item_size * 1.35;
            }
            cursor_y -= (3.0 - depth).max(1.0);
        }

        pdf.stream(content_id, &content.finish());
    }

    pdf.pages(page_tree_id)
        .kids(page_ids)
        .count(slides.len() as i32);

    pdf.finish()
}

fn draw_text_line(
    content: &mut Content,
    font: Name<'_>,
    size: f32,
    x: f32,
    y: f32,
    rgb: (f32, f32, f32),
    text: &str,
) {
    let encoded = encode_winansi(text);
    content.begin_text();
    content.set_font(font, size);
    content.set_fill_rgb(rgb.0, rgb.1, rgb.2);
    content.next_line(x, y);
    content.show(pdf_writer::Str(&encoded));
    content.end_text();
}

fn wrap_text(text: &str, max_width_pt: f32, font_size_pt: f32) -> Vec<String> {
    let avg_char_w = font_size_pt * 0.52;
    let max_chars = ((max_width_pt / avg_char_w).floor() as usize).max(12);
    let mut lines = Vec::new();

    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

/// Encode UTF-8 string into Windows-1252 (WinAnsiEncoding) bytes for standard PDF Type1 fonts.
fn encode_winansi(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for ch in input.chars() {
        let b = match ch {
            '\u{0020}'..='\u{007E}' => ch as u8,
            '\u{00A0}'..='\u{00FF}' => ch as u8,
            '€' => 0x80,
            '‚' => 0x82,
            'ƒ' => 0x83,
            '„' => 0x84,
            '…' => 0x85,
            '†' => 0x86,
            '‡' => 0x87,
            'ˆ' => 0x88,
            '‰' => 0x89,
            'Š' => 0x8A,
            '‹' => 0x8B,
            'Œ' => 0x8C,
            'Ž' => 0x8E,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '˜' => 0x98,
            '™' => 0x99,
            'š' => 0x9A,
            '›' => 0x9B,
            'œ' => 0x9C,
            'ž' => 0x9E,
            'Ÿ' => 0x9F,
            _ => b'?',
        };
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Visibility;

    #[test]
    fn contract_presentation_pdf_16x9_and_mobile_dimensions_and_gdpr() {
        let mut doc = DocumentAst::new("PDF Pitch", "fr");
        doc.push_heading(1, "Écran d'ouverture", Some("Point clé : œuvre & cœur".into()));
        doc.push_paragraph("SECRET_POSTAL_ONLY_PRINT", Visibility::OnlyPrint);
        doc.push_heading_with_visibility(2, "BROUILLON_MINDMAP", None, Visibility::OnlyMindMap);
        doc.push_heading(2, "Deuxième écran", None);

        let pdf_16x9 = export_presentation_pdf(&doc, ScreenRatio::Desktop16x9);
        assert!(pdf_16x9.starts_with(b"%PDF-"));
        let raw_16x9 = String::from_utf8_lossy(&pdf_16x9);
        assert!(raw_16x9.contains("960 540"));
        assert!(raw_16x9.contains("/Count 2"));
        assert!(!raw_16x9.contains("SECRET_POSTAL_ONLY_PRINT"));
        assert!(!raw_16x9.contains("BROUILLON_MINDMAP"));

        let pdf_mobile = export_presentation_pdf(&doc, ScreenRatio::MobilePortrait);
        assert!(pdf_mobile.starts_with(b"%PDF-"));
        let raw_mobile = String::from_utf8_lossy(&pdf_mobile);
        assert!(raw_mobile.contains("390 693"));
        assert!(raw_mobile.contains("/Count 2"));
    }
}
