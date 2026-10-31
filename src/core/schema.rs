//! Schema definitions for MangoWM settings.
//!
//! The option list itself lives in `schema_data.rs` (taken from Mango's own
//! source and docs); this module turns it into pages for the CLI.

use serde::{Deserialize, Serialize};

use super::schema_data::{OPTIONS, PAGES};

/// Layout names accepted by Mango's `layout_name` (MangoWM 0.18.4 `layouts[]`).
pub const LAYOUTS: &[&str] = &[
    "tile",
    "scroller",
    "grid",
    "monocle",
    "deck",
    "center_tile",
    "right_tile",
    "vertical_scroller",
    "vertical_tile",
    "vertical_grid",
    "vertical_deck",
    "dwindle",
    "fair",
    "vertical_fair",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub r#type: String,
    pub default: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub description: String,
    pub options: Vec<OptionItem>,
}

/// The "Default Layout" entry: a `tag_rule` for every tag (`id:*`).
fn default_layout_option() -> OptionItem {
    OptionItem {
        key: "tag_rule".into(),
        label: "Default Layout".into(),
        description: format!(
            "Tag rule applied to all tags, e.g. id:*,layout_name:scroller (layouts: {})",
            LAYOUTS.join(", ")
        ),
        r#type: "tag_rule".into(),
        default: String::new(),
        choices: Vec::new(),
    }
}

pub fn get_schema_pages() -> Vec<Page> {
    PAGES
        .iter()
        .map(|(id, title, description)| {
            let mut options: Vec<OptionItem> = OPTIONS
                .iter()
                .filter(|row| row.0 == *id)
                .map(|&(_, key, label, ty, default, desc, choices)| OptionItem {
                    key: key.into(),
                    label: label.into(),
                    description: desc.into(),
                    r#type: ty.into(),
                    default: default.into(),
                    choices: choices.iter().map(|c| c.to_string()).collect(),
                })
                .collect();
            if *id == "layouts" {
                options.insert(0, default_layout_option());
            }
            Page {
                id: id.to_string(),
                title: title.to_string(),
                description: description.to_string(),
                options,
            }
        })
        .collect()
}

/// Look up one option by its Mango key.
pub fn find_option(key: &str) -> Option<OptionItem> {
    get_schema_pages()
        .into_iter()
        .flat_map(|p| p.options)
        .find(|o| o.key == key)
}

/// Find options whose key or label contains `query` (case-insensitive).
pub fn search_options(query: &str) -> Vec<OptionItem> {
    let q = query.to_lowercase();
    get_schema_pages()
        .into_iter()
        .flat_map(|p| p.options)
        .filter(|o| o.key.contains(&q) || o.label.to_lowercase().contains(&q))
        .collect()
}
