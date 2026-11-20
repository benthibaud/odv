use spellbook::Dictionary;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpellError {
    #[error("No Hunspell dictionary found for language '{0}'")]
    DictionaryNotFound(String),
    #[error("Failed to read dictionary file '{path}': {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse Hunspell dictionary: {0}")]
    Parse(String),
}

/// Pure-Rust Hunspell/Nuspell-compatible spellchecker backed by `spellbook`.
pub struct SpellChecker {
    dict: Dictionary,
}

impl SpellChecker {
    /// Build a spellchecker from raw `.aff` and `.dic` contents.
    pub fn from_aff_dic(aff: &str, dic: &str) -> Result<Self, SpellError> {
        let dict = Dictionary::new(aff, dic).map_err(|e| SpellError::Parse(format!("{e:?}")))?;
        Ok(Self { dict })
    }

    /// Locate and load a system Hunspell dictionary for a BCP 47 tag (e.g. `fr-FR`, `en-US`).
    pub fn load_system(lang_tag: &str) -> Result<Self, SpellError> {
        let search_dirs = [
            Path::new("/usr/share/hunspell"),
            Path::new("/usr/local/share/hunspell"),
        ];
        let candidates = candidate_names(lang_tag);
        for dir in search_dirs {
            if !dir.is_dir() {
                continue;
            }
            for name in &candidates {
                let aff_path = dir.join(format!("{name}.aff"));
                let dic_path = dir.join(format!("{name}.dic"));
                if aff_path.is_file() && dic_path.is_file() {
                    let aff = std::fs::read_to_string(&aff_path).map_err(|source| SpellError::Io {
                        path: aff_path,
                        source,
                    })?;
                    let dic = std::fs::read_to_string(&dic_path).map_err(|source| SpellError::Io {
                        path: dic_path,
                        source,
                    })?;
                    return Self::from_aff_dic(&aff, &dic);
                }
            }
        }
        Err(SpellError::DictionaryNotFound(lang_tag.to_string()))
    }

    pub fn check_word(&self, word: &str) -> bool {
        self.dict.check(word)
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(word, &mut out);
        out
    }

    /// Return `(byte_offset, word)` for each misspelled word in `text`.
    pub fn check_indices<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut errors = Vec::new();
        let mut word_start: Option<usize> = None;

        for (idx, ch) in text.char_indices() {
            let is_word_char = ch.is_alphabetic() || ch == '\'' || ch == '’' || ch == '-';
            match (word_start, is_word_char) {
                (None, true) => word_start = Some(idx),
                (Some(start), false) => {
                    let slice = trim_word_edges(&text[start..idx]);
                    if !slice.is_empty() && !self.dict.check(slice) {
                        errors.push((start, slice));
                    }
                    word_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = word_start {
            let slice = trim_word_edges(&text[start..]);
            if !slice.is_empty() && !self.dict.check(slice) {
                errors.push((start, slice));
            }
        }
        errors
    }
}

fn trim_word_edges(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\'' || c == '’' || c == '-')
}

fn candidate_names(lang_tag: &str) -> Vec<String> {
    let normalized = lang_tag.trim().replace('-', "_");
    let mut list = Vec::new();
    if !normalized.is_empty() {
        list.push(normalized.clone());
        if let Some((lang, _)) = normalized.split_once('_') {
            list.push(lang.to_string());
            if lang == "en" {
                list.push("en_US".to_string());
                list.push("en_GB".to_string());
            } else if lang == "fr" {
                list.push("fr_FR".to_string());
            }
        } else if normalized == "en" {
            list.push("en_US".to_string());
            list.push("en_GB".to_string());
        } else if normalized == "fr" {
            list.push("fr_FR".to_string());
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_spellbook_embedded_mini_dictionary() {
        let aff = "SET UTF-8\n";
        let dic = "3\nbonjour\nmonde\nsémantique\n";
        let checker = SpellChecker::from_aff_dic(aff, dic).unwrap();

        assert!(checker.check_word("bonjour"));
        assert!(checker.check_word("sémantique"));
        assert!(!checker.check_word("erreurxyz"));

        let errors = checker.check_indices("bonjour erreurxyz monde");
        assert_eq!(errors, vec![(8, "erreurxyz")]);
    }

    #[test]
    fn contract_spellbook_system_french_and_english_hunspell() {
        if Path::new("/usr/share/hunspell/fr_FR.dic").is_file() {
            let fr = SpellChecker::load_system("fr-FR").expect("French Hunspell must load");
            assert!(fr.check_word("bonjour"));
            assert!(fr.check_word("manœuvre"));
            assert!(!fr.check_word("ortografe"));
        }
        if Path::new("/usr/share/hunspell/en_US.dic").is_file() {
            let en = SpellChecker::load_system("en-US").expect("English Hunspell must load");
            assert!(en.check_word("keyboard"));
            assert!(!en.check_word("mispeled"));
        }
    }
}
