//! Presets tailored for MangoWM & Mangobar.
//!
//! Every preset is a tiling setup (Mango's `tile` layout on every tag, new
//! windows join the stack like on Hyprland) with its own colors, gaps and an
//! original wallpaper (`data/wallpapers/<id>.png`, see `render.py` there).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::manager::{ApplyResult, ConfigManager};
use crate::core::schema::find_option;
use crate::core::values::format_option_value;
use crate::wallpaper::{finish_wallpaper, stage_wallpaper, WALLPAPER_DIR};

/// Settings every preset shares: tile all tags, new windows go to the stack.
pub const TILING: &[(&str, &str)] = &[
    ("tag_rule", "id:*,layout_name:tile"),
    ("new_is_master", "0"),
];

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

impl Preset {
    /// File name of the preset's wallpaper.
    pub fn wallpaper_file(&self) -> String {
        format!("{}.png", self.id)
    }
}

/// A preset: the shared tiling settings, `(inner, outer)` gaps on both axes,
/// then its own settings.
fn preset(
    id: &str,
    name: &str,
    description: &str,
    (inner, outer): (&str, &str),
    settings: &[(&str, &str)],
) -> Preset {
    let gaps = [
        ("gap_inner_horizontal", inner),
        ("gap_inner_vertical", inner),
        ("gap_outer_horizontal", outer),
        ("gap_outer_vertical", outer),
    ];
    Preset {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        settings: TILING
            .iter()
            .chain(&gaps)
            .chain(settings)
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

pub fn list_presets() -> Vec<Preset> {
    vec![
        preset(
            "clean",
            "Clean",
            "Balanced everyday layout with calm blue borders and modest gaps",
            ("6", "10"),
            &[
                ("border_radius", "10"),
                ("border_px", "2"),
                ("focus_color", "#89b4fa"),
                ("border_color", "#313244"),
                ("blur", "true"),
            ],
        ),
        preset(
            "tokyo-night",
            "Tokyo Night",
            "Deep navy background with neon blue and cyan accents",
            ("8", "14"),
            &[
                ("border_radius", "12"),
                ("border_px", "2"),
                ("focus_color", "#7aa2f7"),
                ("border_color", "#1a1b26"),
                ("blur", "true"),
            ],
        ),
        preset(
            "nord",
            "Nord",
            "Arctic frost blues and icy polar night aesthetic",
            ("5", "10"),
            &[
                ("border_radius", "8"),
                ("border_px", "2"),
                ("focus_color", "#88c0d0"),
                ("border_color", "#3b4252"),
                ("blur", "false"),
            ],
        ),
        preset(
            "gruvbox",
            "Gruvbox",
            "Retro warm amber and orange tones with crisp borders",
            ("6", "10"),
            &[
                ("border_radius", "4"),
                ("border_px", "3"),
                ("focus_color", "#fe8019"),
                ("border_color", "#3c3836"),
                ("blur", "false"),
            ],
        ),
        preset(
            "glass",
            "Glass",
            "Translucent windows with deep blur, soft shadow and hairline borders",
            ("10", "18"),
            &[
                ("border_radius", "16"),
                ("border_px", "1"),
                ("focus_color", "#cba6f7"),
                ("border_color", "#45475a"),
                ("blur", "true"),
                ("focused_opacity", "0.92"),
                ("unfocused_opacity", "0.78"),
            ],
        ),
        preset(
            "zen",
            "Zen",
            "No borders, no gaps, no blur: maximum focus and screen space",
            ("0", "0"),
            &[
                ("border_radius", "0"),
                ("border_px", "0"),
                ("blur", "false"),
            ],
        ),
        preset(
            "everforest",
            "Everforest",
            "Soft forest greens on misty grey, easy on the eyes",
            ("7", "12"),
            &[
                ("border_radius", "10"),
                ("border_px", "2"),
                ("focus_color", "#a7c080"),
                ("border_color", "#374145"),
                ("blur", "false"),
            ],
        ),
        preset(
            "rose-pine",
            "Rosé Pine",
            "Muted rose and gold on charcoal, rounded and airy",
            ("8", "14"),
            &[
                ("border_radius", "12"),
                ("border_px", "2"),
                ("focus_color", "#ebbcba"),
                ("border_color", "#2a2730"),
                ("blur", "true"),
            ],
        ),
        preset(
            "kanagawa",
            "Kanagawa",
            "Ink-blue waves and old-paper accents, quiet and focused",
            ("6", "12"),
            &[
                ("border_radius", "6"),
                ("border_px", "2"),
                ("focus_color", "#7e9cd8"),
                ("border_color", "#2a2a37"),
                ("blur", "false"),
            ],
        ),
        preset(
            "solarized",
            "Solarized",
            "Classic deep-sea teal with cyan focus and tight gaps",
            ("6", "10"),
            &[
                ("border_radius", "6"),
                ("border_px", "2"),
                ("focus_color", "#2aa198"),
                ("border_color", "#073642"),
                ("blur", "false"),
            ],
        ),
        preset(
            "latte",
            "Latte",
            "Light and bright: pale greys, crisp blue focus, roomy gaps",
            ("8", "14"),
            &[
                ("border_radius", "12"),
                ("border_px", "2"),
                ("focus_color", "#1e66f5"),
                ("border_color", "#ccd0da"),
                ("blur", "false"),
            ],
        ),
        preset(
            "graphite",
            "Graphite",
            "Monochrome charcoal with a thin white focus line",
            ("6", "10"),
            &[
                ("border_radius", "8"),
                ("border_px", "1"),
                ("focus_color", "#e6e6e6"),
                ("border_color", "#2a2b2f"),
                ("blur", "false"),
            ],
        ),
    ]
}

pub fn find_preset(name_or_id: &str) -> Option<Preset> {
    list_presets()
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id))
}

