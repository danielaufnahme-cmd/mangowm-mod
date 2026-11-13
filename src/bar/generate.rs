//! Config and style generator for Mangobar and Waybar.
//!
//! Mangobar: `config.jsonc` keys and CSS ids from mangobar `rtconfig.c` /
//! `style.c` / `mangobar.c` (hex colors only; 8 digits are read as
//! #rrggbbaa by `parse_hex_color`, although the wiki says #AARRGGBB).
//! Waybar: `mango/*` modules (Waybar master), GTK3 CSS with rgba() colors. Both write `config.jsonc` + `style.css`.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::buttons::{click_command, BarButton, Click};
use super::model::{memory_displays, module_base, Backend, BarConfig, MANGOBAR_MODULE_KEYS};
use super::options::find_opt;
use crate::core::fsutil::atomic_write;

pub const CONFIG_FILE: &str = "config.jsonc";
pub const STYLE_FILE: &str = "style.css";

/// Output directory: `$MANGOBAR_MOD_DIR` / `$WAYBAR_MOD_DIR`, else
/// `<config dir>/<bar>/mango-mod` (never the user's own bar config).
pub fn bar_dir(backend: Backend) -> PathBuf {
    let env = match backend {
        Backend::Mangobar => "MANGOBAR_MOD_DIR",
        Backend::Waybar => "WAYBAR_MOD_DIR",
    };
    if let Some(dir) = std::env::var_os(env) {
        return PathBuf::from(dir);
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config"));
    base.join(backend.binary()).join("mango-mod")
}

pub fn mangobar_dir() -> PathBuf {
    bar_dir(Backend::Mangobar)
}

/// Window title for Waybar without the mango/* modules: a custom module
/// that follows Mango's focused client over IPC (as ~/.config/waybar did).
pub const WAYBAR_WINDOW_MODULE: &str = "custom/mango-window";
const WAYBAR_WINDOW_EXEC: &str = "mmsg watch focusing-client | jq --unbuffered -r '.title // \"\"'";

/// Waybar name of a module, or `None` when this Waybar setup cannot show it.
pub fn waybar_module_name(name: &str, mango_modules: bool) -> Option<String> {
    let mapped = match name {
        "workspaces" if mango_modules => "mango/workspaces",
        "workspaces" => "ext/workspaces",
        "window" | "layout" | "keymode" if mango_modules => {
            return Some(format!("mango/{}", name));
        }
        "keyboardlayout" if mango_modules => "mango/language",
        "window" => WAYBAR_WINDOW_MODULE,
        "layout" | "keymode" | "keyboardlayout" | "hideclients" => return None,
        _ => return Some(name.to_string()),
    };
    Some(mapped.to_string())
}

/// Waybar name of a module of this bar: a picture button becomes an
/// `image#btn-<id>` module, the rest as [`waybar_module_name`].
fn waybar_name(bar: &BarConfig, name: &str) -> Option<String> {
    match bar.button_for_module(name) {
        Some(b) if !b.picture.is_empty() => Some(format!("image#btn-{}", b.id)),
        _ => waybar_module_name(name, bar.waybar_mango_modules),
    }
}

/// Escape text for Waybar's Pango markup.
fn markup_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Waybar's memory format for a [`super::model::MEMORY_DISPLAYS`] id.
pub fn waybar_memory_format(display: &str) -> &'static str {
    match display {
        "used" => "󰘚 {used:0.1f} GB",
        "used-total" => "󰘚 {used:0.1f}/{total:0.0f} GB",
        "both" => "󰘚 {used:0.1f} GB · {percentage}%",
        _ => "󰘚 {percentage}%",
    }
}

