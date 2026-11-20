use crate::ast::{BlockNode, InlineNode, Visibility};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An AutoText entry stored in `autotext.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTextEntry {
    pub shortcut: String,
    pub label: String,
    pub blocks: Vec<AutoTextBlockTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoTextBlockTemplate {
    Paragraph {
        text: String,
        #[serde(default)]
        visibility: Visibility,
    },
    Aside {
        kind: String,
        title: String,
        text: String,
        #[serde(default)]
        visibility: Visibility,
    },
}

/// Shareable AutoText catalog (`autotext.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTextCatalog {
    pub entries: BTreeMap<String, AutoTextEntry>,
}

impl AutoTextCatalog {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Standard AutoText templates defined in the specification.
    pub fn default_catalog() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(
            "fenetre".into(),
            AutoTextEntry {
                shortcut: "fenetre".into(),
                label: "Envelope window block (DL)".into(),
                blocks: vec![AutoTextBlockTemplate::Aside {
                    kind: "destinataire-fenetre".into(),
                    title: "Destinataire".into(),
                    text: "Nom Prénom\n12 rue de la République\n75001 PARIS".into(),
                    visibility: Visibility::OnlyPrint,
                }],
            },
        );
        entries.insert(
            "focus".into(),
            AutoTextEntry {
                shortcut: "focus".into(),
                label: "Focus callout".into(),
                blocks: vec![AutoTextBlockTemplate::Aside {
                    kind: "encadre-focus".into(),
                    title: "À retenir".into(),
                    text: "Point clé à mettre en valeur.".into(),
                    visibility: Visibility::All,
                }],
            },
        );
        entries.insert(
            "edito".into(),
            AutoTextEntry {
                shortcut: "edito".into(),
                label: "Editorial block".into(),
                blocks: vec![AutoTextBlockTemplate::Aside {
                    kind: "rubrique-edito".into(),
                    title: "Éditorial".into(),
                    text: "Texte de l'éditorial...".into(),
                    visibility: Visibility::All,
                }],
            },
        );
        entries.insert(
            "sig".into(),
            AutoTextEntry {
                shortcut: "sig".into(),
                label: "Standard signature".into(),
                blocks: vec![AutoTextBlockTemplate::Paragraph {
                    text: "Veuillez agréer, Madame, Monsieur, l'expression de mes salutations distinguées.".into(),
                    visibility: Visibility::All,
                }],
            },
        );
        entries.insert(
            "dt".into(),
            AutoTextEntry {
                shortcut: "dt".into(),
                label: "Dummy semantic text".into(),
                blocks: vec![
                    AutoTextBlockTemplate::Paragraph {
                        text: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed non risus.".into(),
                        visibility: Visibility::All,
                    },
                    AutoTextBlockTemplate::Paragraph {
                        text: "Suspendisse lectus tortor, dignissim sit amet, adipiscing nec, ultricies sed, dolor.".into(),
                        visibility: Visibility::All,
                    },
                ],
            },
        );
        Self { entries }
    }

    pub fn materialize(&self, shortcut: &str, alloc_id: &mut impl FnMut() -> crate::ast::NodeId) -> Option<Vec<BlockNode>> {
        let key = shortcut.strip_prefix('/').unwrap_or(shortcut);
        let entry = self.entries.get(key)?;
        let blocks = entry
            .blocks
            .iter()
            .map(|tpl| match tpl {
                AutoTextBlockTemplate::Paragraph { text, visibility } => BlockNode::Paragraph {
                    id: alloc_id(),
                    children: vec![InlineNode::Text { text: text.clone() }],
                    visibility: *visibility,
                },
                AutoTextBlockTemplate::Aside {
                    kind,
                    title,
                    text,
                    visibility,
                } => BlockNode::Aside {
                    id: alloc_id(),
                    kind: kind.clone(),
                    title: title.clone(),
                    children: vec![InlineNode::Text { text: text.clone() }],
                    visibility: *visibility,
                },
            })
            .collect();
        Some(blocks)
    }
}

