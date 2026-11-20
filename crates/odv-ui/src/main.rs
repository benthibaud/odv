mod clipboard;

use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocumentConfig, UserAgentStylesheet};
use blitz_html::HtmlDocument;
use blitz_shell::{
    BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use clipboard::PersistentClipboard;
use odv_core::ast::{BlockNode, BreakTarget, DocumentAst, InlineNode, NodeId, ViewMode, Visibility};
use odv_core::document_view::PaperFormat;
use odv_core::editor::EditorState;
use odv_core::format::OdvPackage;
use odv_core::i18n::{Locale, set_locale, tr};
use odv_core::mindmap::{import_freemind_mm, import_vym_bytes, import_xmind_bytes};
use odv_core::odt::import_odt_bytes;
use odv_core::pdf::{export_document_pdf, export_presentation_pdf};
use odv_core::presentation::{ScreenRatio, build_slides, export_standalone_html};
use odv_core::typo::AutoTextCatalog;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Modifiers, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{WindowAttributes, WindowId};

static TOKENS_CSS: &str = include_str!("../../../styles/tokens.css");
static DOCUMENT_CSS: &str = include_str!("../../../styles/document.css");
static PRESENTATION_CSS: &str = include_str!("../../../styles/presentation.css");
static MINDMAP_CSS: &str = include_str!("../../../styles/mindmap.css");

struct OdvApp {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    active_view: ViewMode,
    last_synced_view: ViewMode,
    active_slide: usize,
    screen_ratio: ScreenRatio,
    paper_format: PaperFormat,
    show_presenter_notes: bool,
    editor: EditorState,
    modifiers: Modifiers,
    cursor_pos: (f32, f32),
    is_dragging: bool,
    is_panning: bool,
    pan_last_pos: (f32, f32),
    last_click: Option<(std::time::Instant, (f32, f32))>,
    font_ctx: parley::FontContext,
    autotexts: AutoTextCatalog,
    tokens_sheet: UserAgentStylesheet,
    document_sheet: UserAgentStylesheet,
    presentation_sheet: UserAgentStylesheet,
    mindmap_sheet: UserAgentStylesheet,
    clipboard: PersistentClipboard,
}

impl OdvApp {
    fn view_label(&self) -> String {
        match self.active_view {
            ViewMode::Document => tr("Document View"),
            ViewMode::MindMap => "Carte Mentale (h1-h5)".to_string(),
            ViewMode::Presentation => tr("Screen View"),
            _ => tr("Document View"),
        }
    }

    fn sync_dom(&mut self, window_id: WindowId) {
        let label = self.view_label();
        let inner_html = if self.active_view == ViewMode::Presentation {
            self.editor.render_presentation_inner_html(
                self.active_slide,
                self.screen_ratio,
                self.show_presenter_notes,
                &label,
                "F2: vue | PgUp/PgDn: écran | F5: 16:9/Mobile | F6: notes | Ctrl+E: HTML | Ctrl+P: PDF",
            )
        } else if self.active_view == ViewMode::Document {
            self.editor.render_document_inner_html(
                self.paper_format,
                &label,
                "F2: vue | F5: A4/Letter | Ctrl+Entrée: saut de page | Ctrl+P: PDF | Ctrl+B/I | Ctrl+Z/Y | Ctrl+C/V",
            )
        } else {
            self.editor.render_main_inner_html(
                self.active_view,
                &label,
                "F2: vue | Tab/Entrée/F4: carte | Shift+←/→: sélect. | Ctrl+B/I | Ctrl+Z/Y | Ctrl+C/V",
            )
        };
        if self.active_view == self.last_synced_view
            && let Some(view) = self.inner.windows.get_mut(&window_id)
        {
            let mut doc = view.doc.inner_mut();
            if let Some(main_id) = doc.get_element_by_id("odv-main") {
                doc.mutate().set_inner_html(main_id, &inner_html);
                drop(doc);
                view.request_redraw();
                return;
            }
        }

        let html = format!(
            "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\"></head><body><main id=\"odv-main\">{inner_html}</main></body></html>"
        );
        let ua_stylesheets = match self.active_view {
            ViewMode::MindMap => vec![
                UserAgentStylesheet::default_sheet(),
                self.tokens_sheet.clone(),
                self.mindmap_sheet.clone(),
            ],
            ViewMode::Presentation => vec![
                UserAgentStylesheet::default_sheet(),
                self.tokens_sheet.clone(),
                self.presentation_sheet.clone(),
            ],
            _ => vec![
                UserAgentStylesheet::default_sheet(),
                self.tokens_sheet.clone(),
                self.document_sheet.clone(),
            ],
        };
        let config = DocumentConfig {
            font_ctx: Some(self.font_ctx.clone()),
            ua_stylesheets,
            ..Default::default()
        };
        let new_doc = HtmlDocument::from_html(&html, config);

        if let Some(view) = self.inner.windows.get_mut(&window_id) {
            let saved_scroll = view.doc.inner().viewport_scroll();
            view.replace_document(Box::new(new_doc), true);
            view.doc.inner_mut().set_viewport_scroll(saved_scroll);
        }
        self.last_synced_view = self.active_view;
    }

    fn hit_block_and_offset(&self, window_id: WindowId, x: f32, y: f32) -> Option<(NodeId, usize)> {
        let view = self.inner.windows.get(&window_id)?;
        let doc = view.doc.inner();
        let scroll = doc.viewport_scroll();
        let hit = doc.hit(x + scroll.x as f32, y + scroll.y as f32)?;

        // Walk up to find the block with id="odv-<n>"
        let mut odv_id = None;
        let mut cur_id = Some(hit.node_id);
        while let Some(nid) = cur_id {
            let node = doc.get_node(nid)?;
            if let Some(val) = node.attr(blitz_dom::local_name!("id"))
                && let Some(num) = val.strip_prefix("odv-")
                && let Ok(parsed) = num.parse::<u64>()
            {
                odv_id = Some(NodeId(parsed));
                break;
            }
            cur_id = node.parent;
        }
        let block_id = odv_id?;

        // Compute exact character offset via Parley's inline layout when hitting text
        let mut offset = usize::MAX;
        if hit.is_text
            && let Some(hit_node) = doc.get_node(hit.node_id)
            && let Some(inline_root) = hit_node.inline_root_ancestor()
            && let Some(el) = inline_root.element_data()
            && let Some(ild) = el.inline_layout_data.as_ref()
        {
            let scale = ild.layout.scale();
            let parley_cursor =
                parley::layout::Cursor::from_point(&ild.layout, hit.x * scale, hit.y * scale);
            let raw_idx = parley_cursor.index();
            // Adjust for the synthetic "|" caret marker rendered inside the active block
            offset = if let Some(active_cur) = self.editor.cursor
                && active_cur.block_id == block_id
                && raw_idx > active_cur.offset
            {
                raw_idx.saturating_sub(1)
            } else {
                raw_idx
            };
        }
        Some((block_id, offset))
    }
}

impl ApplicationHandler for OdvApp {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.can_create_surfaces(event_loop);
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.destroy_surfaces(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = *mods;
            }
            WindowEvent::PointerMoved { position, .. } => {
                let scale = self
                    .inner
                    .windows
                    .get(&window_id)
                    .map(|v| v.window.scale_factor() as f32)
                    .unwrap_or(1.0);
                let new_pos = (position.x as f32 / scale, position.y as f32 / scale);
                self.cursor_pos = new_pos;
                if self.is_panning {
                    let dx = (self.pan_last_pos.0 - new_pos.0) as f64;
                    let dy = (self.pan_last_pos.1 - new_pos.1) as f64;
                    self.pan_last_pos = new_pos;
                    if let Some(view) = self.inner.windows.get_mut(&window_id) {
                        let mut doc = view.doc.inner_mut();
                        let mut cur = doc.viewport_scroll();
                        cur.x = (cur.x + dx).max(0.0);
                        cur.y = (cur.y + dy).max(0.0);
                        doc.set_viewport_scroll(cur);
                        drop(doc);
                        view.request_redraw();
                    }
                    return;
                }
                if self.is_dragging
                    && let Some((odv_id, offset)) =
                        self.hit_block_and_offset(window_id, self.cursor_pos.0, self.cursor_pos.1)
                {
                    self.editor.set_cursor_at(odv_id, offset, true);
                    self.sync_dom(window_id);
                }
            }
            WindowEvent::PointerButton {
                state: ElementState::Released,
                button: winit::event::ButtonSource::Mouse(MouseButton::Left),
                ..
            } => {
                self.is_dragging = false;
                self.is_panning = false;
            }
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                button: winit::event::ButtonSource::Mouse(MouseButton::Left),
                ..
            } => {
                if let Some((odv_id, offset)) =
                    self.hit_block_and_offset(window_id, self.cursor_pos.0, self.cursor_pos.1)
                {
                    self.is_panning = false;
                    let now = std::time::Instant::now();
                    let is_double_click = self.last_click.is_some_and(|(prev_t, prev_pos)| {
                        now.duration_since(prev_t).as_millis() < 400
                            && (prev_pos.0 - self.cursor_pos.0).abs() < 6.0
                            && (prev_pos.1 - self.cursor_pos.1).abs() < 6.0
                    });
                    if is_double_click {
                        self.editor.select_word_at(odv_id, offset);
                        self.last_click = None;
                        self.is_dragging = false;
                    } else {
                        let shift = self.modifiers.state().shift_key();
                        self.editor.set_cursor_at(odv_id, offset, shift);
                        self.last_click = Some((now, self.cursor_pos));
                        self.is_dragging = true;
                    }
                    self.sync_dom(window_id);
                } else if self.active_view == ViewMode::MindMap {
                    self.is_panning = true;
                    self.pan_last_pos = self.cursor_pos;
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if key_event.state == ElementState::Pressed => {
                let ctrl = self.modifiers.state().control_key();
                let shift = self.modifiers.state().shift_key();
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        event_loop.exit();
                        return;
                    }
                    Key::Named(NamedKey::F2) => {
                        self.active_view = match self.active_view {
                            ViewMode::Document => ViewMode::MindMap,
                            ViewMode::MindMap => ViewMode::Presentation,
                            _ => ViewMode::Document,
                        };
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::F3) => {
                        if self.editor.trigger_autotext(&self.autotexts) {
                            self.sync_dom(window_id);
                        }
                        return;
                    }
                    Key::Named(NamedKey::F4) => {
                        if self.active_view == ViewMode::MindMap
                            && self.editor.toggle_heading_collapsed()
                        {
                            self.sync_dom(window_id);
                        }
                        return;
                    }
                    Key::Named(NamedKey::F5) => {
                        if self.active_view == ViewMode::Presentation {
                            self.screen_ratio = match self.screen_ratio {
                                ScreenRatio::Desktop16x9 => ScreenRatio::MobilePortrait,
                                ScreenRatio::MobilePortrait => ScreenRatio::Desktop16x9,
                            };
                            self.sync_dom(window_id);
                        } else if self.active_view == ViewMode::Document {
                            self.paper_format = match self.paper_format {
                                PaperFormat::A4 => PaperFormat::Letter,
                                PaperFormat::Letter => PaperFormat::A4,
                            };
                            println!("Paper format: {:?}", self.paper_format);
                            self.sync_dom(window_id);
                        }
                        return;
                    }
                    Key::Named(NamedKey::F6) => {
                        if self.active_view == ViewMode::Presentation {
                            self.show_presenter_notes = !self.show_presenter_notes;
                            self.sync_dom(window_id);
                        }
                        return;
                    }
                    Key::Named(NamedKey::PageDown) => {
                        if self.active_view == ViewMode::Presentation {
                            let total = build_slides(self.editor.document()).len();
                            if self.active_slide + 1 < total {
                                self.active_slide += 1;
                                self.sync_dom(window_id);
                            }
                        }
                        return;
                    }
                    Key::Named(NamedKey::PageUp) => {
                        if self.active_view == ViewMode::Presentation && self.active_slide > 0 {
                            self.active_slide -= 1;
                            self.sync_dom(window_id);
                        }
                        return;
                    }
                    Key::Named(NamedKey::Tab) => {
                        if self.active_view == ViewMode::MindMap {
                            self.editor.add_mindmap_child("Sous-branche");
                            self.sync_dom(window_id);
                            return;
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if ctrl {
                            self.editor.move_word_left(shift);
                        } else {
                            self.editor.move_left(shift);
                        }
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if ctrl {
                            self.editor.move_word_right(shift);
                        } else {
                            self.editor.move_right(shift);
                        }
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Home) => {
                        self.editor.move_home(shift);
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::End) => {
                        self.editor.move_end(shift);
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Delete) => {
                        self.editor.delete_forward();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        if ctrl && self.active_view == ViewMode::MindMap {
                            self.editor.move_active_branch(true);
                        } else {
                            self.editor.move_up(self.active_view);
                        }
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if ctrl && self.active_view == ViewMode::MindMap {
                            self.editor.move_active_branch(false);
                        } else {
                            self.editor.move_down(self.active_view);
                        }
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.editor.backspace();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if ctrl {
                            let target = if self.active_view == ViewMode::Presentation {
                                BreakTarget::Screen
                            } else {
                                BreakTarget::Print
                            };
                            self.editor.insert_page_break(target);
                        } else if self.active_view == ViewMode::MindMap {
                            self.editor.add_mindmap_sibling("Nouvelle branche");
                        } else {
                            self.editor.split_block();
                        }
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Character(ch) if ctrl => {
                        match ch.as_str() {
                            "a" | "A" => {
                                if shift {
                                    self.editor.select_all_in_block();
                                } else {
                                    self.editor.select_all_doc();
                                }
                                self.sync_dom(window_id);
                                return;
                            }
                            "b" | "B" => {
                                self.editor.toggle_strong();
                                self.sync_dom(window_id);
                                return;
                            }
                            "e" | "E" => {
                                let html = export_standalone_html(
                                    self.editor.document(),
                                    self.screen_ratio,
                                );
                                let out_path = std::path::Path::new("export_presentation.html");
                                match std::fs::write(out_path, html) {
                                    Ok(()) => println!("Exported standalone presentation to {}", out_path.display()),
                                    Err(err) => eprintln!("Export error: {err}"),
                                }
                                return;
                            }
                            "p" | "P" => {
                                if self.active_view == ViewMode::Presentation {
                                    let pdf_bytes = export_presentation_pdf(
                                        self.editor.document(),
                                        self.screen_ratio,
                                    );
                                    let filename = match self.screen_ratio {
                                        ScreenRatio::Desktop16x9 => "export_screen_16x9.pdf",
                                        ScreenRatio::MobilePortrait => "export_screen_mobile.pdf",
                                    };
                                    match std::fs::write(filename, pdf_bytes) {
                                        Ok(()) => println!("Exported screen PDF to {filename}"),
                                        Err(err) => eprintln!("PDF export error: {err}"),
                                    }
                                } else {
                                    let pdf_bytes = export_document_pdf(
                                        self.editor.document(),
                                        self.paper_format,
                                    );
                                    let filename = match self.paper_format {
                                        PaperFormat::A4 => "export_document_a4.pdf",
                                        PaperFormat::Letter => "export_document_letter.pdf",
                                    };
                                    match std::fs::write(filename, pdf_bytes) {
                                        Ok(()) => println!("Exported document PDF to {filename}"),
                                        Err(err) => eprintln!("PDF export error: {err}"),
                                    }
                                }
                                return;
                            }
                            "i" | "I" => {
                                self.editor.toggle_emphasis();
                                self.sync_dom(window_id);
                                return;
                            }
                            "0" => {
                                self.editor.set_active_block_heading(None);
                                self.sync_dom(window_id);
                                return;
                            }
                            "1" | "2" | "3" | "4" | "5" => {
                                if let Ok(lvl) = ch.parse::<u8>() {
                                    self.editor.set_active_block_heading(Some(lvl));
                                    self.sync_dom(window_id);
                                }
                                return;
                            }
                            "z" | "Z" => {
                                self.editor.undo();
                                self.sync_dom(window_id);
                                return;
                            }
                            "y" | "Y" => {
                                self.editor.redo();
                                self.sync_dom(window_id);
                                return;
                            }
                            "c" | "C" => {
                                if let Some(text) = self.editor.copy_text() {
                                    let _ = self.clipboard.set_text(&text);
                                }
                                return;
                            }
                            "v" | "V" => {
                                if let Ok(text) = self.clipboard.get_text() {
                                    self.editor.insert_text(&text);
                                    self.sync_dom(window_id);
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        if !ctrl && let Some(txt) = &key_event.text {
                            let s = txt.as_str();
                            if !s.is_empty() && !s.chars().all(|c| c.is_control()) {
                                self.editor.insert_text_with_typo(s);
                                self.sync_dom(window_id);
                                return;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        self.inner.window_event(event_loop, window_id, event);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.proxy_wake_up(event_loop);
    }
}

fn sample_document() -> DocumentAst {
    let mut doc = DocumentAst::new("Open Document View", "fr");
    doc.push_heading(
        1,
        "1. Vision & Source Unique",
        Some("Idée clé : un seul document, 4 vues".into()),
    );
    doc.push_paragraph(
        "Cliquez sur n'importe quel bloc pour le modifier. Ctrl+B passe le bloc en gras sémantique.",
        Visibility::All,
    );
    doc.push_heading(
        2,
        "1.1. Prototype d'édition AST dans Blitz",
        Some("F2 alterne Document / Carte Mentale / Présentation".into()),
    );
    let q_id = doc.alloc_id();
    doc.blocks.push(BlockNode::Blockquote {
        id: q_id,
        children: vec![InlineNode::Text {
            text: "Testez les accents (é, è, à, ç, ê), Ctrl+Z/Y et Ctrl+C/V.".into(),
        }],
        visibility: Visibility::All,
    });
    doc
}

fn main() {
    if std::env::var_os("LANG").is_none() {
        set_locale(Locale::Fr);
    }

    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());

    let font_ctx = parley::FontContext::new();
    let tokens_sheet = UserAgentStylesheet::parse(TOKENS_CSS);
    let document_sheet = UserAgentStylesheet::parse(DOCUMENT_CSS);
    let presentation_sheet = UserAgentStylesheet::parse(PRESENTATION_CSS);
    let mindmap_sheet = UserAgentStylesheet::parse(MINDMAP_CSS);

    let (initial_doc, initial_view) = if let Some(path_arg) = std::env::args().nth(1) {
        let path = std::path::Path::new(&path_arg);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let bytes = std::fs::read(path).expect("Failed to read input file");
        match ext.as_str() {
            "xmind" => (
                import_xmind_bytes(&bytes, "fr").expect("Invalid .xmind file"),
                ViewMode::MindMap,
            ),
            "vym" => (
                import_vym_bytes(&bytes, "fr").expect("Invalid .vym file"),
                ViewMode::MindMap,
            ),
            "mm" => (
                import_freemind_mm(&String::from_utf8_lossy(&bytes), "fr")
                    .expect("Invalid .mm file"),
                ViewMode::MindMap,
            ),
            "odv" => (
                OdvPackage::read_from_bytes(&bytes)
                    .expect("Invalid .odv file")
                    .document,
                ViewMode::Document,
            ),
            "odt" => (
                import_odt_bytes(&bytes, "fr").expect("Invalid .odt file"),
                ViewMode::Document,
            ),
            _ => (sample_document(), ViewMode::Document),
        }
    } else {
        (sample_document(), ViewMode::Document)
    };

    let editor = EditorState::new(initial_doc);
    let view_label = match initial_view {
        ViewMode::MindMap => "Carte Mentale (h1-h5)".to_string(),
        _ => tr("Document View"),
    };
    let inner_html = if initial_view == ViewMode::Document {
        editor.render_document_inner_html(
            PaperFormat::A4,
            &view_label,
            "F2: vue | F5: A4/Letter | Ctrl+Entrée: saut de page | Ctrl+P: PDF | Ctrl+B/I | Ctrl+Z/Y | Ctrl+C/V",
        )
    } else {
        editor.render_main_inner_html(
            initial_view,
            &view_label,
            "F2: vue | Tab/Entrée/F4: carte | Shift+←/→: sélect. | Ctrl+B/I | Ctrl+Z/Y | Ctrl+C/V",
        )
    };
    let html = format!(
        "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\"></head><body><main id=\"odv-main\">{inner_html}</main></body></html>"
    );

    let ua_stylesheets = if initial_view == ViewMode::MindMap {
        vec![
            UserAgentStylesheet::default_sheet(),
            tokens_sheet.clone(),
            mindmap_sheet.clone(),
        ]
    } else {
        vec![
            UserAgentStylesheet::default_sheet(),
            tokens_sheet.clone(),
            document_sheet.clone(),
        ]
    };

    let doc = HtmlDocument::from_html(
        &html,
        DocumentConfig {
            font_ctx: Some(font_ctx.clone()),
            ua_stylesheets,
            ..Default::default()
        },
    );

    let renderer = VelloCpuWindowRenderer::new();
    let attrs = WindowAttributes::default()
        .with_title(tr("Open Document View"))
        .with_surface_size(LogicalSize::new(960.0, 720.0));

    let mut inner = BlitzApplication::new(proxy, receiver);
    inner.add_window(WindowConfig::with_attributes(
        Box::new(doc),
        renderer,
        attrs,
    ));

    let app = OdvApp {
        inner,
        active_view: initial_view,
        last_synced_view: initial_view,
        active_slide: 0,
        screen_ratio: ScreenRatio::Desktop16x9,
        paper_format: PaperFormat::A4,
        show_presenter_notes: true,
        editor,
        modifiers: Modifiers::default(),
        cursor_pos: (0.0, 0.0),
        is_dragging: false,
        is_panning: false,
        pan_last_pos: (0.0, 0.0),
        last_click: None,
        font_ctx,
        autotexts: AutoTextCatalog::default_catalog(),
        tokens_sheet,
        document_sheet,
        presentation_sheet,
        mindmap_sheet,
        clipboard: PersistentClipboard::new(),
    };

    if let Err(err) = event_loop.run_app(app) {
        eprintln!("Event loop error: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnose_mindmap_svg_layout() {
        let mut ast = DocumentAst::new("Test", "fr");
        ast.push_heading(1, "Root", None);
        ast.push_heading(2, "Child", None);
        let editor = EditorState::new(ast);
        let inner = editor.render_main_inner_html(ViewMode::MindMap, "MM", "");
        let html = format!("<!DOCTYPE html><html><body><main>{inner}</main></body></html>");
        let mindmap_sheet = UserAgentStylesheet::parse(MINDMAP_CSS);
        let mut doc = HtmlDocument::from_html(
            &html,
            DocumentConfig {
                ua_stylesheets: vec![UserAgentStylesheet::default_sheet(), mindmap_sheet],
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        let svg_id = doc.query_selector("svg").unwrap().expect("svg node must exist");
        let node = doc.get_node(svg_id).unwrap();
        let svg_data = node.element_data().unwrap().svg_data();
        let has_svg_data = svg_data.is_some();
        let children_count = svg_data.map(|s| s.root().children().len()).unwrap_or(0);
        let size = node.final_layout().size;
        let loc = node.final_layout().location;
        println!("svg has_svg_data={has_svg_data}, tree_children={children_count}, size={size:?}, loc={loc:?}");
        assert!(has_svg_data, "svg_data must be parsed by usvg");
        assert!(children_count > 0, "usvg tree must have rendered paths/polygons, got 0");
        assert!(size.width > 100.0 && size.height > 100.0, "svg layout size must be > 100: {size:?}");
    }
}
