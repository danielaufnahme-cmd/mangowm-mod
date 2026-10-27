//! Mangobar status bar model definitions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarModules {
    #[serde(default = "default_left")]
    pub left: Vec<String>,
    #[serde(default = "default_center")]
    pub center: Vec<String>,
    #[serde(default = "default_right")]
    pub right: Vec<String>,
}

fn default_left() -> Vec<String> {
    vec!["tag".into(), "layout".into(), "title".into()]
}
fn default_center() -> Vec<String> {
    vec!["clock".into()]
}
fn default_right() -> Vec<String> {
    vec![
        "cpu".into(),
        "memory".into(),
        "brightness".into(),
        "volume".into(),
        "battery".into(),
        "tray".into(),
    ]
}

impl Default for BarModules {
    fn default() -> Self {
        Self {
            left: default_left(),
            center: default_center(),
            right: default_right(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mangobar {
    #[serde(default = "default_position")]
    pub position: String, // "top" or "bottom"
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_spacing")]
    pub spacing: u32,
    #[serde(default = "default_floating")]
    pub floating: bool,
    #[serde(default = "default_margin")]
    pub margin: u32,
    #[serde(default)]
    pub modules: BarModules,
    #[serde(default = "default_bg")]
    pub bg_color: String,
    #[serde(default = "default_fg")]
    pub fg_color: String,
    #[serde(default = "default_radius")]
    pub radius: u32,
}

fn default_position() -> String { "top".to_string() }
fn default_height() -> u32 { 32 }
fn default_spacing() -> u32 { 6 }
fn default_floating() -> bool { true }
fn default_margin() -> u32 { 8 }
fn default_bg() -> String { "rgba(30, 30, 46, 0.92)".to_string() }
fn default_fg() -> String { "#cdd6f4".to_string() }
fn default_radius() -> u32 { 10 }

impl Default for Mangobar {
    fn default() -> Self {
        Self {
            position: default_position(),
            height: default_height(),
            spacing: default_spacing(),
            floating: default_floating(),
            margin: default_margin(),
            modules: BarModules::default(),
            bg_color: default_bg(),
            fg_color: default_fg(),
            radius: default_radius(),
        }
    }
}