/// Config of a custom button: its icon and the two click helpers.
fn button_module(bar: &BarConfig, b: &BarButton) -> Value {
    let left = click_command(None, &b.id, Click::Left);
    let right = click_command(None, &b.id, Click::Right);
    match bar.backend {
        // no tooltips and no pictures in Mangobar: emoji, else the label
        Backend::Mangobar => json!({"format": b.text(), "on-click": left, "on-click-right": right}),
        Backend::Waybar if !b.picture.is_empty() => {
            let mut o = json!({
                "size": bar.font_size * 4 / 3,
                "on-click": left,
                "on-click-right": right
            });
            if b.label.is_empty() {
                o["path"] = json!(b.picture);
                o["tooltip"] = json!(false);
            } else {
                // the image module takes a tooltip only from its exec output
                o["exec"] = json!(format!(
                    "printf '%s\\n%s\\n' {} {}",
                    super::login::shell_quote(&b.picture),
                    super::login::shell_quote(&b.label)
                ));
            }
            o
        }
        Backend::Waybar => {
            let mut o = json!({
                "format": markup_escape(b.text()),
                "on-click": left,
                "on-click-right": right
            });
            if b.label.is_empty() || b.emoji.is_empty() {
                o["tooltip"] = json!(false);
            } else {
                o["tooltip-format"] = json!(markup_escape(&b.label));
            }
            o
        }
    }
}

/// Notes about modules the selected backend cannot show (or may not have).
pub fn warnings(bar: &BarConfig) -> Vec<String> {
    let mut out = Vec::new();
    if !memory_displays(bar.backend)
        .iter()
        .any(|(d, _)| *d == bar.memory_display)
        && bar.modules.all().any(|m| m == "memory")
    {
        out.push(format!(
            "{} shows memory only as a percentage (memory display {:?} is for Waybar)",
            bar.backend.binary(),
            bar.memory_display
        ));
    }
    if bar.backend != Backend::Waybar {
        for b in &bar.buttons {
            if !b.picture.is_empty() && bar.button_section(&b.id).is_some() {
                out.push(format!(
                    "button {}: Mangobar cannot draw pictures; it shows {:?} instead (pictures need Waybar)",
                    b.id,
                    b.text()
                ));
            }
        }
        return out;
    }
    let mut seen = Vec::new();
    for m in bar.modules.all() {
        if seen.contains(m) {
            continue;
        }
        seen.push(m.clone());
        if m == "hideclients" {
            out.push("hideclients has no Waybar equivalent; it was left out".to_string());
        } else if waybar_module_name(m, bar.waybar_mango_modules).is_none() {
            out.push(format!(
                "{} needs Waybar's mango/* modules (waybar-mango-modules=true); it was left out",
                m
            ));
        } else if m == "workspaces" && !bar.waybar_mango_modules {
            out.push("workspaces uses Waybar's ext/workspaces module".to_string());
        } else if m == "window" && !bar.waybar_mango_modules {
            out.push(format!(
                "window uses {} (reads `mmsg watch`, needs jq)",
                WAYBAR_WINDOW_MODULE
            ));
        }
    }
    if bar.waybar_mango_modules
        && bar.modules.all().any(|m| {
            matches!(
                m.as_str(),
                "workspaces" | "window" | "layout" | "keymode" | "keyboardlayout"
            )
        })
    {
        out.push(
            "Waybar's mango/* modules are newer than Waybar 0.15.0; if your Waybar does not show \
             them, run `mangowm-mod bar set waybar-mango-modules false`"
                .to_string(),
        );
    }
    out
}

