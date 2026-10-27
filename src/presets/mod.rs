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
                ("corner_radius".into(), "10".into()),
                ("border_width".into(), "2".into()),
                ("focused_border_color".into(), "#89b4fa".into()),
                ("unfocused_border_color".into(), "#313244".into()),
                ("blur_enabled".into(), "true".into()),
                ("gaps_in".into(), "6".into()),
                ("gaps_out".into(), "10".into()),
                ("default_layout".into(), "scroller".into()),
            ]),
        },
        Preset {
            id: "tokyo-night".into(),
            name: "Tokyo Night".into(),
            description: "Deep navy background with neon cyan and magenta accents".into(),
            settings: BTreeMap::from([
                ("corner_radius".into(), "12".into()),
                ("border_width".into(), "2".into()),
                ("focused_border_color".into(), "#7aa2f7".into()),
                ("unfocused_border_color".into(), "#1a1b26".into()),
                ("blur_enabled".into(), "true".into()),
                ("gaps_in".into(), "8".into()),
                ("gaps_out".into(), "14".into()),
                ("default_layout".into(), "master".into()),
            ]),
        },
        Preset {
            id: "nord".into(),
            name: "Nord".into(),
            description: "Arctic frost blues and icy polar night aesthetic".into(),
            settings: BTreeMap::from([
                ("corner_radius".into(), "8".into()),
                ("border_width".into(), "2".into()),
                ("focused_border_color".into(), "#88c0d0".into()),
                ("unfocused_border_color".into(), "#3b4252".into()),
                ("blur_enabled".into(), "false".into()),
                ("gaps_in".into(), "5".into()),
                ("gaps_out".into(), "10".into()),
                ("default_layout".into(), "scroller".into()),
            ]),
        },
        Preset {
            id: "gruvbox".into(),
            name: "Gruvbox".into(),
            description: "Retro warm amber and orange tones with crisp borders".into(),
            settings: BTreeMap::from([
                ("corner_radius".into(), "4".into()),
                ("border_width".into(), "3".into()),
                ("focused_border_color".into(), "#fe8019".into()),
                ("unfocused_border_color".into(), "#3c3836".into()),
                ("blur_enabled".into(), "false".into()),
                ("gaps_in".into(), "6".into()),
                ("gaps_out".into(), "10".into()),
                ("default_layout".into(), "master".into()),
            ]),
        },
        Preset {
            id: "glass".into(),
            name: "Glass".into(),
            description: "Translucent windows with deep blur, soft shadow and hairline borders".into(),
            settings: BTreeMap::from([
                ("corner_radius".into(), "16".into()),
                ("border_width".into(), "1".into()),
                ("focused_border_color".into(), "#cba6f7".into()),
                ("unfocused_border_color".into(), "#45475a".into()),
                ("blur_enabled".into(), "true".into()),
                ("focused_opacity".into(), "0.92".into()),
                ("unfocused_opacity".into(), "0.78".into()),
                ("gaps_in".into(), "10".into()),
                ("gaps_out".into(), "18".into()),
                ("default_layout".into(), "scroller".into()),
            ]),
        },
        Preset {
            id: "zen".into(),
            name: "Zen".into(),
            description: "No borders, no gaps, no blur: maximum focus and screen space".into(),
            settings: BTreeMap::from([
                ("corner_radius".into(), "0".into()),
                ("border_width".into(), "0".into()),
                ("blur_enabled".into(), "false".into()),
                ("gaps_in".into(), "0".into()),
                ("gaps_out".into(), "0".into()),
                ("default_layout".into(), "monocle".into()),
            ]),
        },
    ]
}

pub fn find_preset(name_or_id: &str) -> Option<Preset> {
    list_presets()
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id))
}
