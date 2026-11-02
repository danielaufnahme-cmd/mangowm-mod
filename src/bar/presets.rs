//! Ready-made bar looks. A preset sets the layout, sizes, radii, module set
//! and colors; the program, monitors, layer, font family, login start and
//! module options stay as they are.
//!
//! Colors: neutral dark greys with one restrained accent (teal #76ccb1).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::model::{module_base, BarConfig};
use crate::core::fsutil::atomic_write;

#[derive(Debug, Clone, Copy)]
pub struct BarPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    bg: &'static str,
    fg: &'static str,
    accent: &'static str,
    module_bg: &'static str,
    hover: &'static str,
    urgent: &'static str,
    inactive: &'static str,
    border_color: &'static str,
    border_width: u32,
    /// -1 = pill.
    radius: i32,
    module_radius: i32,
    margin_top: u32,
    margin_side: u32,
    module_padding: u32,
    module_margin: u32,
    module_gap: u32,
    font_size: u32,
    font_weight: &'static str,
    title_length: u32,
    tag_fill: bool,
    left: &'static [&'static str],
    center: &'static [&'static str],
    right: &'static [&'static str],
}

const FULL_LEFT: &[&str] = &["workspaces", "layout", "window"];
const FULL_RIGHT: &[&str] = &["cpu", "memory", "pulseaudio", "network", "battery", "tray"];

/// Modules a preset keeps when the bar already has them (hardware or the
/// user's own); they go to the right section, before the tray.
fn kept_module(name: &str) -> bool {
    name.starts_with("custom/")
        || name.contains('#') && module_base(name) == "battery"
        || matches!(
            name,
            "backlight" | "keyboardlayout" | "keymode" | "hideclients"
        )
}

pub const BAR_PRESETS: &[BarPreset] = &[
    BarPreset {
        id: "islands",
        name: "Islands",
        description: "Each module a separate rounded block on a clear bar. Looks the same in \
                      Mangobar and Waybar",
        bg: "#00000000",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#16181ce6",
        hover: "#2a2e35ff",
        urgent: "#e5737aff",
        inactive: "#7c828cff",
        border_color: "#ffffff14",
        border_width: 0,
        radius: 14,
        module_radius: 10,
        margin_top: 6,
        margin_side: 8,
        module_padding: 12,
        module_margin: 0,
        module_gap: 3,
        font_size: 13,
        font_weight: "medium",
        title_length: 48,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "floating-pill",
        name: "Floating Pill",
        description: "One rounded, floating bar with flat modules. Mangobar draws it with \
                      square ends",
        bg: "#16181cf0",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff14",
        urgent: "#e5737aff",
        inactive: "#7c828cff",
        border_color: "#ffffff12",
        border_width: 1,
        radius: -1,
        module_radius: -1,
        margin_top: 8,
        margin_side: 12,
        module_padding: 10,
        module_margin: 4,
        module_gap: 2,
        font_size: 13,
        font_weight: "medium",
        title_length: 56,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "solid-dock",
        name: "Solid Dock",
        description: "Opaque, full width, edge to edge, with a solid accent block on the \
                      active tag",
        bg: "#111316ff",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff12",
        urgent: "#e5737aff",
        inactive: "#6f757eff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 12,
        module_margin: 0,
        module_gap: 0,
        font_size: 13,
        font_weight: "medium",
        title_length: 64,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "minimal",
        name: "Minimal",
        description: "Flat and see-through, text only: the active tag is accent text",
        bg: "#0d0e11a6",
        fg: "#dfe2e6ff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff10",
        urgent: "#e5737aff",
        inactive: "#6b7079ff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 10,
        module_margin: 0,
        module_gap: 0,
        font_size: 13,
        font_weight: "normal",
        title_length: 48,
        tag_fill: false,
        left: &["workspaces", "window"],
        center: &["clock"],
        right: &["pulseaudio", "network", "battery", "tray"],
    },
    BarPreset {
        id: "compact",
        name: "Compact",
        description: "Thin full-width bar with a smaller font; the clock sits on the right",
        bg: "#16181cf2",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff12",
        urgent: "#e5737aff",
        inactive: "#6f757eff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 7,
        module_margin: 0,
        module_gap: 0,
        font_size: 12,
        font_weight: "medium",
        title_length: 40,
        tag_fill: true,
        left: FULL_LEFT,
        center: &[],
        right: &["cpu", "memory", "pulseaudio", "network", "battery", "clock", "tray"],
    },
];