/// Generated options for one module (before the user's `module_options`).
fn module_defaults(bar: &BarConfig, name: &str) -> Value {
    match bar.backend {
        Backend::Mangobar => match name {
            "workspaces" => {
                json!({"hide-empty": true, "on-click": "activate", "on-click-right": "toggle"})
            }
            "layout" => json!({"format": "{}"}),
            // max-length is in pixels in Mangobar (mangobar `module_max_length`)
            "window" if bar.title_length > 0 => {
                json!({"format": "{}", "max-length": bar.title_max_px()})
            }
            "window" => json!({"format": "{}"}),
            "keyboardlayout" => json!({"format": "{short}"}),
            "hideclients" => json!({"format": " {}"}),
            // {load} is the load average, not a percentage
            "cpu" => json!({"format": "󰍛 {usage}%", "interval": 3}),
            // Mangobar's memory module has only the percentage
            "memory" => json!({"format": "󰘚 {}%", "interval": 5}),
            "backlight" => json!({
                "format": "{icon} {percent}%",
                "icons": ["󰃚", "󰃛", "󰃜", "󰃝", "󰃞", "󰃟"]
            }),
            "pulseaudio" => json!({
                "format": "{icon} {volume}%",
                "format-muted": "󰝟 muted",
                "icons": ["󰕿", "󰖀", "󰕾"],
                "scroll-step": 5
            }),
            "network" => json!({
                "format": "{icon} {ifname}",
                "format-wifi": "{signal} {ssid}",
                "format-disconnected": "{icon}",
                "icon-wifi": "󰖩",
                "icon-ethernet": "󰈀",
                "icon-disconnected": "󰤭",
                "signal-icons": ["󰤯", "󰤟", "󰤢", "󰤥", "󰤨"]
            }),
            "clock" => json!({"format": "{:L%a %d %b  %H:%M}"}),
            "clock#time" => json!({"format": " {:L%H:%M}"}),
            "clock#date" => json!({"format": " {:L%a %d %b}"}),
            "tray" => json!({"icon-size": 16, "spacing": 8}),
            _ if module_base(name) == "battery" => json!({
                "format": "{icon} {percent}%",
                "icons": ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"]
            }),
            _ => json!({}),
        },
        Backend::Waybar => match name {
            "workspaces" if !bar.waybar_mango_modules => json!({
                "format": "{name}",
                "on-click": "activate",
                "sort-by-id": true,
                "ignore-hidden": true
            }),
            "window" if !bar.waybar_mango_modules => {
                let mut o = json!({
                    "exec": WAYBAR_WINDOW_EXEC,
                    "restart-interval": 3,
                    "escape": true,
                    "hide-empty-text": true,
                    "tooltip": false
                });
                if bar.title_length > 0 {
                    o["max-length"] = json!(bar.title_length);
                }
                o
            }
            "window" if bar.title_length > 0 => json!({"max-length": bar.title_length}),
            "cpu" => json!({"format": "󰍛 {usage}%", "interval": 3, "tooltip": false}),
            "memory" => json!({
                "format": waybar_memory_format(&bar.memory_display),
                "interval": 5,
                "tooltip-format": "{used:0.1f} / {total:0.1f} GiB"
            }),
            "backlight" => json!({
                "format": "{icon} {percent}%",
                "format-icons": ["󰃚", "󰃛", "󰃜", "󰃝", "󰃞", "󰃟"]
            }),
            "pulseaudio" => json!({
                "format": "{icon} {volume}%",
                "format-muted": "󰝟 muted",
                "format-icons": {"headphone": "󰋋", "default": ["󰕿", "󰖀", "󰕾"]},
                "scroll-step": 5,
                "tooltip-format": "{desc}"
            }),
            "network" => json!({
                "format-wifi": "󰤨 {essid}",
                "format-ethernet": "󰈀",
                "format-linked": "󰈀 no IP",
                "format-disconnected": "󰤭",
                "tooltip-format": "{ifname}  {ipaddr}/{cidr}",
                "tooltip-format-wifi": "{essid} ({signalStrength}%)  {ipaddr}"
            }),
            "clock" => json!({
                "format": "{:%a %d %b  %H:%M}",
                "tooltip-format": "<tt>{calendar}</tt>"
            }),
            "clock#time" => json!({"format": " {:%H:%M}"}),
            "clock#date" => json!({"format": " {:%a %d %b}"}),
            "tray" => json!({"icon-size": 16, "spacing": 8}),
            _ if module_base(name) == "battery" => json!({
                "format": "{icon} {capacity}%",
                "format-icons": ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"]
            }),
            _ => json!({}),
        },
    }
}

/// The user's options for module `m`, renamed for the backend; options the
/// backend does not have are left out.
fn module_overrides(bar: &BarConfig, m: &str) -> Vec<(String, Value)> {
    let Some(over) = bar.module_options.get(m) else {
        return Vec::new();
    };
    over.iter()
        .filter_map(|(k, v)| {
            let key = match (find_opt(m, k), bar.backend) {
                (Some(o), be) => o.key_for(be)?.to_string(),
                (None, Backend::Mangobar) if !MANGOBAR_MODULE_KEYS.contains(&k.as_str()) => {
                    return None
                }
                (None, _) => k.clone(),
            };
            Some((key, v.clone()))
        })
        .collect()
}

