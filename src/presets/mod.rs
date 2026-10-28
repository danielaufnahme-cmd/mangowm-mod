//! Presets tailored for MangoWM & Mangobar.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

pub fn list_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "clean".into(),
            name: "Clean".into(),
            description: "Balanced everyday layout with calm blue borders and modest gaps".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "10".into()),
                ("border_px".into(), "2".into()),
                ("focus_color".into(), "#89b4fa".into()),
                ("border_color".into(), "#313244".into()),
                ("blur".into(), "true".into()),
                ("gap_inner_horizontal".into(), "6".into()),
                ("gap_inner_vertical".into(), "6".into()),
                ("gap_outer_horizontal".into(), "10".into()),
                ("gap_outer_vertical".into(), "10".into()),
                ("tag_rule".into(), "id:*,layout_name:scroller".into()),
            ]),
        },
        Preset {
            id: "tokyo-night".into(),
            name: "Tokyo Night".into(),
            description: "Deep navy background with neon cyan and magenta accents".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "12".into()),
                ("border_px".into(), "2".into()),
                ("focus_color".into(), "#7aa2f7".into()),
                ("border_color".into(), "#1a1b26".into()),
                ("blur".into(), "true".into()),
                ("gap_inner_horizontal".into(), "8".into()),
                ("gap_inner_vertical".into(), "8".into()),
                ("gap_outer_horizontal".into(), "14".into()),
                ("gap_outer_vertical".into(), "14".into()),
                ("tag_rule".into(), "id:*,layout_name:tile".into()),
            ]),
        },
        Preset {
            id: "nord".into(),
            name: "Nord".into(),
            description: "Arctic frost blues and icy polar night aesthetic".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "8".into()),
                ("border_px".into(), "2".into()),
                ("focus_color".into(), "#88c0d0".into()),
                ("border_color".into(), "#3b4252".into()),
                ("blur".into(), "false".into()),
                ("gap_inner_horizontal".into(), "5".into()),
                ("gap_inner_vertical".into(), "5".into()),
                ("gap_outer_horizontal".into(), "10".into()),
                ("gap_outer_vertical".into(), "10".into()),
                ("tag_rule".into(), "id:*,layout_name:scroller".into()),
            ]),
        },
        Preset {
            id: "gruvbox".into(),
            name: "Gruvbox".into(),
            description: "Retro warm amber and orange tones with crisp borders".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "4".into()),
                ("border_px".into(), "3".into()),
                ("focus_color".into(), "#fe8019".into()),
                ("border_color".into(), "#3c3836".into()),
                ("blur".into(), "false".into()),
                ("gap_inner_horizontal".into(), "6".into()),
                ("gap_inner_vertical".into(), "6".into()),
                ("gap_outer_horizontal".into(), "10".into()),
                ("gap_outer_vertical".into(), "10".into()),
                ("tag_rule".into(), "id:*,layout_name:tile".into()),
            ]),
        },
        Preset {
            id: "glass".into(),
            name: "Glass".into(),
            description: "Translucent windows with deep blur, soft shadow and hairline borders".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "16".into()),
                ("border_px".into(), "1".into()),
                ("focus_color".into(), "#cba6f7".into()),
                ("border_color".into(), "#45475a".into()),
                ("blur".into(), "true".into()),
                ("focused_opacity".into(), "0.92".into()),
                ("unfocused_opacity".into(), "0.78".into()),
                ("gap_inner_horizontal".into(), "10".into()),
                ("gap_inner_vertical".into(), "10".into()),
                ("gap_outer_horizontal".into(), "18".into()),
                ("gap_outer_vertical".into(), "18".into()),
                ("tag_rule".into(), "id:*,layout_name:scroller".into()),
            ]),
        },
        Preset {
            id: "zen".into(),
            name: "Zen".into(),
            description: "No borders, no gaps, no blur: maximum focus and screen space".into(),
            settings: BTreeMap::from([
                ("border_radius".into(), "0".into()),
                ("border_px".into(), "0".into()),
                ("blur".into(), "false".into()),
                ("gap_inner_horizontal".into(), "0".into()),
                ("gap_inner_vertical".into(), "0".into()),
                ("gap_outer_horizontal".into(), "0".into()),
                ("gap_outer_vertical".into(), "0".into()),
                ("tag_rule".into(), "id:*,layout_name:monocle".into()),
            ]),
        },
    ]
}

pub fn find_preset(name_or_id: &str) -> Option<Preset> {
    list_presets()
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id))
}
