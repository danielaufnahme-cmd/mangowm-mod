//! Status bar model shared by the Mangobar and Waybar generators.
//!
//! Module names are Mangobar's config names (mangobar `rtconfig.c`); the
//! Waybar generator maps them to Waybar's names (`mango/workspaces`, ...).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::buttons::{button_id, next_id, BarButton};
use super::options::{find_opt, parse_opt_value};
use crate::core::fsutil::{atomic_write, read_text};
use crate::core::values::parse_color;

/// Keys `BarConfig::set` takes for the bar's own fields.
pub const BAR_FIELDS: &[&str] = &[
    "backend",
    "height",
    "auto-height",
    "layer",
    "position",
    "output",
    "margin-top",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "spacing",
    "font",
    "font-size",
    "font-weight",
    "radius",
    "bg",
    "fg",
    "accent",
    "module-bg",
    "hover",
    "urgent",
    "inactive",
    "border-width",
    "border-color",
    "module-padding",
    "module-margin",
    "module-gap",
    "module-radius",
    "title-length",
    "tag-fill",
    "left",
    "center",
    "right",
    "waybar-mango-modules",
    "start-at-login",
    "memory-display",
];

/// Font sizes the bar accepts.
pub const FONT_SIZE_RANGE: (u32, u32) = (6, 72);

/// What the memory module shows: (id, label). Mangobar only knows the
/// percentage (mangobar `M_MEM` formats one value), see [`memory_displays`].
pub const MEMORY_DISPLAYS: &[(&str, &str)] = &[
    ("percent", "Percent"),
    ("used", "Used GB"),
    ("used-total", "Used / total GB"),
    ("both", "Both"),
];

/// The memory displays `backend` can show.
pub fn memory_displays(backend: Backend) -> &'static [(&'static str, &'static str)] {
    match backend {
        Backend::Mangobar => &MEMORY_DISPLAYS[..1],
        Backend::Waybar => MEMORY_DISPLAYS,
    }
}

/// Space between the text line and a module's top/bottom edge.
pub const TEXT_INSET: u32 = 4;

/// Text line height (ascent + descent) of a `size` px font, rounded up.
/// fcft and Pango give 1.38-1.46 x the size for the usual UI and Nerd fonts
/// (JetBrainsMono Nerd Font 13: 14 + 5 = 19 px, measured with fcft); 1.5 x
/// covers them.
pub fn line_height(font_size: u32) -> u32 {
    (font_size * 3).div_ceil(2)
}

/// Average glyph advance, for the window title cap (JetBrainsMono: 0.6 em).
pub fn char_width(font_size: u32) -> u32 {
    (font_size * 62).div_ceil(100)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Mangobar,
    Waybar,
}

impl Backend {
    pub fn binary(self) -> &'static str {
        match self {
            Backend::Mangobar => "mangobar",
            Backend::Waybar => "waybar",
        }
    }
}

/// Built-in modules (Mangobar names). `battery#<dev>`, `clock#date`,
/// `clock#time` and `custom/<name>` are accepted as well.
pub const KNOWN_MODULES: &[(&str, &str)] = &[
    ("workspaces", "Tags 1-9 with active/occupied/urgent state"),
    ("layout", "Current layout symbol"),
    ("window", "Focused window title"),
    ("keymode", "Current Mango key mode"),
    ("keyboardlayout", "Keyboard layout"),
    ("hideclients", "Number of minimized windows (Mangobar only)"),
    ("cpu", "CPU load"),
    ("memory", "Memory use"),
    ("backlight", "Screen brightness"),
    ("pulseaudio", "Volume"),
    ("battery", "Battery level"),
    ("network", "Network status"),
    ("clock", "Clock (also clock#date, clock#time)"),
    ("tray", "System tray"),
];