/// The preset's wallpaper: the installed copy in `<data dir>/wallpapers`, else
/// the one in the source checkout this binary was built from.
pub fn wallpaper_path(data_dir: &Path, p: &Preset) -> Option<PathBuf> {
    let file = p.wallpaper_file();
    [
        data_dir.join(WALLPAPER_DIR).join(&file),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join(WALLPAPER_DIR)
            .join(&file),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Stage the preset's settings and wallpaper (an `exec_once=swaybg ...` that
/// replaces any other managed swaybg line), apply everything that is staged,
/// and when Mango confirmed the reload also switch the wallpaper live.
pub fn apply_preset(mgr: &mut ConfigManager, p: &Preset) -> Result<ApplyResult> {
    // Drop what another preset left behind (e.g. Glass's opacity) when this
    // preset does not set it; values the user picked themselves stay.
    for other in list_presets() {
        for (k, v) in other.settings.iter().filter(|(k, _)| !p.settings.contains_key(*k)) {
            let from_preset = find_option(k)
                .and_then(|o| format_option_value(&o.r#type, &o.choices, v).ok())
                .is_some_and(|v| mgr.staged().settings.get(k) == Some(&v));
            if from_preset {
                mgr.reset(k)?;
            }
        }
    }
    for (k, v) in &p.settings {
        mgr.set(k, v)?;
    }
    let wallpaper = wallpaper_path(&mgr.data_dir, p);
    if let Some(path) = &wallpaper {
        stage_wallpaper(mgr, path)?;
    }

    let mut res = mgr.apply()?;
    let Some(path) = wallpaper else {
        res.warnings.push(format!(
            "wallpaper {} not found in {} (run install.sh); the preset was applied without it",
            p.wallpaper_file(),
            mgr.data_dir.join(WALLPAPER_DIR).display()
        ));
        return Ok(res);
    };
    finish_wallpaper(mgr, &mut res, &path);
    Ok(res)
}