/// The user's options for a module as the backend's config writes them
/// (generated default first, then the override). For the GUI.
pub fn module_option_value(bar: &BarConfig, module: &str, key: &str) -> Option<Value> {
    if let Some(v) = bar.module_options.get(module).and_then(|o| o.get(key)) {
        return Some(v.clone());
    }
    let opt = find_opt(module, key)?;
    let backend_key = opt.key_for(bar.backend)?;
    module_defaults(bar, module).get(backend_key).cloned()
}

/// Module names as the backend's config lists them (unsupported ones dropped).
fn backend_list(bar: &BarConfig, list: &[String]) -> Vec<String> {
    match bar.backend {
        Backend::Mangobar => list.to_vec(),
        Backend::Waybar => list.iter().filter_map(|m| waybar_name(bar, m)).collect(),
    }
}

pub fn generate_config(bar: &BarConfig) -> String {
    let mut map = Map::new();
    map.insert("layer".into(), json!(bar.layer));
    if bar.backend == Backend::Waybar {
        map.insert("position".into(), json!(bar.position));
    }
    map.insert("height".into(), json!(bar.effective_height()));
    if !bar.output.is_empty() {
        map.insert("output".into(), json!(bar.output));
    }
    if bar.backend == Backend::Waybar {
        // Mangobar takes its margins from the #bar CSS rule
        map.insert("margin-top".into(), json!(bar.margin_top));
        map.insert("margin-bottom".into(), json!(bar.margin_bottom));
        map.insert("margin-left".into(), json!(bar.margin_left));
        map.insert("margin-right".into(), json!(bar.margin_right));
        map.insert("spacing".into(), json!(bar.spacing));
        map.insert("reload_style_on_change".into(), json!(true));
    }
    map.insert(
        "modules-left".into(),
        json!(backend_list(bar, &bar.modules.left)),
    );
    map.insert(
        "modules-center".into(),
        json!(backend_list(bar, &bar.modules.center)),
    );
    map.insert(
        "modules-right".into(),
        json!(backend_list(bar, &bar.modules.right)),
    );

    for m in bar.modules.all() {
        let name = match bar.backend {
            Backend::Mangobar => m.clone(),
            Backend::Waybar => match waybar_name(bar, m) {
                Some(n) => n,
                None => continue,
            },
        };
        let defaults = match bar.button_for_module(m) {
            Some(b) => button_module(bar, b),
            None => module_defaults(bar, m),
        };
        let mut obj = match defaults {
            Value::Object(o) => o,
            _ => Map::new(),
        };
        for (k, v) in module_overrides(bar, m) {
            obj.insert(k, v);
        }
        if !obj.is_empty() {
            map.insert(name, Value::Object(obj));
        }
    }

    for (k, v) in &bar.options {
        map.insert(k.clone(), v.clone());
    }

    let backend = match bar.backend {
        Backend::Mangobar => "Mangobar",
        Backend::Waybar => "Waybar",
    };
    let body = serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default();
    format!("// Generated by MangoWM Mod for {}\n{}\n", backend, body)
}

/// Parse a stored #rrggbbaa color into bytes.
fn rgba(color: &str) -> (u8, u8, u8, u8) {
    let hex = color.trim_start_matches('#');
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(255)
    };
    (byte(0), byte(2), byte(4), byte(6))
}