/// Per-module option keys Mangobar reads (mangobar `rtconfig.c`, 2026-10).
pub const MANGOBAR_MODULE_KEYS: &[&str] = &[
    "device",
    "exec",
    "format",
    "format-alt",
    "format-disconnected",
    "format-ethernet",
    "format-muted",
    "format-wifi",
    "hide-empty",
    "hide-on-ac",
    "icon-ac",
    "icon-bluetooth",
    "icon-charging",
    "icon-discharging",
    "icon-disconnected",
    "icon-ethernet",
    "icon-full",
    "icon-muted",
    "icons",
    "icon-size",
    "icon-wifi",
    "interval",
    "max-length",
    "max-volume",
    "on-click",
    "on-click-middle",
    "on-click-right",
    "on-scroll-down",
    "on-scroll-up",
    "overview-label",
    "pinned",
    "scroll-step",
    "signal",
    "signal-icons",
    "smooth-scrolling-threshold",
    "spacing",
    "tag-names",
];

/// Top-level keys Mangobar reads besides height/layer/modules-*.
pub const MANGOBAR_BAR_KEYS: &[&str] = &[
    "buffer-scale",
    "scroll-interval",
    "sys-interval",
    "smooth-scrolling-threshold",
];

pub const LAYERS: &[&str] = &["top", "bottom", "overlay"];

/// Waybar only: Mangobar always sits at the top (mangobar `bar_create_surface`).
pub const POSITIONS: &[&str] = &["top", "bottom"];

/// Font weights; the generator writes fcft styles (Mangobar) or CSS weights.
pub const FONT_WEIGHTS: &[&str] = &["normal", "medium", "semibold", "bold"];

pub const SECTIONS: &[&str] = &["left", "center", "right"];

pub fn module_base(name: &str) -> &str {
    name.split('#').next().unwrap_or(name)
}

pub fn is_known_module(name: &str) -> bool {
    if let Some(custom) = name.strip_prefix("custom/") {
        return !custom.is_empty()
            && custom
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    }
    match name {
        "clock#date" | "clock#time" => true,
        _ if name.starts_with("battery#") => name.len() > "battery#".len(),
        _ => KNOWN_MODULES.iter().any(|(m, _)| *m == name),
    }
}

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
    ["cpu", "memory", "pulseaudio", "network", "battery", "tray"]
        .iter()
        .map(|s| s.to_string())
        .collect()
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

impl BarModules {
    pub fn all(&self) -> impl Iterator<Item = &String> {
        self.left.iter().chain(&self.center).chain(&self.right)
    }

    pub fn section(&self, section: &str) -> Result<&Vec<String>> {
        match section {
            "left" => Ok(&self.left),
            "center" => Ok(&self.center),
            "right" => Ok(&self.right),
            _ => bail!("bar section must be left, center or right"),
        }
    }

    pub fn section_mut(&mut self, section: &str) -> Result<&mut Vec<String>> {
        match section {
            "left" => Ok(&mut self.left),
            "center" => Ok(&mut self.center),
            "right" => Ok(&mut self.right),
            _ => bail!("bar section must be left, center or right"),
        }
    }

    /// Append a module to a section. A module can be in the bar only once
    /// (its options are shared).
    pub fn add(&mut self, section: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if !is_known_module(name) {
            bail!(
                "unknown bar module {:?}: run `mangowm-mod bar modules` for the list",
                name
            );
        }
        if self.all().any(|m| m == name) {
            bail!("{} is already in the bar", name);
        }
        self.section_mut(section)?.push(name.to_string());
        Ok(())
    }

    pub fn remove(&mut self, section: &str, index: usize) -> Result<String> {
        let list = self.section_mut(section)?;
        if index >= list.len() {
            bail!("no module #{} in the {} section", index + 1, section);
        }
        Ok(list.remove(index))
    }

