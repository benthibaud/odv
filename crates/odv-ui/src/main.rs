mod clipboard;

use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocumentConfig, UserAgentStylesheet};
use blitz_html::HtmlDocument;
use blitz_shell::{
    BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use clipboard::PersistentClipboard;
use odv_core::ast::{BlockNode, DocumentAst, InlineNode, NodeId, ViewMode, Visibility};
use odv_core::editor::EditorState;
use odv_core::i18n::{Locale, set_locale, tr};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Modifiers, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{WindowAttributes, WindowId};

static TOKENS_CSS: &str = include_str!("../../../styles/tokens.css");
static DOCUMENT_CSS: &str = include_str!("../../../styles/document.css");
static PRESENTATION_CSS: &str = include_str!("../../../styles/presentation.css");

struct OdvApp {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    active_view: ViewMode,
    editor: EditorState,
    modifiers: Modifiers,
    cursor_pos: (f32, f32),
    tokens_sheet: UserAgentStylesheet,
    document_sheet: UserAgentStylesheet,
    presentation_sheet: UserAgentStylesheet,
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
        let inner_html = self.editor.render_main_inner_html(
            self.active_view,
            &label,
            "F2: vue | Ctrl+B: gras | Ctrl+Z/Y: annuler | Ctrl+C/V: copier-coller",
        );
        let html = format!(
            "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\"></head><body><main>{inner_html}</main></body></html>"
        );
        let mut new_doc = HtmlDocument::from_html(&html, DocumentConfig::default());
        new_doc.add_user_agent_stylesheet(self.tokens_sheet.clone());
        let sheet = match self.active_view {
            ViewMode::Presentation => &self.presentation_sheet,
            _ => &self.document_sheet,
        };
        new_doc.add_user_agent_stylesheet(sheet.clone());

        if let Some(view) = self.inner.windows.get_mut(&window_id) {
            view.replace_document(Box::new(new_doc), true);
        }
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
                self.cursor_pos = (position.x as f32 / scale, position.y as f32 / scale);
            }
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                button: winit::event::ButtonSource::Mouse(MouseButton::Left),
                ..
            } => {
                let clicked_odv_id = self.inner.windows.get(&window_id).and_then(|view| {
                    let doc = view.doc.inner();
                    let mut node_id = doc.element_from_point(self.cursor_pos.0, self.cursor_pos.1)?;
                    loop {
                        let node = doc.get_node(node_id)?;
                        if let Some(val) = node.attr(blitz_dom::local_name!("id"))
                            && let Some(num) = val.strip_prefix("odv-")
                            && let Ok(parsed) = num.parse::<u64>()
                        {
                            return Some(NodeId(parsed));
                        }
                        node_id = node.parent?;
                    }
                });
                if let Some(odv_id) = clicked_odv_id {
                    self.editor.focus_block(odv_id);
                    self.sync_dom(window_id);
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if key_event.state == ElementState::Pressed => {
                let ctrl = self.modifiers.state().control_key();
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
                    Key::Named(NamedKey::ArrowLeft) => {
                        self.editor.move_left();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        self.editor.move_right();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.editor.backspace();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.editor.split_block();
                        self.sync_dom(window_id);
                        return;
                    }
                    Key::Character(ch) if ctrl => {
                        match ch.as_str() {
                            "b" | "B" => {
                                self.editor.toggle_strong_block();
                                self.sync_dom(window_id);
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
                                if let Some(text) = self.editor.active_block_plain_text() {
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
                                self.editor.insert_text(s);
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

    let tokens_sheet = UserAgentStylesheet::parse(TOKENS_CSS);
    let document_sheet = UserAgentStylesheet::parse(DOCUMENT_CSS);
    let presentation_sheet = UserAgentStylesheet::parse(PRESENTATION_CSS);

    let editor = EditorState::new(sample_document());
    let inner_html = editor.render_main_inner_html(
        ViewMode::Document,
        &tr("Document View"),
        "F2: vue | Ctrl+B: gras | Ctrl+Z/Y: annuler | Ctrl+C/V: copier-coller",
    );
    let html = format!(
        "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\"></head><body><main>{inner_html}</main></body></html>"
    );

    let mut doc = HtmlDocument::from_html(&html, DocumentConfig::default());
    doc.add_user_agent_stylesheet(tokens_sheet.clone());
    doc.add_user_agent_stylesheet(document_sheet.clone());

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
        active_view: ViewMode::Document,
        editor,
        modifiers: Modifiers::default(),
        cursor_pos: (0.0, 0.0),
        tokens_sheet,
        document_sheet,
        presentation_sheet,
        clipboard: PersistentClipboard::new(),
    };

    if let Err(err) = event_loop.run_app(app) {
        eprintln!("Event loop error: {err}");
    }
}
