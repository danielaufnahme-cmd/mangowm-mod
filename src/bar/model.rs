//! Status bar model shared by the Mangobar and Waybar generators.
//!
//! Module names are Mangobar's config names (mangobar `rtconfig.c`); the
//! Waybar generator maps them to Waybar's names (`mango/workspaces`, ...).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::fsutil::{atomic_write, read_text};
use crate::core::values::parse_color;

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
    [
        "cpu",
        "memory",
        "backlight",
        "pulseaudio",
        "battery",
        "tray",
    ]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarConfig {
    #[serde(default)]
    pub backend: Backend,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub modules: BarModules,
    /// Colors are stored as #rrggbbaa.
    #[serde(default = "default_bg")]
    pub bg_color: String,
    #[serde(default = "default_fg")]
    pub fg_color: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_radius")]
    pub radius: u32,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_layer")]
    pub layer: String,
    /// Waybar only: use Waybar's `mango/*` modules (added to Waybar after
    /// 0.15.0). When false, workspaces use `ext/workspaces` and the other
    /// Mango-only modules are left out.
    #[serde(default = "default_true")]
    pub waybar_mango_modules: bool,
    /// Extra top-level config keys (e.g. "sys-interval", Waybar "position").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, Value>,
    /// Per-module overrides, merged over the generated defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub module_options: BTreeMap<String, Map<String, Value>>,
}

/// Kept for compatibility with mangowm-mod 1.0.
pub type Mangobar = BarConfig;

fn default_height() -> u32 {
    32
}
fn default_bg() -> String {
    "#1e1e2eeb".to_string()
}
fn default_fg() -> String {
    "#cdd6f4ff".to_string()
}
fn default_accent() -> String {
    "#89b4faff".to_string()
}
fn default_radius() -> u32 {
    10
}
fn default_font() -> String {
    "JetBrainsMono Nerd Font".to_string()
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

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            height: default_height(),
            modules: BarModules::default(),
            bg_color: default_bg(),
            fg_color: default_fg(),
            accent_color: default_accent(),
            radius: default_radius(),
            font: default_font(),
            font_size: default_font_size(),
            layer: default_layer(),
            waybar_mango_modules: true,
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

    pub fn validate(&self) -> Result<()> {
        if self.height == 0 || self.height > 500 {
            bail!("bar height must be 1-500");
        }
        if !LAYERS.contains(&self.layer.as_str()) {
            bail!("layer must be one of {}", LAYERS.join(", "));
        }
        for c in [&self.bg_color, &self.fg_color, &self.accent_color] {
            normalize_color(c)?;
        }
        if self.font.contains(['"', '\n', ';', '}']) {
            bail!("font name must not contain quotes, ';' or '}}'");
        }
        for m in self.modules.all() {
            if !is_known_module(m) {
                bail!("unknown bar module {:?}", m);
            }
        }
        Ok(())
    }

    /// Set one bar option. `key` is a bar field (height, radius, bg, fg, accent,
    /// font, font-size, layer, backend, left, center, right), a top-level config
    /// key, or `<module>.<option>`.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "height" => self.height = value.trim().parse().context("height must be a number")?,
            "radius" => self.radius = value.trim().parse().context("radius must be a number")?,
            "bg" | "bg_color" | "background" => self.bg_color = normalize_color(value)?,
            "fg" | "fg_color" | "foreground" => self.fg_color = normalize_color(value)?,
            "accent" | "accent_color" => self.accent_color = normalize_color(value)?,
            "font" => self.font = value.trim().to_string(),
            "font-size" | "font_size" => {
                self.font_size = value.trim().parse().context("font size must be a number")?
            }
            "layer" => self.layer = value.trim().to_string(),
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
                    self.module_options
                        .entry(module.to_string())
                        .or_default()
                        .insert(opt.to_string(), parse_json_value(value));
                } else {
                    if self.backend == Backend::Mangobar && !MANGOBAR_BAR_KEYS.contains(&key) {
                        bail!(
                            "unknown bar setting {:?}; use height, radius, bg, fg, accent, font, \
                             font-size, layer, backend, left, center, right, {} or <module>.<option>",
                            key,
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