    /// Move a module one place up (towards the start) or down; at either end
    /// it moves into the neighbouring section.
    pub fn shift(&mut self, section: &str, index: usize, up: bool) -> Result<()> {
        let pos = SECTIONS
            .iter()
            .position(|s| *s == section)
            .context("bar section must be left, center or right")?;
        let len = self.section(section)?.len();
        if index >= len {
            bail!("no module #{} in the {} section", index + 1, section);
        }
        let list = self.section_mut(section)?;
        if up && index > 0 {
            list.swap(index, index - 1);
        } else if !up && index + 1 < len {
            list.swap(index, index + 1);
        } else if up && pos > 0 {
            let m = list.remove(index);
            self.section_mut(SECTIONS[pos - 1])?.push(m);
        } else if !up && pos + 1 < SECTIONS.len() {
            let m = list.remove(index);
            self.section_mut(SECTIONS[pos + 1])?.insert(0, m);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarConfig {
    #[serde(default)]
    pub backend: Backend,
    /// Manual height; only used when `auto_height` is off, and never below
    /// [`BarConfig::min_height`].
    #[serde(default = "default_height")]
    pub height: u32,
    /// Height follows the font and module spacing ([`BarConfig::min_height`]).
    #[serde(default = "default_true")]
    pub auto_height: bool,
    #[serde(default)]
    pub modules: BarModules,
    /// Colors are stored as #rrggbbaa.
    #[serde(default = "default_bg")]
    pub bg_color: String,
    #[serde(default = "default_fg")]
    pub fg_color: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    /// Bar corner radius; -1 = pill.
    #[serde(default = "default_radius")]
    pub radius: i32,
    /// Font families, comma-separated (Mangobar uses one, see the generator).
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_font_weight")]
    pub font_weight: String,
    #[serde(default = "default_layer")]
    pub layer: String,
    /// Waybar only.
    #[serde(default = "default_position")]
    pub position: String,
    /// Gap between the bar and the screen edges. Mangobar has no bottom margin.
    #[serde(default = "default_margin_top")]
    pub margin_top: u32,
    #[serde(default)]
    pub margin_bottom: u32,
    #[serde(default = "default_margin_side")]
    pub margin_left: u32,
    #[serde(default = "default_margin_side")]
    pub margin_right: u32,
    /// Waybar `spacing`: gap between modules (Mangobar uses `module_gap`).
    #[serde(default = "default_spacing")]
    pub spacing: u32,
    /// Monitors the bar is shown on; empty = all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<String>,
    #[serde(default = "default_module_bg")]
    pub module_bg_color: String,
    #[serde(default = "default_hover")]
    pub hover_color: String,
    #[serde(default = "default_urgent")]
    pub urgent_color: String,
    /// Text of empty / inactive tags.
    #[serde(default = "default_inactive")]
    pub inactive_color: String,
    /// Horizontal padding inside a module.
    #[serde(default = "default_module_padding")]
    pub module_padding: u32,
    /// Space above and below each module.
    #[serde(default = "default_module_margin")]
    pub module_margin: u32,
    /// Space left and right of each module.
    #[serde(default = "default_module_gap")]
    pub module_gap: u32,
    /// Module corner radius; -1 = pill.
    #[serde(default = "default_module_radius")]
    pub module_radius: i32,
    /// Waybar only (Mangobar draws no bar border).
    #[serde(default = "default_border_width")]
    pub border_width: u32,
    #[serde(default = "default_border_color")]
    pub border_color: String,
    /// Window title cap in characters (Mangobar gets it in pixels); 0 = none.
    #[serde(default = "default_title_length")]
    pub title_length: u32,
    /// Active tag filled with the accent; off = accent text (Waybar adds an
    /// underline).
    #[serde(default = "default_true")]
    pub tag_fill: bool,
    /// Start this bar when Mango starts (exec_once in mango-mod.conf).
    #[serde(default = "default_true")]
    pub start_at_login: bool,
    /// Waybar only: use Waybar's `mango/*` modules (added to Waybar after
    /// 0.15.0). When false, workspaces use `ext/workspaces`, the window title
    /// a `custom/window` reading `mmsg`, and the other Mango-only modules are
    /// left out.
    #[serde(default)]
    pub waybar_mango_modules: bool,
    /// Memory module text, one of [`MEMORY_DISPLAYS`] (a `memory.format`
    /// override wins).
    #[serde(default = "default_memory_display")]
    pub memory_display: String,
    /// The preset last applied: a built-in id or `user:<slug>`. The Bar page
    /// shows "Modified" once the settings differ from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Custom buttons; each is the module `custom/btn-<id>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<BarButton>,
    /// Extra top-level config keys (e.g. "sys-interval", Waybar "position").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, Value>,
    /// Per-module overrides, merged over the generated defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub module_options: BTreeMap<String, Map<String, Value>>,
}

/// Kept for compatibility with mangowm-mod 1.0.
pub type Mangobar = BarConfig;

// Defaults are the "Islands" bar preset (src/bar/presets.rs; a test keeps
// them in step).
fn default_height() -> u32 {
    28
}
fn default_bg() -> String {
    "#00000000".to_string()
}
fn default_fg() -> String {
    "#e6e8ebff".to_string()
}
fn default_accent() -> String {
    "#76ccb1ff".to_string()
}
fn default_radius() -> i32 {
    14
}
fn default_font() -> String {
    "Adwaita Sans, JetBrainsMono Nerd Font".to_string()
}
fn default_font_weight() -> String {
    "medium".to_string()
}
fn default_position() -> String {
    "top".to_string()
}
fn default_margin_top() -> u32 {
    6
}
fn default_margin_side() -> u32 {
    8
}
fn default_spacing() -> u32 {
    0
}
fn default_module_bg() -> String {
    "#16181ce6".to_string()
}
fn default_hover() -> String {
    "#2a2e35ff".to_string()
}
fn default_urgent() -> String {
    "#e5737aff".to_string()
}
fn default_inactive() -> String {
    "#7c828cff".to_string()
}
fn default_module_padding() -> u32 {
    12
}
fn default_module_margin() -> u32 {
    0
}
fn default_module_gap() -> u32 {
    3
}
fn default_module_radius() -> i32 {
    10
}
fn default_border_width() -> u32 {
    0
}
fn default_border_color() -> String {
    "#ffffff14".to_string()
}
fn default_title_length() -> u32 {
    48
}
fn default_font_size() -> u32 {
    13
}
fn default_layer() -> String {
    "top".to_string()
}
fn default_true() -> bool {
    true
}
fn default_memory_display() -> String {
    "percent".to_string()
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            height: default_height(),
            auto_height: true,
            modules: BarModules::default(),
            bg_color: default_bg(),
            fg_color: default_fg(),
            accent_color: default_accent(),
            radius: default_radius(),
            font: default_font(),
            font_size: default_font_size(),
            font_weight: default_font_weight(),
            layer: default_layer(),
            position: default_position(),
            margin_top: default_margin_top(),
            margin_bottom: 0,
            margin_left: default_margin_side(),
            margin_right: default_margin_side(),
            spacing: default_spacing(),
            output: Vec::new(),
            module_bg_color: default_module_bg(),
            hover_color: default_hover(),
            urgent_color: default_urgent(),
            inactive_color: default_inactive(),
            module_padding: default_module_padding(),
            module_margin: default_module_margin(),
            module_gap: default_module_gap(),
            module_radius: default_module_radius(),
            border_width: default_border_width(),
            border_color: default_border_color(),
            title_length: default_title_length(),
            tag_fill: true,
            start_at_login: true,
            waybar_mango_modules: false,
            memory_display: default_memory_display(),
            preset: None,
            buttons: Vec::new(),
            options: BTreeMap::new(),
            module_options: BTreeMap::new(),
        }
    }
}

/// Parse a comma-separated module list ("workspaces,layout"); "" means none.
pub fn parse_module_list(text: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for m in text.split(',').map(str::trim).filter(|m| !m.is_empty()) {
        if !is_known_module(m) {
            bail!(
                "unknown bar module {:?}: run `mangowm-mod bar modules` for the list",
                m
            );
        }
        out.push(m.to_string());
    }
    Ok(out)
}

/// Normalize any accepted color (#rgb not included) to #rrggbbaa.
pub fn normalize_color(text: &str) -> Result<String> {
    let c = parse_color(text).with_context(|| format!("invalid color {:?}", text))?;
    Ok(if c.len() == 7 { format!("{}ff", c) } else { c })
}

/// Parse a CLI value as JSON when possible (numbers, true/false, arrays), else a string.
pub fn parse_json_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

impl BarConfig {
    pub fn default_path(data_dir: &Path) -> PathBuf {
        data_dir.join("bar.json")
    }