/// `top` drawn over `bottom` (both #rrggbbaa), as #rrggbbaa.
pub fn blend_over(top: &str, bottom: &str) -> String {
    let (tr, tg, tb, ta) = rgba(top);
    let (br, bg, bb, ba) = rgba(bottom);
    let (ta, ba) = (ta as f64 / 255.0, ba as f64 / 255.0);
    let a = ta + ba * (1.0 - ta);
    if a <= 0.0 {
        return "#00000000".to_string();
    }
    let mix = |t: u8, b: u8| {
        ((t as f64 * ta + b as f64 * ba * (1.0 - ta)) / a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        mix(tr, br),
        mix(tg, bg),
        mix(tb, bb),
        (a * 255.0).round() as u8
    )
}

pub fn alpha(color: &str) -> u8 {
    rgba(color).3
}

/// Opaque color for menus, tooltips and the text on the accent: the bar
/// background, else the module background (a transparent bar), else a dark grey.
pub fn surface_color(bar: &BarConfig) -> String {
    let pick = [&bar.bg_color, &bar.module_bg_color]
        .into_iter()
        .find(|c| alpha(c) > 0)
        .map(|c| c.as_str())
        .unwrap_or("#16181cff");
    let (r, g, b, _) = rgba(pick);
    format!("#{:02x}{:02x}{:02x}ff", r, g, b)
}

/// Mangobar paints `#bar` only between the module groups, as a plain
/// rectangle, and each module only under itself, at the bar's full height
/// (mangobar `draw_bar`, `paint_module`). A bar with a visible background is
/// made continuous: every module paints the bar background (with its chip
/// color on top) and has no side margins, so the bar has square ends.
pub fn mangobar_continuous(bar: &BarConfig) -> bool {
    bar.backend == Backend::Mangobar && alpha(&bar.bg_color) > 0
}

/// Color in the backend's CSS syntax, with the alpha replaced when given.
fn css_color(backend: Backend, color: &str, alpha: Option<u8>) -> String {
    let (r, g, b, a) = rgba(color);
    let a = alpha.unwrap_or(a);
    match backend {
        Backend::Mangobar => format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a),
        Backend::Waybar => {
            let fa = crate::core::values::format_float(a as f64 / 255.0);
            format!("rgba({}, {}, {}, {})", r, g, b, fa)
        }
    }
}

/// Mangobar fills the bar's middle (#bar) with pixman, which takes the color
/// as premultiplied, while modules are drawn with cairo from straight alpha
/// (mangobar `draw_bar`, `paint_module`). The #bar color is premultiplied
/// here so a see-through bar has the same shade under and between modules.
pub fn premultiplied(color: &str) -> String {
    let (r, g, b, a) = rgba(color);
    let m = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", m(r), m(g), m(b), a)
}

/// CSS radius; a pill (-1) is a large radius. Mangobar's CSS parser drops
/// the minus sign (`-1` becomes 1 px, mangobar `parse_int`) and clamps the
/// radius to half the height (`module_radius`); GTK clamps it too.
fn css_radius(radius: i32) -> String {
    if radius < 0 {
        "999px".to_string()
    } else {
        format!("{}px", radius)
    }
}

/// Workspace tags: (side padding, side margin). The tags sit close together
/// as one group: 1 px each side (2 px between) where modules have a gap.
pub fn tag_spacing(bar: &BarConfig) -> (u32, u32) {
    let pad = (bar.module_padding * 2 / 3).max(4);
    let gap = if mangobar_continuous(bar) || bar.module_gap == 0 {
        0
    } else {
        1
    };
    (pad, gap)
}

/// Waybar: how far the tag buttons sit inside the workspaces island, so the
/// active pill has a frame of the island around it. None on square modules
/// (a solid block) or an island without a color.
fn waybar_tag_inset(bar: &BarConfig) -> u32 {
    if bar.module_radius == 0 || alpha(&bar.module_bg_color) == 0 {
        0
    } else {
        3
    }
}

/// Fast color change on hover and tag switches (Waybar; Mangobar has no
/// transitions).
const TRANSITION: &str = "120ms ease-out";

