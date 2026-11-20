mod clipboard;

use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocumentConfig, UserAgentStylesheet};
use blitz_html::HtmlDocument;
use blitz_shell::{
    BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use clipboard::PersistentClipboard;
use odv_core::ast::{BlockNode, DocumentAst, InlineNode, ViewMode, Visibility};
use odv_core::i18n::{Locale, set_locale, tr};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{WindowAttributes, WindowId};

static TOKENS_CSS: &str = include_str!("../../../styles/tokens.css");
static DOCUMENT_CSS: &str = include_str!("../../../styles/document.css");
static PRESENTATION_CSS: &str = include_str!("../../../styles/presentation.css");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveView {
    Document,
    Presentation,
}

struct OdvApp {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    active_view: ActiveView,
    tokens_sheet: UserAgentStylesheet,
    document_sheet: UserAgentStylesheet,
    presentation_sheet: UserAgentStylesheet,
    _clipboard: PersistentClipboard,
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
        if let WindowEvent::KeyboardInput {
            event: ref key_event,
            ..
        } = event
            && key_event.state == ElementState::Pressed
        {
            match key_event.logical_key {
                Key::Named(NamedKey::F2) => {
                    self.active_view = match self.active_view {
                        ActiveView::Document => ActiveView::Presentation,
                        ActiveView::Presentation => ActiveView::Document,
                    };
                    if let Some(view) = self.inner.windows.get_mut(&window_id) {
                        let mut doc = view.doc.inner_mut();
                        doc.remove_user_agent_stylesheet(&self.document_sheet);
                        doc.remove_user_agent_stylesheet(&self.presentation_sheet);
                        doc.add_user_agent_stylesheet(self.tokens_sheet.clone());
                        let next_sheet = match self.active_view {
                            ActiveView::Document => &self.document_sheet,
                            ActiveView::Presentation => &self.presentation_sheet,
                        };
                        doc.add_user_agent_stylesheet(next_sheet.clone());
                        drop(doc);
                        view.request_redraw();
                    }
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    event_loop.exit();
                    return;
                }
                _ => {}
            }
        }

        self.inner.window_event(event_loop, window_id, event);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.proxy_wake_up(event_loop);
    }
}

fn sample_document() -> DocumentAst {
    let mut doc = DocumentAst::new(tr("Open Document View"), "fr");
    doc.push_heading(
        1,
        tr("Open Document View"),
        Some(tr("Switch view (F2)")),
    );
    let p_id = doc.alloc_id();
    doc.blocks.push(BlockNode::Paragraph {
        id: p_id,
        children: vec![
            InlineNode::Text {
                text: format!("{} — ", tr("Document View")),
            },
            InlineNode::Strong {
                children: vec![InlineNode::Text {
                    text: tr("Switch view (F2)"),
                }],
            },
        ],
        visibility: Visibility::All,
    });
    let q_id = doc.alloc_id();
    doc.blocks.push(BlockNode::Blockquote {
        id: q_id,
        children: vec![InlineNode::Text {
            text: tr("Press F2 to switch view, Esc to quit."),
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

    let ast = sample_document();
    let html = ast.to_semantic_html(
        ViewMode::Document,
        &tr("Open Document View"),
        &tr("Switch view (F2)"),
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
        active_view: ActiveView::Document,
        tokens_sheet,
        document_sheet,
        presentation_sheet,
        _clipboard: PersistentClipboard::new(),
    };

    if let Err(err) = event_loop.run_app(app) {
        eprintln!("Event loop error: {err}");
    }
}
