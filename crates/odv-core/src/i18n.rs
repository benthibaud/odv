use gettext::Catalog;
use std::sync::{OnceLock, RwLock};

static FR_MO: &[u8] = include_bytes!("../../../locales/fr.mo");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Fr,
}

struct I18nState {
    locale: Locale,
    fr_catalog: Catalog,
}

static STATE: OnceLock<RwLock<I18nState>> = OnceLock::new();

fn state() -> &'static RwLock<I18nState> {
    STATE.get_or_init(|| {
        let fr_catalog =
            Catalog::parse(FR_MO).expect("embedded French .mo catalog must be valid");
        let default_locale = detect_system_locale();
        RwLock::new(I18nState {
            locale: default_locale,
            fr_catalog,
        })
    })
}

fn detect_system_locale() -> Locale {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var)
            && val.to_lowercase().starts_with("fr")
        {
            return Locale::Fr;
        }
    }
    Locale::En
}

/// Set the active UI/core translation locale.
pub fn set_locale(locale: Locale) {
    if let Ok(mut guard) = state().write() {
        guard.locale = locale;
    }
}

/// Return the currently active locale.
pub fn current_locale() -> Locale {
    state()
        .read()
        .map(|g| g.locale)
        .unwrap_or(Locale::En)
}

/// Translate an English source string into the active locale.
pub fn tr(msgid: &str) -> String {
    let Ok(guard) = state().read() else {
        return msgid.to_string();
    };
    match guard.locale {
        Locale::En => msgid.to_string(),
        Locale::Fr => guard.fr_catalog.gettext(msgid).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_english_pivot_to_french() {
        set_locale(Locale::En);
        assert_eq!(tr("Open Document View"), "Open Document View");

        set_locale(Locale::Fr);
        assert_eq!(tr("Switch view (F2)"), "Changer de vue (F2)");
    }
}
