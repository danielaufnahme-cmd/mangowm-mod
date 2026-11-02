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

use super::model::{module_base, Backend, BarConfig, MANGOBAR_MODULE_KEYS};
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
const WAYBAR_WINDOW_EXEC: &str =
    "mmsg watch focusing-client | jq --unbuffered -r '.title // \"\"'";

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

/// Notes about modules the selected backend cannot show (or may not have).
pub fn warnings(bar: &BarConfig) -> Vec<String> {
    let mut out = Vec::new();
    if bar.backend != Backend::Waybar {
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
            "layout" | "window" => json!({"format": "{}"}),
            "keyboardlayout" => json!({"format": "{short}"}),
            "hideclients" => json!({"format": " {}"}),
            // {load} is the load average, not a percentage
            "cpu" => json!({"format": "󰍛 {usage}%", "interval": 3}),
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
            "window" if !bar.waybar_mango_modules => json!({
                "exec": WAYBAR_WINDOW_EXEC,
                "restart-interval": 3,
                "escape": true,
                "max-length": 60,
                "hide-empty-text": true,
                "tooltip": false
            }),
            "cpu" => json!({"format": "󰍛 {usage}%", "interval": 3, "tooltip": false}),
            "memory" => json!({
                "format": "󰘚 {percentage}%",
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
        Backend::Waybar => list
            .iter()
            .filter_map(|m| waybar_module_name(m, bar.waybar_mango_modules))
            .collect(),
    }
}

pub fn generate_config(bar: &BarConfig) -> String {
    let mut map = Map::new();
    map.insert("layer".into(), json!(bar.layer));
    if bar.backend == Backend::Waybar {
        map.insert("position".into(), json!(bar.position));
    }
    map.insert("height".into(), json!(bar.height));
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
            Backend::Waybar => match waybar_module_name(m, bar.waybar_mango_modules) {
                Some(n) => n,
                None => continue,
            },
        };
        let mut obj = match module_defaults(bar, m) {
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

/// CSS radius: Mangobar draws -1 as a pill, GTK clamps a large radius.
fn css_radius(backend: Backend, radius: i32) -> String {
    match (backend, radius < 0) {
        (Backend::Mangobar, true) => "-1".to_string(),
        (Backend::Waybar, true) => "999px".to_string(),
        _ => format!("{}px", radius),
    }
}

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
    let bg = css_color(be, &bar.bg_color, None);
    let fg = css_color(be, &bar.fg_color, None);
    let accent = css_color(be, &bar.accent_color, None);
    let solid_bg = css_color(be, &bar.bg_color, Some(0xff));
    let chip = css_color(be, &bar.module_bg_color, None);
    let hover = css_color(be, &bar.hover_color, None);
    let urgent = css_color(be, &bar.urgent_color, None);
    let inactive = css_color(be, &bar.inactive_color, None);
    let radius = css_radius(be, bar.radius);
    let mr = css_radius(be, bar.module_radius);
    let (pad, vm, gap) = (bar.module_padding, bar.module_margin, bar.module_gap);

    let mut ids: Vec<String> = Vec::new();
    let mut workspaces = false;
    for m in bar.modules.all() {
        if module_base(m) == "workspaces" {
            workspaces = true;
            continue;
        }
        if let Some(id) = css_id(be, m, bar.waybar_mango_modules) {
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
        Backend::Mangobar => {
            out.push_str(&format!(
                "* {{\n    font-family: {font};\n    font-size: {size}px;\n    font-weight: {weight};\n}}\n\n"
            ));
            out.push_str(&format!(
                "#bar {{\n    background-color: {bg};\n    color: {fg};\n    border-radius: {radius};\n    \
                 margin-top: {}px;\n    margin-left: {}px;\n    margin-right: {}px;\n}}\n\n",
                bar.margin_top, bar.margin_left, bar.margin_right
            ));
            if workspaces {
                out.push_str(&format!(
                    "#tags {{\n    padding: 0px {pad}px;\n    margin: {vm}px {gap}px;\n    color: {inactive};\n    \
                     background: none;\n    border-radius: {mr};\n}}\n\n\
                     #tags:hover {{\n    background-color: {hover};\n    color: {fg};\n}}\n\n\
                     #tags.active {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n\
                     #tags.occupied {{\n    color: {fg};\n}}\n\n\
                     #tags.urgent {{\n    background-color: {urgent};\n    color: {solid_bg};\n}}\n\n\
                     #tags.empty,\n#tags.inactive {{\n    color: {inactive};\n}}\n\n\
                     #overview {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n"
                ));
            }
            if !ids.is_empty() {
                out.push_str(&format!(
                    "{} {{\n    padding: 0px {pad}px;\n    margin: {vm}px {gap}px;\n    color: {fg};\n    \
                     background-color: {chip};\n    border-radius: {mr};\n}}\n\n",
                    ids.join(",\n")
                ));
            }
            // tray menus (border-color is only read for menus)
            out.push_str(&format!(
                "menu {{\n    background-color: {solid_bg};\n    color: {fg};\n    border-color: {chip};\n}}\n\n\
                 menuitem:hover {{\n    background-color: {hover};\n}}\n"
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
                 tooltip {{\n    background-color: {solid_bg};\n    border-radius: 10px;\n}}\n\n\
                 tooltip label {{\n    color: {fg};\n}}\n\n"
            ));
            if workspaces {
                out.push_str(&format!(
                    "#workspaces button {{\n    padding: 0 {pad}px;\n    margin: {vm}px {gap}px;\n    color: {inactive};\n    \
                     background: transparent;\n    border-radius: {mr};\n    \
                     transition: background-color 150ms ease, color 150ms ease;\n}}\n\n\
                     #workspaces button:hover {{\n    background: {hover};\n    color: {fg};\n    box-shadow: none;\n}}\n\n\
                     #workspaces button.active {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n\
                     #workspaces button.urgent {{\n    background-color: {urgent};\n    color: {solid_bg};\n}}\n\n\
                     #workspaces button.empty {{\n    color: {inactive};\n}}\n\n"
                ));
            }
            if !ids.is_empty() {
                out.push_str(&format!(
                    "{} {{\n    padding: 0 {pad}px;\n    margin: {vm}px {gap}px;\n    color: {fg};\n    \
                     background-color: {chip};\n    border-radius: {mr};\n    \
                     transition: background-color 150ms ease;\n}}\n\n",
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
