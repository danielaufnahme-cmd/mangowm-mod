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
    vec!["workspaces".into(), "layout".into(), "window".into()]
}
fn default_center() -> Vec<String> {
    vec!["clock".into()]
}
fn default_right() -> Vec<String> {
    vec![
        "cpu".into(),
        "memory".into(),
        "backlight".into(),
        "pulseaudio".into(),
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
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub modules: BarModules,
    #[serde(default = "default_bg")]
    pub bg_color: String,
    #[serde(default = "default_fg")]
    pub fg_color: String,
    #[serde(default = "default_radius")]
    pub radius: u32,
}

fn default_height() -> u32 { 32 }
fn default_bg() -> String { "#1e1e2eeb".to_string() }
fn default_fg() -> String { "#cdd6f4".to_string() }
fn default_radius() -> u32 { 10 }

impl Default for Mangobar {
    fn default() -> Self {
        Self {
            height: default_height(),
            modules: BarModules::default(),
            bg_color: default_bg(),
            fg_color: default_fg(),
            radius: default_radius(),
        }
    }
}
