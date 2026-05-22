#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Atlassian Document Format node, used for Jira descriptions and comments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdfNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<AdfNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

impl AdfNode {
    /// Create a plain text node.
    fn text_node(text: impl Into<String>) -> Self {
        AdfNode {
            node_type: "text".to_string(),
            text: Some(text.into()),
            content: None,
            attrs: None,
            version: None,
        }
    }

    /// Create a paragraph node containing children.
    fn paragraph(children: Vec<AdfNode>) -> Self {
        AdfNode {
            node_type: "paragraph".to_string(),
            text: None,
            content: Some(children),
            attrs: None,
            version: None,
        }
    }
}

/// Convert a plain text string into an ADF document node.
///
/// Each non-empty line becomes a paragraph with a single text node.
pub fn to_adf(text: &str) -> AdfNode {
    let paragraphs: Vec<AdfNode> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| AdfNode::paragraph(vec![AdfNode::text_node(line)]))
        .collect();

    AdfNode {
        node_type: "doc".to_string(),
        text: None,
        content: Some(paragraphs),
        attrs: None,
        version: Some(1),
    }
}

/// Recursively convert an ADF node tree to plain text.
pub fn adf_to_text(node: Option<&AdfNode>) -> String {
    let node = match node {
        Some(n) => n,
        None => return String::new(),
    };

    match node.node_type.as_str() {
        "text" => node.text.clone().unwrap_or_default(),

        "hardBreak" => "\n".to_string(),

        "mention" => {
            // attrs.text or attrs.id
            if let Some(Value::Object(ref attrs)) = node.attrs {
                if let Some(Value::String(name)) = attrs.get("text") {
                    return format!("@{}", name.trim_start_matches('@'));
                }
                if let Some(Value::String(id)) = attrs.get("id") {
                    return format!("@{}", id);
                }
            }
            "@mention".to_string()
        }

        "paragraph" | "heading" | "blockquote" | "listItem" => {
            let children = children_text(node);
            if children.is_empty() {
                "\n".to_string()
            } else {
                format!("{}\n", children)
            }
        }

        "bulletList" | "orderedList" | "codeBlock" => children_text(node),

        "doc" => {
            let raw = children_text(node);
            raw.trim().to_string()
        }

        _ => children_text(node),
    }
}

/// Concatenate plain-text representations of all children.
fn children_text(node: &AdfNode) -> String {
    node.content
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|child| adf_to_text(Some(child)))
        .collect()
}