/// The look of a fresh bar.json.
pub const DEFAULT_BAR_PRESET: &str = "islands";

pub fn find_bar_preset(id: &str) -> Option<&'static BarPreset> {
    BAR_PRESETS.iter().find(|p| p.id == id)
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl BarPreset {
    /// Set the preset's look on `bar`. The height becomes automatic.
    pub fn apply(&self, bar: &mut BarConfig) {
        bar.bg_color = self.bg.to_string();
        bar.fg_color = self.fg.to_string();
        bar.accent_color = self.accent.to_string();
        bar.module_bg_color = self.module_bg.to_string();
        bar.hover_color = self.hover.to_string();
        bar.urgent_color = self.urgent.to_string();
        bar.inactive_color = self.inactive.to_string();
        bar.border_color = self.border_color.to_string();
        bar.border_width = self.border_width;
        bar.radius = self.radius;
        bar.module_radius = self.module_radius;
        // a bottom Waybar floats the same distance above the screen edge
        let (top, bottom) = if bar.position == "bottom" {
            (0, self.margin_top)
        } else {
            (self.margin_top, 0)
        };
        bar.margin_top = top;
        bar.margin_bottom = bottom;
        bar.margin_left = self.margin_side;
        bar.margin_right = self.margin_side;
        // the gap between modules is their side margin (Mangobar has no spacing)
        bar.spacing = 0;
        bar.module_padding = self.module_padding;
        bar.module_margin = self.module_margin;
        bar.module_gap = self.module_gap;
        bar.font_size = self.font_size;
        bar.font_weight = self.font_weight.to_string();
        bar.title_length = self.title_length;
        bar.tag_fill = self.tag_fill;
        bar.auto_height = true;

        let kept: Vec<String> = bar
            .modules
            .all()
            .filter(|m| kept_module(m))
            .cloned()
            .collect();
        bar.modules.left = strings(self.left);
        bar.modules.center = strings(self.center);
        bar.modules.right = strings(self.right);
        let at = bar
            .modules
            .right
            .iter()
            .position(|m| m == "tray")
            .unwrap_or(bar.modules.right.len());
        for (i, m) in kept.into_iter().enumerate() {
            bar.modules.right.insert(at + i, m);
        }
    }

    /// `bar` with this preset applied.
    pub fn applied_to(&self, bar: &BarConfig) -> BarConfig {
        let mut out = bar.clone();
        self.apply(&mut out);
        out
    }
}

// ---- the user's own presets ----

/// Prefix of a user preset's id in [`BarConfig::preset`].
pub const USER_PREFIX: &str = "user:";

/// A bar look the user saved: every setting except the per-machine ones
/// (program, login start, monitors).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserPreset {
    /// File stem in [`user_presets_dir`].
    #[serde(skip)]
    pub slug: String,
    pub name: String,
    pub bar: BarConfig,
}

pub fn user_presets_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bar-presets")
}

/// "My Bar 2" -> "my-bar-2".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "preset".to_string()
    } else {
        out
    }
}

/// Copy `bar` without its per-machine settings and the preset mark.
fn portable(bar: &BarConfig) -> BarConfig {
    let mut out = bar.clone();
    let base = BarConfig::default();
    out.backend = base.backend;
    out.start_at_login = base.start_at_login;
    out.output = base.output;
    out.preset = None;
    out
}

impl UserPreset {
    pub fn id(&self) -> String {
        format!("{}{}", USER_PREFIX, self.slug)
    }

    fn path(&self, data_dir: &Path) -> PathBuf {
        user_presets_dir(data_dir).join(format!("{}.json", self.slug))
    }

    fn save(&self, data_dir: &Path) -> Result<()> {
        atomic_write(
            self.path(data_dir),
            &(serde_json::to_string_pretty(self)? + "\n"),
        )
    }