    /// Load saved bar choices; defaults when the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = read_text(path)?;
        serde_json::from_str(&text)
            .with_context(|| format!("invalid bar settings in {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_write(path, &(serde_json::to_string_pretty(self)? + "\n"))
    }

    /// Height a module needs: the text line plus [`TEXT_INSET`] above and below.
    pub fn module_min_height(&self) -> u32 {
        line_height(self.font_size) + 2 * TEXT_INSET
    }

    /// Smallest bar height that fits the modules. Mangobar draws every module
    /// at the bar's full height and has no vertical module margin (mangobar
    /// `paint_module`); in Waybar the module margins and the border sit
    /// inside the bar's height.
    pub fn min_height(&self) -> u32 {
        match self.backend {
            Backend::Mangobar => self.module_min_height(),
            Backend::Waybar => {
                self.module_min_height() + 2 * self.module_margin + 2 * self.border_width
            }
        }
    }

    /// Height written to the config: automatic, or the manual height raised
    /// to [`Self::min_height`].
    pub fn effective_height(&self) -> u32 {
        if self.auto_height {
            self.min_height()
        } else {
            self.height.max(self.min_height())
        }
    }

    /// Height of a module in the running bar.
    pub fn module_height(&self) -> u32 {
        let h = self.effective_height();
        match self.backend {
            Backend::Mangobar => h,
            Backend::Waybar => h - 2 * self.module_margin - 2 * self.border_width,
        }
    }

    /// The window title cap in pixels (Mangobar's `max-length` unit).
    pub fn title_max_px(&self) -> u32 {
        self.title_length * char_width(self.font_size)
    }

    pub fn validate(&self) -> Result<()> {
        if self.height == 0 || self.height > 500 {
            bail!("bar height must be 1-500");
        }
        let (lo, hi) = FONT_SIZE_RANGE;
        if self.font_size < lo || self.font_size > hi {
            bail!("font size must be {}-{}", lo, hi);
        }
        if !MEMORY_DISPLAYS
            .iter()
            .any(|(d, _)| *d == self.memory_display)
        {
            bail!(
                "memory display must be one of {}",
                MEMORY_DISPLAYS
                    .iter()
                    .map(|(d, _)| *d)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for (i, b) in self.buttons.iter().enumerate() {
            b.validate()?;
            if self.buttons[..i].iter().any(|o| o.id == b.id) {
                bail!("two buttons have the id {:?}", b.id);
            }
        }
        if self.title_length > 500 {
            bail!("title length must be 0-500 characters");
        }
        if !LAYERS.contains(&self.layer.as_str()) {
            bail!("layer must be one of {}", LAYERS.join(", "));
        }
        if !POSITIONS.contains(&self.position.as_str()) {
            bail!("position must be one of {}", POSITIONS.join(", "));
        }
        if !FONT_WEIGHTS.contains(&self.font_weight.as_str()) {
            bail!("font weight must be one of {}", FONT_WEIGHTS.join(", "));
        }
        if self.radius < -1 || self.module_radius < -1 {
            bail!("corner radius must be 0 or more (-1 = pill)");
        }
        for c in self.colors() {
            normalize_color(c)?;
        }
        if self.font.contains(['"', '\'', '\n', ';', '}', '{']) {
            bail!("font name must not contain quotes, ';' or braces");
        }
        for o in &self.output {
            if o.is_empty() || o.contains(['"', '\n', ',']) {
                bail!("invalid output name {:?}", o);
            }
        }
        for m in self.modules.all() {
            if !is_known_module(m) {
                bail!("unknown bar module {:?}", m);
            }
        }
        Ok(())
    }

    fn colors(&self) -> [&String; 8] {
        [
            &self.bg_color,
            &self.fg_color,
            &self.accent_color,
            &self.module_bg_color,
            &self.hover_color,
            &self.urgent_color,
            &self.inactive_color,
            &self.border_color,
        ]
    }

    /// Set one bar option. `key` is a bar field (see [`BAR_FIELDS`]), a
    /// top-level config key, or `<module>.<option>`. Nothing changes on error.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let before = self.clone();
        let res = self.set_unchecked(key, value);
        if res.is_err() {
            *self = before;
        }
        res
    }

    fn set_unchecked(&mut self, key: &str, value: &str) -> Result<()> {
        let num = |what: &str| -> Result<u32> {
            value
                .trim()
                .parse()
                .with_context(|| format!("{} must be a whole number of 0 or more", what))
        };
        let radius = || -> Result<i32> {
            match value.trim() {
                "pill" => Ok(-1),
                v => v
                    .parse()
                    .context("radius must be a number (-1 or pill = pill)"),
            }
        };
        match key {
            "height" => {
                // a height typed in means a manual height
                self.height = num("height")?;
                self.auto_height = false;
            }
            "auto-height" => {
                self.auto_height = crate::core::values::parse_bool(value)
                    .context("auto-height must be true or false")?
            }
            "title-length" => self.title_length = num("title length")?,
            "tag-fill" => {
                self.tag_fill = crate::core::values::parse_bool(value)
                    .context("tag-fill must be true or false")?
            }
            "radius" => self.radius = radius()?,
            "bg" | "bg_color" | "background" => self.bg_color = normalize_color(value)?,
            "fg" | "fg_color" | "foreground" => self.fg_color = normalize_color(value)?,
            "accent" | "accent_color" => self.accent_color = normalize_color(value)?,
            "module-bg" => self.module_bg_color = normalize_color(value)?,
            "hover" => self.hover_color = normalize_color(value)?,
            "urgent" => self.urgent_color = normalize_color(value)?,
            "inactive" => self.inactive_color = normalize_color(value)?,
            "border-color" => self.border_color = normalize_color(value)?,
            "border-width" => self.border_width = num("border width")?,
            "font" => self.font = value.trim().to_string(),
            "font-size" | "font_size" => self.font_size = num("font size")?,
            "font-weight" => self.font_weight = value.trim().to_string(),
            "layer" => self.layer = value.trim().to_string(),
            "position" => self.position = value.trim().to_string(),
            "margin-top" => self.margin_top = num("margin")?,
            "margin-bottom" => self.margin_bottom = num("margin")?,
            "margin-left" => self.margin_left = num("margin")?,
            "margin-right" => self.margin_right = num("margin")?,
            "spacing" => self.spacing = num("spacing")?,
            "module-padding" => self.module_padding = num("module padding")?,
            "module-margin" => self.module_margin = num("module margin")?,
            "module-gap" => self.module_gap = num("module gap")?,
            "module-radius" => self.module_radius = radius()?,
            "output" => {
                self.output = value
                    .split(',')
                    .map(str::trim)
                    .filter(|o| !o.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            "start-at-login" => {
                self.start_at_login = crate::core::values::parse_bool(value)
                    .context("start-at-login must be true or false")?
            }
            "backend" => {
                self.backend = match value.trim() {
                    "mangobar" => Backend::Mangobar,
                    "waybar" => Backend::Waybar,
                    _ => bail!("backend must be mangobar or waybar"),
                }
            }
            "left" => self.modules.left = parse_module_list(value)?,
            "center" => self.modules.center = parse_module_list(value)?,
            "right" => self.modules.right = parse_module_list(value)?,
            "memory-display" => self.memory_display = value.trim().to_string(),
            "waybar-mango-modules" => {
                self.waybar_mango_modules = crate::core::values::parse_bool(value)
                    .context("waybar-mango-modules must be true or false")?
            }
            _ => {
                if let Some((module, opt)) = key.split_once('.') {
                    if !is_known_module(module) {
                        bail!("unknown bar module {:?}", module);
                    }
                    if self.backend == Backend::Mangobar && !MANGOBAR_MODULE_KEYS.contains(&opt) {
                        bail!(
                            "Mangobar has no module option {:?}; known: {}",
                            opt,
                            MANGOBAR_MODULE_KEYS.join(", ")
                        );
                    }
                    // known options get their type, so a format "{}" stays a string
                    let parsed = match find_opt(module, opt) {
                        Some(o) => parse_opt_value(o.kind, value)?,
                        None => parse_json_value(value),
                    };
                    self.module_options
                        .entry(module.to_string())
                        .or_default()
                        .insert(opt.to_string(), parsed);
                } else {
                    if self.backend == Backend::Mangobar && !MANGOBAR_BAR_KEYS.contains(&key) {
                        bail!(
                            "unknown bar setting {:?}; use {}, {} or <module>.<option>",
                            key,
                            BAR_FIELDS.join(", "),
                            MANGOBAR_BAR_KEYS.join(", ")
                        );
                    }
                    self.options
                        .insert(key.to_string(), parse_json_value(value));
                }
            }
        }
        self.validate()
    }

    /// Remove a top-level or `<module>.<option>` override. Returns true if something was removed.
    pub fn unset(&mut self, key: &str) -> bool {
        if let Some((module, opt)) = key.split_once('.') {
            let removed = self
                .module_options
                .get_mut(module)
                .map(|m| m.remove(opt).is_some())
                .unwrap_or(false);
            if self
                .module_options
                .get(module)
                .is_some_and(|m| m.is_empty())
            {
                self.module_options.remove(module);
            }
            removed
        } else {
            self.options.remove(key).is_some()
        }
    }
}

impl BarConfig {
    pub fn button(&self, id: &str) -> Option<&BarButton> {
        self.buttons.iter().find(|b| b.id == id)
    }

    pub fn button_mut(&mut self, id: &str) -> Option<&mut BarButton> {
        self.buttons.iter_mut().find(|b| b.id == id)
    }

    /// The button of a `custom/btn-<id>` module.
    pub fn button_for_module(&self, module: &str) -> Option<&BarButton> {
        button_id(module).and_then(|id| self.button(id))
    }

    /// Add a button (a new id is given when its id is empty or taken) at
    /// the end of `section`. Returns the id.
    pub fn add_button(&mut self, mut button: BarButton, section: &str) -> Result<String> {
        if button.id.is_empty() || self.button(&button.id).is_some() {
            button.id = next_id(&self.buttons);
        }
        button.validate()?;
        let module = button.module();
        if self.modules.all().any(|m| *m == module) {
            bail!("{} is already in the bar", module);
        }
        self.modules.section_mut(section)?.push(module);
        let id = button.id.clone();
        self.buttons.push(button);
        Ok(id)
    }

    /// Remove a button and its module. False when there is no such button.
    pub fn remove_button(&mut self, id: &str) -> bool {
        let Some(pos) = self.buttons.iter().position(|b| b.id == id) else {
            return false;
        };
        let module = self.buttons.remove(pos).module();
        for list in [
            &mut self.modules.left,
            &mut self.modules.center,
            &mut self.modules.right,
        ] {
            list.retain(|m| *m != module);
        }
        self.module_options.remove(&module);
        true
    }

    /// Section the button's module is in; `None` when it is not in the bar.
    pub fn button_section(&self, id: &str) -> Option<&'static str> {
        let module = self.button(id)?.module();
        SECTIONS
            .iter()
            .copied()
            .find(|s| self.modules.section(s).is_ok_and(|l| l.contains(&module)))
    }

    /// Move the button to the end of `section`, or out of the bar (`None`).
    pub fn set_button_section(&mut self, id: &str, section: Option<&str>) -> Result<()> {
        let module = self.button(id).context("no such button")?.module();
        if section.is_some() && self.button_section(id) == section {
            return Ok(());
        }
        for list in [
            &mut self.modules.left,
            &mut self.modules.center,
            &mut self.modules.right,
        ] {
            list.retain(|m| *m != module);
        }
        if let Some(s) = section {
            self.modules.section_mut(s)?.push(module);
        }
        Ok(())
    }
}
