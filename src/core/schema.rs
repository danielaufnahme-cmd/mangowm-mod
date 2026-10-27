//! Schema definitions for MangoWM settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub r#type: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub description: String,
    pub options: Vec<OptionItem>,
}

pub fn get_schema_pages() -> Vec<Page> {
    vec![
        Page {
            id: "scenefx".into(),
            title: "SceneFX & Decoration".into(),
            description: "Corners, borders, blur, opacity and drop shadows".into(),
            options: vec![
                OptionItem {
                    key: "corner_radius".into(),
                    label: "Corner Radius".into(),
                    description: "Radius of window rounded corners in pixels".into(),
                    r#type: "int".into(),
                    default: "10".into(),
                },
                OptionItem {
                    key: "border_width".into(),
                    label: "Border Width".into(),
                    description: "Thickness of window borders in pixels".into(),
                    r#type: "int".into(),
                    default: "2".into(),
                },
                OptionItem {
                    key: "focused_border_color".into(),
                    label: "Focused Border Color".into(),
                    description: "Color of active window border".into(),
                    r#type: "color".into(),
                    default: "#89b4fa".into(),
                },
                OptionItem {
                    key: "unfocused_border_color".into(),
                    label: "Unfocused Border Color".into(),
                    description: "Color of inactive window border".into(),
                    r#type: "color".into(),
                    default: "#313244".into(),
                },
                OptionItem {
                    key: "blur_enabled".into(),
                    label: "Enable Blur".into(),
                    description: "Enable background blur via scenefx".into(),
                    r#type: "boolean".into(),
                    default: "true".into(),
                },
                OptionItem {
                    key: "blur_radius".into(),
                    label: "Blur Radius".into(),
                    description: "Blur blur size/passes".into(),
                    r#type: "int".into(),
                    default: "5".into(),
                },
                OptionItem {
                    key: "shadow_enabled".into(),
                    label: "Enable Shadow".into(),
                    description: "Render drop shadows under floating windows".into(),
                    r#type: "boolean".into(),
                    default: "true".into(),
                },
                OptionItem {
                    key: "focused_opacity".into(),
                    label: "Focused Opacity".into(),
                    description: "Opacity of focused window (0.0 - 1.0)".into(),
                    r#type: "float".into(),
                    default: "1.0".into(),
                },
                OptionItem {
                    key: "unfocused_opacity".into(),
                    label: "Unfocused Opacity".into(),
                    description: "Opacity of unfocused windows (0.0 - 1.0)".into(),
                    r#type: "float".into(),
                    default: "0.92".into(),
                },
            ],
        },
        Page {
            id: "layouts".into(),
            title: "Layouts & Gaps".into(),
            description: "Tiling layouts, master ratios, and window gaps".into(),
            options: vec![
                OptionItem {
                    key: "default_layout".into(),
                    label: "Default Layout".into(),
                    description: "Starting layout: scroller, master, monocle, grid, deck, dwindle".into(),
                    r#type: "string".into(),
                    default: "scroller".into(),
                },
                OptionItem {
                    key: "gaps_in".into(),
                    label: "Inner Gaps".into(),
                    description: "Gap between adjacent windows in pixels".into(),
                    r#type: "int".into(),
                    default: "6".into(),
                },
                OptionItem {
                    key: "gaps_out".into(),
                    label: "Outer Gaps".into(),
                    description: "Gap between windows and screen edges in pixels".into(),
                    r#type: "int".into(),
                    default: "10".into(),
                },
                OptionItem {
                    key: "master_ratio".into(),
                    label: "Master Split Ratio".into(),
                    description: "Screen proportion given to master window (0.1 - 0.9)".into(),
                    r#type: "float".into(),
                    default: "0.55".into(),
                },
            ],
        },
        Page {
            id: "animations".into(),
            title: "Animations".into(),
            description: "Smooth window animations and transitions".into(),
            options: vec![
                OptionItem {
                    key: "animations_enabled".into(),
                    label: "Enable Animations".into(),
                    description: "Enable smooth window and tag animations".into(),
                    r#type: "boolean".into(),
                    default: "true".into(),
                },
                OptionItem {
                    key: "animation_duration".into(),
                    label: "Animation Duration (ms)".into(),
                    description: "Transition speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "250".into(),
                },
            ],
        },
    ]
}
