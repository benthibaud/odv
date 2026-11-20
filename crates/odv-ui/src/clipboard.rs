use arboard::Clipboard;

/// Persistent clipboard handle kept alive for the whole application lifetime
/// so X11 clipboard ownership is preserved after copy operations.
#[allow(dead_code)]
pub struct PersistentClipboard {
    inner: Option<Clipboard>,
}

impl PersistentClipboard {
    pub fn new() -> Self {
        let inner = Clipboard::new().ok();
        Self { inner }
    }

    pub fn set_text(&mut self, text: &str) -> Result<(), String> {
        let Some(cb) = self.inner.as_mut() else {
            return Err("Clipboard unavailable".to_string());
        };
        cb.set_text(text.to_owned()).map_err(|e| e.to_string())
    }

    pub fn get_text(&mut self) -> Result<String, String> {
        let Some(cb) = self.inner.as_mut() else {
            return Err("Clipboard unavailable".to_string());
        };
        cb.get_text().map_err(|e| e.to_string())
    }
}