/// Regional typography profile derived from the document's BCP 47 language tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypoProfile {
    /// France (`fr` / `fr-FR`): non-breaking space before `: ; ! ?`, `«\u{00A0}...\u{00A0}»`, `œ` ligatures.
    FrenchFrance,
    /// Switzerland / Quebec (`fr-CH`, `fr-CA`): no space before `; ! ?` (space before `:` in fr-CA), `«...»`, `œ` ligatures.
    FrenchCompact,
    /// English (`en`): curly quotes `“...”`, em-dash, ellipsis, no space before double punctuation.
    English,
    /// Disabled / unknown language: no automatic typographic replacement.
    None,
}

impl TypoProfile {
    pub fn from_language_tag(tag: &str) -> Self {
        let lower = tag.trim().to_ascii_lowercase();
        if lower == "fr" || lower == "fr-fr" || lower.starts_with("fr-be") {
            Self::FrenchFrance
        } else if lower.starts_with("fr-ch") || lower.starts_with("fr-ca") {
            Self::FrenchCompact
        } else if lower == "en" || lower.starts_with("en-") {
            Self::English
        } else {
            Self::None
        }
    }
}

/// Apply live micro-typography according to `profile` to `before_caret` + `inserted`,
/// returning `(bytes_to_erase_before_caret, replacement_text)`.
pub fn apply_micro_typo(profile: TypoProfile, before_caret: &str, inserted: &str) -> (usize, String) {
    if profile == TypoProfile::None {
        return (0, inserted.to_string());
    }

    // Double hyphen -> em-dash
    if inserted == "-" && before_caret.ends_with('-') {
        return (1, "—".to_string());
    }

    // Three dots -> ellipsis
    if inserted == "." && before_caret.ends_with("..") {
        return (2, "…".to_string());
    }

    // Straight double quote
    if inserted == "\"" {
        let is_opening = before_caret.is_empty()
            || before_caret.ends_with([' ', '\u{00A0}', '\n', '\t', '(', '[']);
        return match profile {
            TypoProfile::FrenchFrance => {
                if is_opening {
                    (0, "«\u{00A0}".to_string())
                } else {
                    (0, "\u{00A0}»".to_string())
                }
            }
            TypoProfile::FrenchCompact => {
                if is_opening {
                    (0, "«".to_string())
                } else {
                    (0, "»".to_string())
                }
            }
            TypoProfile::English => {
                if is_opening {
                    (0, "“".to_string())
                } else {
                    (0, "”".to_string())
                }
            }
            TypoProfile::None => (0, "\"".to_string()),
        };
    }

    let is_french = matches!(profile, TypoProfile::FrenchFrance | TypoProfile::FrenchCompact);

    // Word-boundary oe -> œ ligatures for French profiles (safe list, avoids corrupting coexister/moelle/poème)
    if is_french && matches!(inserted, " " | "." | "," | ";" | ":" | "!" | "?") {
        let ligatures = [
            ("oeuf", "œuf"),
            ("Oeuf", "Œuf"),
            ("oeufs", "œufs"),
            ("Oeufs", "Œufs"),
            ("boeuf", "bœuf"),
            ("Boeuf", "Bœuf"),
            ("boeufs", "bœufs"),
            ("coeur", "cœur"),
            ("Coeur", "Cœur"),
            ("coeurs", "cœurs"),
            ("choeur", "chœur"),
            ("Choeur", "Chœur"),
            ("choeurs", "chœurs"),
            ("soeur", "sœur"),
            ("Soeur", "Sœur"),
            ("soeurs", "sœurs"),
            ("moeurs", "mœurs"),
            ("voeu", "vœu"),
            ("Voeu", "Vœu"),
            ("voeux", "vœux"),
            ("oeuvre", "œuvre"),
            ("Oeuvre", "Œuvre"),
            ("oeuvres", "œuvres"),
            ("manoeuvre", "manœuvre"),
            ("Manoeuvre", "Manœuvre"),
            ("manoeuvres", "manœuvres"),
            ("desoeuvre", "désœuvré"),
            ("noeud", "nœud"),
            ("Noeud", "Nœud"),
            ("noeuds", "nœuds"),
            ("oeil", "œil"),
            ("Oeil", "Œil"),
            ("foetus", "fœtus"),
            ("oesophage", "œsophage"),
        ];
        for (raw, lig) in ligatures {
            if let Some(prefix) = before_caret.strip_suffix(raw) {
                let is_word_start = prefix.is_empty()
                    || prefix
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_alphanumeric());
                if is_word_start {
                    let suffix = if profile == TypoProfile::FrenchFrance
                        && matches!(inserted, ":" | ";" | "!" | "?")
                    {
                        format!("\u{00A0}{inserted}")
                    } else {
                        inserted.to_string()
                    };
                    return (raw.len(), format!("{lig}{suffix}"));
                }
            }
        }
    }

    // Non-breaking space before double punctuation (France/Belgium only)
    if profile == TypoProfile::FrenchFrance && matches!(inserted, ":" | ";" | "!" | "?") {
        if before_caret.ends_with(' ') {
            return (1, format!("\u{00A0}{inserted}"));
        }
        if !before_caret.is_empty() && !before_caret.ends_with('\u{00A0}') {
            return (0, format!("\u{00A0}{inserted}"));
        }
    }

    (0, inserted.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_french_micro_typography_rules() {
        let fr = TypoProfile::from_language_tag("fr-FR");
        let ch = TypoProfile::from_language_tag("fr-CH");
        let en = TypoProfile::from_language_tag("en-US");

        assert_eq!(apply_micro_typo(fr, "bonjour ", ":"), (1, "\u{00A0}:".into()));
        assert_eq!(apply_micro_typo(fr, "bonjour", "!"), (0, "\u{00A0}!".into()));
        assert_eq!(apply_micro_typo(fr, "mot -", "-"), (1, "—".into()));
        assert_eq!(apply_micro_typo(fr, "suite..", "."), (2, "…".into()));
        assert_eq!(apply_micro_typo(fr, "", "\""), (0, "«\u{00A0}".into()));
        assert_eq!(apply_micro_typo(fr, "«\u{00A0}oui", "\""), (0, "\u{00A0}»".into()));
        assert_eq!(apply_micro_typo(fr, "un oeuf", " "), (4, "œuf ".into()));
        assert_eq!(apply_micro_typo(fr, "une manoeuvre", " "), (9, "manœuvre ".into()));
        assert_eq!(apply_micro_typo(fr, "Le Coeur", "!"), (5, "Cœur\u{00A0}!".into()));
        // Must NOT corrupt words like "coexister" or "moelle"
        assert_eq!(apply_micro_typo(fr, "coexister", " "), (0, " ".into()));

        // Swiss French: no non-breaking space before double punctuation, compact guillemets
        assert_eq!(apply_micro_typo(ch, "bonjour", "!"), (0, "!".into()));
        assert_eq!(apply_micro_typo(ch, "", "\""), (0, "«".into()));
        assert_eq!(apply_micro_typo(ch, "une manoeuvre", " "), (9, "manœuvre ".into()));

        // English: curly quotes, no space before punctuation
        assert_eq!(apply_micro_typo(en, "", "\""), (0, "“".into()));
        assert_eq!(apply_micro_typo(en, "hello", "!"), (0, "!".into()));
    }

    #[test]
    fn contract_autotext_json_roundtrip() {
        let cat = AutoTextCatalog::default_catalog();
        let json = cat.to_json().unwrap();
        let parsed = AutoTextCatalog::from_json(&json).unwrap();
        assert_eq!(cat, parsed);
        assert!(parsed.entries.contains_key("fenetre"));
        assert!(parsed.entries.contains_key("focus"));
    }
}
