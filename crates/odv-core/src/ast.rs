use serde::{Deserialize, Serialize};

/// Target visibility scope for multi-support publishing and GDPR physical purge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    All,
    OnlyPrint,
    OnlyWeb,
    OnlyEpub,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::All
    }
}

/// Semantic inline span inside a block (no arbitrary formatting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineNode {
    Text { text: String },
    Strong { children: Vec<InlineNode> },
    Emphasis { children: Vec<InlineNode> },
    Code { text: String },
}

/// Semantic block node allowed by the guard rails (no arbitrary div/span).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    Heading {
        level: u8,
        children: Vec<InlineNode>,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        visibility: Visibility,
    },
    Paragraph {
        children: Vec<InlineNode>,
        #[serde(default)]
        visibility: Visibility,
    },
    Blockquote {
        children: Vec<InlineNode>,
        #[serde(default)]
        visibility: Visibility,
    },
    ThematicBreak,
}

/// Root semantic document AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAst {
    pub title: String,
    pub language: String,
    pub blocks: Vec<BlockNode>,
}

impl DocumentAst {
    /// Render a minimal semantic HTML representation for view projection.
    pub fn to_semantic_html(&self, title_label: &str, hint_label: &str) -> String {
        let mut body = String::new();
        for block in &self.blocks {
            match block {
                BlockNode::Heading { level, children, .. } => {
                    let lvl = (*level).clamp(1, 5);
                    body.push_str(&format!("<h{lvl}>{}</h{lvl}>\n", render_inlines(children)));
                }
                BlockNode::Paragraph { children, .. } => {
                    body.push_str(&format!("<p>{}</p>\n", render_inlines(children)));
                }
                BlockNode::Blockquote { children, .. } => {
                    body.push_str(&format!(
                        "<blockquote>{}</blockquote>\n",
                        render_inlines(children)
                    ));
                }
                BlockNode::ThematicBreak => {
                    body.push_str("<hr />\n");
                }
            }
        }

        format!(
            r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<title>{title}</title>
</head>
<body>
<main>
<header class="odv-toolbar">{title_label} — {hint_label}</header>
{body}
</main>
</body>
</html>"#,
            lang = escape_html(&self.language),
            title = escape_html(&self.title),
            title_label = escape_html(title_label),
            hint_label = escape_html(hint_label),
        )
    }
}

fn render_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text { text } => out.push_str(&escape_html(text)),
            InlineNode::Strong { children } => {
                out.push_str(&format!("<strong>{}</strong>", render_inlines(children)));
            }
            InlineNode::Emphasis { children } => {
                out.push_str(&format!("<em>{}</em>", render_inlines(children)));
            }
            InlineNode::Code { text } => {
                out.push_str(&format!("<code>{}</code>", escape_html(text)));
            }
        }
    }
    out
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_semantic_html_without_arbitrary_divs() {
        let doc = DocumentAst {
            title: "Sample".into(),
            language: "en".into(),
            blocks: vec![
                BlockNode::Heading {
                    level: 1,
                    children: vec![InlineNode::Text {
                        text: "Chapter 1".into(),
                    }],
                    note: None,
                    visibility: Visibility::All,
                },
                BlockNode::Paragraph {
                    children: vec![InlineNode::Strong {
                        children: vec![InlineNode::Text {
                            text: "Semantic".into(),
                        }],
                    }],
                    visibility: Visibility::All,
                },
            ],
        };

        let html = doc.to_semantic_html("ODV", "F2");
        assert!(html.contains("<h1>Chapter 1</h1>"));
        assert!(html.contains("<p><strong>Semantic</strong></p>"));
        assert!(!html.contains("<div"));
        assert!(!html.contains("<span"));
    }
}