/// `font-family` value. Mangobar reads only one family (mangobar `style.c`),
/// so it gets the first Nerd Font in the list (for the icons), else the first.
fn css_font(backend: Backend, font: &str) -> String {
    let families: Vec<&str> = font
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    match backend {
        Backend::Mangobar => {
            let pick = families
                .iter()
                .find(|f| f.contains("Nerd"))
                .or(families.first())
                .copied()
                .unwrap_or("monospace");
            format!("\"{}\"", pick)
        }
        Backend::Waybar => families
            .iter()
            .map(|f| format!("\"{}\"", f))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Mangobar passes the weight to fcft as a font style; Waybar is GTK CSS.
fn css_weight(backend: Backend, weight: &str) -> &'static str {
    match (backend, weight) {
        (Backend::Mangobar, "medium") => "Medium",
        (Backend::Mangobar, "semibold") => "SemiBold",
        (Backend::Mangobar, "bold") => "Bold",
        (Backend::Mangobar, _) => "Regular",
        (Backend::Waybar, "medium") => "500",
        (Backend::Waybar, "semibold") => "600",
        (Backend::Waybar, "bold") => "bold",
        (Backend::Waybar, _) => "normal",
    }
}

/// CSS selector for a module (Mangobar ids from mangobar `style.c`).
fn css_id(backend: Backend, name: &str, mango_modules: bool) -> Option<String> {
    if let Some(custom) = name.strip_prefix("custom/") {
        return Some(format!("#custom-{}", custom));
    }
    let base = module_base(name);
    let id = match backend {
        Backend::Mangobar => match base {
            "workspaces" => "tags",
            "window" => "title",
            "memory" => "mem",
            "backlight" => "brightness",
            "pulseaudio" => "volume",
            other => other,
        },
        Backend::Waybar => {
            let mapped = waybar_module_name(base, mango_modules)?;
            if let Some(custom) = mapped.strip_prefix("custom/") {
                return Some(format!("#custom-{}", custom));
            }
            match mapped.as_str() {
                "mango/workspaces" | "ext/workspaces" => "workspaces",
                "mango/language" => "language",
                m => return Some(format!("#{}", m.trim_start_matches("mango/"))),
            }
        }
    };
    Some(format!("#{}", id))
}

pub fn generate_style(bar: &BarConfig) -> String {
    let be = bar.backend;
    let continuous = mangobar_continuous(bar);
    // Mangobar's continuous bar: the colors modules paint over nothing
    let on_bar = |c: &str| {
        if continuous {
            blend_over(c, &bar.bg_color)
        } else {
            c.to_string()
        }
    };
    let bg = match be {
        Backend::Mangobar => premultiplied(&bar.bg_color),
        Backend::Waybar => css_color(be, &bar.bg_color, None),
    };
    let fg = css_color(be, &bar.fg_color, None);
    let accent = css_color(be, &on_bar(&bar.accent_color), None);
    let surface = css_color(be, &surface_color(bar), None);
    let chip = css_color(be, &on_bar(&bar.module_bg_color), None);
    let hover = css_color(be, &on_bar(&bar.hover_color), None);
    let urgent = css_color(be, &on_bar(&bar.urgent_color), None);
    let inactive = css_color(be, &bar.inactive_color, None);
    let radius = css_radius(bar.radius);
    let (mut pad, vm, mut gap) = (bar.module_padding, bar.module_margin, bar.module_gap);
    let mut mr = css_radius(bar.module_radius);
    if continuous {
        // margins would be holes in the bar: the gap becomes padding
        pad += gap;
        gap = 0;
        mr = "0px".to_string();
    }
    let (tag_pad, tag_gap) = tag_spacing(bar);

    let mut ids: Vec<String> = Vec::new();
    let mut workspaces = false;
    for m in bar.modules.all() {
        if module_base(m) == "workspaces" {
            workspaces = true;
            continue;
        }
        let id = match bar.button_for_module(m) {
            Some(b) if be == Backend::Waybar && !b.picture.is_empty() => {
                Some(format!("#image.btn-{}", b.id))
            }
            _ => css_id(be, m, bar.waybar_mango_modules),
        };
        if let Some(id) = id {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }

    let mut out = String::new();
    let name = match be {
        Backend::Mangobar => "Mangobar",
        Backend::Waybar => "Waybar",
    };
    out.push_str(&format!("/* Generated by MangoWM Mod for {} */\n", name));
    let font = css_font(be, &bar.font);
    let weight = css_weight(be, &bar.font_weight);
    let size = bar.font_size;

    match be {
        // Mangobar reads only: color, background(-color), border-radius,
        // padding(-left/-right), margin(-left/-right; -top on #bar),
        // min-width, the font-* trio and border-color in menus (mangobar
        // `style.c` `apply_decl`). Every module and tag is painted at the
        // bar's full height; there are no borders, transitions or hover
        // except on tags.
        Backend::Mangobar => {
            out.push_str(&format!(
                "* {{\n    font-family: {font};\n    font-size: {size}px;\n    font-weight: {weight};\n}}\n\n"
            ));
            out.push_str(&format!(
                "#bar {{\n    background-color: {bg};\n    margin-top: {}px;\n    margin-left: {}px;\n    \
                 margin-right: {}px;\n}}\n\n",
                bar.margin_top, bar.margin_left, bar.margin_right
            ));
            if workspaces {
                // Mangobar paints each tag on its own, so the tags are a row
                // of equal blocks set close together: the island color under
                // every tag, the accent pill on the active one. The hover
                // rule is laid over whatever state the tag is in.
                let tag_w = bar.module_height();
                // tags in the middle of a continuous bar: see `in_center` below
                let tags_mid = continuous
                    && bar
                        .modules
                        .center
                        .iter()
                        .any(|m| module_base(m) == "workspaces");
                let raw = |c: &str| css_color(be, c, None);
                let (chip, hover, urgent, accent) = if tags_mid {
                    (
                        raw(&bar.module_bg_color),
                        raw(&bar.hover_color),
                        raw(&bar.urgent_color),
                        raw(&bar.accent_color),
                    )
                } else {
                    (chip.clone(), hover.clone(), urgent.clone(), accent.clone())
                };
                let (active_bg, active_fg) = if bar.tag_fill {
                    (accent.clone(), surface.clone())
                } else {
                    (chip.clone(), accent.clone())
                };
                out.push_str(&format!(
                    "#tags {{\n    padding: 0px {tag_pad}px;\n    margin: 0px {tag_gap}px;\n    \
                     min-width: {tag_w}px;\n    color: {inactive};\n    background-color: {chip};\n    \
                     border-radius: {mr};\n}}\n\n\
                     #tags.empty {{\n    color: {inactive};\n}}\n\n\
                     #tags.occupied {{\n    color: {fg};\n}}\n\n\
                     #tags.active {{\n    background-color: {active_bg};\n    color: {active_fg};\n}}\n\n\
                     #tags.urgent {{\n    background-color: {urgent};\n    color: {surface};\n}}\n\n\
                     #tags:hover {{\n    background-color: {hover};\n    color: {fg};\n}}\n\n\
                     #overview {{\n    padding: 0px {pad}px;\n    margin: 0px {gap}px;\n    \
                     background-color: {accent};\n    color: {surface};\n    border-radius: {mr};\n}}\n\n"
                ));
            }
            // A continuous bar's middle (#bar, painted from the end of the
            // left group to the right group) is under the center modules
            // too: they get their own color, not blended over the bar again.
            let in_center = |id: &String| {
                continuous
                    && bar
                        .modules
                        .center
                        .iter()
                        .any(|m| css_id(be, m, bar.waybar_mango_modules).as_ref() == Some(id))
                    && !bar
                        .modules
                        .left
                        .iter()
                        .chain(&bar.modules.right)
                        .any(|m| css_id(be, m, bar.waybar_mango_modules).as_ref() == Some(id))
            };
            let (center_ids, side_ids): (Vec<String>, Vec<String>) =
                ids.iter().cloned().partition(|id| in_center(id));
            let center_chip = css_color(be, &bar.module_bg_color, None);
            for (list, ground) in [(&side_ids, &chip), (&center_ids, &center_chip)] {
                if list.is_empty() {
                    continue;
                }
                out.push_str(&format!(
                    "{} {{\n    padding: 0px {pad}px;\n    margin: 0px {gap}px;\n    color: {fg};\n    \
                     background-color: {ground};\n    border-radius: {mr};\n}}\n\n",
                    list.join(",\n")
                ));
            }
            // tray menus: the only place Mangobar draws a border
            let edge = if bar.border_width > 0 && alpha(&bar.border_color) > 0 {
                css_color(be, &bar.border_color, None)
            } else {
                hover.clone()
            };
            out.push_str(&format!(
                "menu {{\n    background-color: {surface};\n    color: {fg};\n    border-color: {edge};\n}}\n\n\
                 menuitem:hover {{\n    background-color: {hover};\n    color: {fg};\n}}\n"
            ));
        }
        Backend::Waybar => {
            out.push_str(&format!(
                "* {{\n    font-family: {font};\n    font-size: {size}px;\n    font-weight: {weight};\n    \
                 min-height: 0;\n    border: none;\n    border-radius: 0;\n}}\n\n"
            ));
            let border = if bar.border_width > 0 {
                format!(
                    "\n    border: {}px solid {};",
                    bar.border_width,
                    css_color(be, &bar.border_color, None)
                )
            } else {
                String::new()
            };
            out.push_str(&format!(
                "window#waybar {{\n    background-color: {bg};\n    color: {fg};\n    border-radius: {radius};{border}\n}}\n\n\
                 tooltip {{\n    background-color: {surface};\n    border-radius: 10px;\n}}\n\n\
                 tooltip label {{\n    color: {fg};\n}}\n\n"
            ));
            if workspaces {
                // one island holds the tags; the buttons sit inside it
                let inset = waybar_tag_inset(bar);
                let inner = if bar.module_radius < 0 {
                    "999px".to_string()
                } else {
                    format!("{}px", (bar.module_radius - inset as i32).max(0))
                };
                let active = if bar.tag_fill {
                    format!("background-color: {accent};\n    color: {surface};")
                } else {
                    format!("color: {accent};\n    box-shadow: inset 0 -2px {accent};")
                };
                out.push_str(&format!(
                    "#workspaces {{\n    padding: 0 {inset}px;\n    margin: {vm}px {gap}px;\n    \
                     background-color: {chip};\n    border-radius: {mr};\n}}\n\n\
                     #workspaces button {{\n    padding: 0 {tag_pad}px;\n    margin: {inset}px {tag_gap}px;\n    \
                     color: {fg};\n    background: transparent;\n    border-radius: {inner};\n    box-shadow: none;\n    \
                     transition: background-color {TRANSITION}, color {TRANSITION};\n}}\n\n\
                     #workspaces button:hover {{\n    background: {hover};\n    color: {fg};\n    box-shadow: none;\n}}\n\n\
                     #workspaces button.empty {{\n    color: {inactive};\n}}\n\n\
                     #workspaces button.active {{\n    {active}\n}}\n\n\
                     #workspaces button.urgent {{\n    background-color: {urgent};\n    color: {surface};\n}}\n\n"
                ));
            }
            if !ids.is_empty() {
                out.push_str(&format!(
                    "{} {{\n    padding: 0 {pad}px;\n    margin: {vm}px {gap}px;\n    color: {fg};\n    \
                     background-color: {chip};\n    border-radius: {mr};\n    \
                     transition: background-color {TRANSITION};\n}}\n\n",
                    ids.join(",\n")
                ));
                let hovers: Vec<String> = ids.iter().map(|i| format!("{}:hover", i)).collect();
                out.push_str(&format!(
                    "{} {{\n    background-color: {hover};\n}}\n\n",
                    hovers.join(",\n")
                ));
            }
            out.push_str(&format!(
                "#pulseaudio.muted,\n#network.disconnected {{\n    color: {inactive};\n}}\n\n\
                 #tray > .needs-attention {{\n    background-color: {urgent};\n}}\n"
            ));
        }
    }
    out
}

/// Write `config.jsonc` and `style.css` into `dir` (default [`bar_dir`]).
pub fn write_bar_files<P: AsRef<Path>>(bar: &BarConfig, dir: Option<P>) -> Result<PathBuf> {
    let out_dir = match dir {
        Some(d) => d.as_ref().to_path_buf(),
        None => bar_dir(bar.backend),
    };
    fs::create_dir_all(&out_dir)?;
    atomic_write(out_dir.join(CONFIG_FILE), &generate_config(bar))?;
    atomic_write(out_dir.join(STYLE_FILE), &generate_style(bar))?;
    Ok(out_dir)
}