    /// Set the preset's settings on `bar`; the program, login start and
    /// monitors stay.
    pub fn apply(&self, bar: &mut BarConfig) {
        let keep = (bar.backend, bar.start_at_login, bar.output.clone(), bar.preset.clone());
        *bar = self.bar.clone();
        (bar.backend, bar.start_at_login, bar.output, bar.preset) = keep;
    }

    pub fn applied_to(&self, bar: &BarConfig) -> BarConfig {
        let mut out = bar.clone();
        self.apply(&mut out);
        out
    }
}

/// The user's presets, sorted by name. Unreadable files are skipped.
pub fn list_user_presets(data_dir: &Path) -> Vec<UserPreset> {
    let Ok(entries) = fs::read_dir(user_presets_dir(data_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<UserPreset> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "json" {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            let text = fs::read_to_string(&path).ok()?;
            let mut p: UserPreset = serde_json::from_str(&text).ok()?;
            p.slug = slug;
            p.bar.validate().ok()?;
            Some(p)
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

pub fn find_user_preset(data_dir: &Path, slug: &str) -> Option<UserPreset> {
    list_user_presets(data_dir).into_iter().find(|p| p.slug == slug)
}

fn check_name(data_dir: &Path, name: &str, except: Option<&str>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("type a name for the preset");
    }
    if name.chars().count() > 40 {
        bail!("preset name must be 40 characters or less");
    }
    let taken = BAR_PRESETS
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(name))
        || list_user_presets(data_dir)
            .iter()
            .any(|p| Some(p.slug.as_str()) != except && p.name.eq_ignore_ascii_case(name));
    if taken {
        bail!("there is already a preset called “{}”", name);
    }
    Ok(name.to_string())
}

/// Save `bar` as a new preset called `name`.
pub fn save_user_preset(data_dir: &Path, name: &str, bar: &BarConfig) -> Result<UserPreset> {
    let name = check_name(data_dir, name, None)?;
    bar.validate()?;
    let base = slugify(&name);
    let mut slug = base.clone();
    let mut n = 2;
    while user_presets_dir(data_dir)
        .join(format!("{}.json", slug))
        .exists()
    {
        slug = format!("{}-{}", base, n);
        n += 1;
    }
    let preset = UserPreset {
        slug,
        name,
        bar: portable(bar),
    };
    preset.save(data_dir)?;
    Ok(preset)
}

/// Replace a user preset's settings with `bar`.
pub fn overwrite_user_preset(data_dir: &Path, slug: &str, bar: &BarConfig) -> Result<UserPreset> {
    let mut p = find_user_preset(data_dir, slug).context("no such preset")?;
    bar.validate()?;
    p.bar = portable(bar);
    p.save(data_dir)?;
    Ok(p)
}

pub fn rename_user_preset(data_dir: &Path, slug: &str, name: &str) -> Result<UserPreset> {
    let mut p = find_user_preset(data_dir, slug).context("no such preset")?;
    p.name = check_name(data_dir, name, Some(slug))?;
    p.save(data_dir)?;
    Ok(p)
}

/// Delete a user preset. Built-in presets cannot be deleted.
pub fn delete_user_preset(data_dir: &Path, id: &str) -> Result<()> {
    let slug = id.strip_prefix(USER_PREFIX).unwrap_or(id);
    if !id.starts_with(USER_PREFIX) && find_bar_preset(id).is_some() {
        bail!("built-in presets cannot be deleted");
    }
    let p = find_user_preset(data_dir, slug).context("no such preset")?;
    let path = p.path(data_dir);
    fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))
}

/// Whether `bar` still has the settings of the preset it last applied:
/// `None` when no known preset was applied.
pub fn preset_state(bar: &BarConfig, data_dir: &Path) -> Option<(String, bool)> {
    let id = bar.preset.clone()?;
    let same = if let Some(slug) = id.strip_prefix(USER_PREFIX) {
        find_user_preset(data_dir, slug)?.applied_to(bar) == *bar
    } else {
        find_bar_preset(&id)?.applied_to(bar) == *bar
    };
    Some((id, same))
}
