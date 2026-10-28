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
                    key: "border_radius".into(),
                    label: "Corner Radius".into(),
                    description: "Radius of window rounded corners in pixels".into(),
                    r#type: "int".into(),
                    default: "10".into(),
                },
                OptionItem {
                    key: "border_px".into(),
                    label: "Border Width".into(),
                    description: "Thickness of window borders in pixels".into(),
                    r#type: "int".into(),
                    default: "2".into(),
                },
                OptionItem {
                    key: "focus_color".into(),
                    label: "Focused Border Color".into(),
                    description: "Color of active window border".into(),
                    r#type: "color".into(),
                    default: "#89b4fa".into(),
                },
                OptionItem {
                    key: "border_color".into(),
                    label: "Unfocused Border Color".into(),
                    description: "Color of inactive window border".into(),
                    r#type: "color".into(),
                    default: "#313244".into(),
                },
                OptionItem {
                    key: "blur".into(),
                    label: "Enable Blur".into(),
                    description: "Enable background blur via scenefx".into(),
                    r#type: "boolean".into(),
                    default: "true".into(),
                },
                OptionItem {
                    key: "blur_params_radius".into(),
                    label: "Blur Radius".into(),
                    description: "Blur kernel radius in pixels".into(),
                    r#type: "int".into(),
                    default: "5".into(),
                },
                OptionItem {
                    key: "shadows".into(),
                    label: "Enable Shadow".into(),
                    description: "Render drop shadows under windows".into(),
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
                    key: "tag_rule".into(),
                    label: "Default Layout".into(),
                    description: "Tag rule applied to all tags, e.g. id:*,layout_name:scroller (layouts: tile, scroller, monocle, grid, deck, dwindle, ...)".into(),
                    r#type: "string".into(),
                    default: "id:*,layout_name:scroller".into(),
                },
                OptionItem {
                    key: "gap_inner_horizontal".into(),
                    label: "Inner Gap (Horizontal)".into(),
                    description: "Horizontal gap between adjacent windows in pixels".into(),
                    r#type: "int".into(),
                    default: "6".into(),
                },
                OptionItem {
                    key: "gap_inner_vertical".into(),
                    label: "Inner Gap (Vertical)".into(),
                    description: "Vertical gap between adjacent windows in pixels".into(),
                    r#type: "int".into(),
                    default: "6".into(),
                },
                OptionItem {
                    key: "gap_outer_horizontal".into(),
                    label: "Outer Gap (Horizontal)".into(),
                    description: "Horizontal gap between windows and screen edges in pixels".into(),
                    r#type: "int".into(),
                    default: "10".into(),
                },
                OptionItem {
                    key: "gap_outer_vertical".into(),
                    label: "Outer Gap (Vertical)".into(),
                    description: "Vertical gap between windows and screen edges in pixels".into(),
                    r#type: "int".into(),
                    default: "10".into(),
                },
                OptionItem {
                    key: "default_master_factor".into(),
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
                    key: "animations".into(),
                    label: "Enable Animations".into(),
                    description: "Enable smooth window and tag animations".into(),
                    r#type: "boolean".into(),
                    default: "true".into(),
                },
                OptionItem {
                    key: "animation_duration_move".into(),
                    label: "Move Duration (ms)".into(),
                    description: "Window move/resize animation speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "500".into(),
                },
                OptionItem {
                    key: "animation_duration_open".into(),
                    label: "Open Duration (ms)".into(),
                    description: "Window open animation speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "400".into(),
                },
                OptionItem {
                    key: "animation_duration_tag".into(),
                    label: "Tag Duration (ms)".into(),
                    description: "Tag switch animation speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "300".into(),
                },
                OptionItem {
                    key: "animation_duration_close".into(),
                    label: "Close Duration (ms)".into(),
                    description: "Window close animation speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "300".into(),
                },
                OptionItem {
                    key: "animation_duration_focus".into(),
                    label: "Focus Duration (ms)".into(),
                    description: "Focus change animation speed in milliseconds".into(),
                    r#type: "int".into(),
                    default: "0".into(),
                },
            ],
        },
    ]
}
