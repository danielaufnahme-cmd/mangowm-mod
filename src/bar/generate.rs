//! Config and style generator for Mangobar and Waybar.
//!
//! Mangobar: `config.jsonc` keys and CSS ids from mangobar `rtconfig.c` /
//! `style.c` (hex colors only). Waybar: `mango/*` modules (Waybar master),
//! GTK3 CSS with rgba() colors. Both write `config.jsonc` + `style.css`.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::model::{module_base, Backend, BarConfig};
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

/// Waybar name of a module, or `None` when this Waybar setup cannot show it.
pub fn waybar_module_name(name: &str, mango_modules: bool) -> Option<String> {
    let mapped = match name {
        "workspaces" if mango_modules => "mango/workspaces",
        "workspaces" => "ext/workspaces",
        "window" | "layout" | "keymode" if mango_modules => {
            return Some(format!("mango/{}", name));
        }
        "keyboardlayout" if mango_modules => "mango/language",
        "window" | "layout" | "keymode" | "keyboardlayout" | "hideclients" => return None,
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
                "{} needs Waybar's mango/* modules (waybar-mango-modules=false); it was left out",
                m
            ));
        } else if m == "workspaces" && !bar.waybar_mango_modules {
            out.push("workspaces uses Waybar's ext/workspaces module".to_string());
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
fn module_defaults(backend: Backend, name: &str) -> Value {
    match backend {
        Backend::Mangobar => match name {
            "workspaces" => {
                json!({"hide-empty": true, "on-click": "activate", "on-click-right": "toggle"})
            }
            "layout" | "window" => json!({"format": "{}"}),
            "keyboardlayout" => json!({"format": "{short}"}),
            "hideclients" => json!({"format": " {}"}),
            "cpu" => json!({"format": " {load}%"}),
            "memory" => json!({"format": " {}%"}),
            "backlight" => json!({
                "format": "{icon} {percent}%",
                "icons": ["󰃚", "󰃛", "󰃜", "󰃝", "󰃞", "󰃟"]
            }),
            "pulseaudio" => json!({
                "format": "{icon} {volume}%",
                "format-muted": "󰝟 {volume}%",
                "icons": ["󰝟", "󰕿", "󰕾"]
            }),
            "network" => json!({
                "format": "{icon} {ifname}",
                "format-wifi": "{signal} {ssid}",
                "format-disconnected": "{icon}",
                "icon-wifi": "󰖩",
                "icon-ethernet": "󰈀",
                "icon-disconnected": "󰖪",
                "signal-icons": ["󰤯", "󰤟", "󰤢", "󰤥", "󰤨"]
            }),
            "clock" | "clock#time" => json!({"format": " {:L%H:%M}"}),
            "clock#date" => json!({"format": " {:L%a %d %b}"}),
            "tray" => json!({"spacing": 8}),
            _ if module_base(name) == "battery" => json!({
                "format": "{icon} {percent}%",
                "icons": ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"]
            }),
            _ => json!({}),
        },
        Backend::Waybar => match name {
            "cpu" => json!({"format": " {usage}%"}),
            "memory" => json!({"format": " {percentage}%"}),
            "backlight" => json!({"format": "󰃟 {percent}%"}),
            "pulseaudio" => json!({"format": "󰕾 {volume}%", "format-muted": "󰝟 muted"}),
            "network" => json!({
                "format-wifi": "󰖩 {essid}",
                "format-ethernet": "󰈀 {ifname}",
                "format-disconnected": "󰖪"
            }),
            "clock" | "clock#time" => json!({"format": " {:%H:%M}"}),
            "clock#date" => json!({"format": " {:%a %d %b}"}),
            "tray" => json!({"spacing": 8}),
            _ if module_base(name) == "battery" => json!({"format": "󰁹 {capacity}%"}),
            _ => json!({}),
        },
    }
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
    map.insert("height".into(), json!(bar.height));
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
        let mut obj = match module_defaults(bar.backend, m) {
            Value::Object(o) => o,
            _ => Map::new(),
        };
        if let Some(over) = bar.module_options.get(m) {
            for (k, v) in over {
                obj.insert(k.clone(), v.clone());
            }
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
    let dim = css_color(be, &bar.fg_color, Some(0x80));
    let chip = css_color(be, &bar.fg_color, Some(0x14));
    let accent = css_color(be, &bar.accent_color, None);
    let solid_bg = css_color(be, &bar.bg_color, Some(0xff));
    let btn = (bar.radius / 2).max(4);

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
    out.push_str(&format!(
        "* {{\n    font-family: \"{}\";\n    font-size: {}px;\n}}\n\n",
        bar.font, bar.font_size
    ));

    match be {
        Backend::Mangobar => {
            out.push_str(&format!(
                "#bar {{\n    background-color: {bg};\n    color: {fg};\n    border-radius: {r}px;\n}}\n\n",
                r = bar.radius
            ));
            if workspaces {
                out.push_str(&format!(
                    "#tags {{\n    padding: 0px 8px;\n    margin: 4px 2px;\n    color: {fg};\n    \
                     background: none;\n    border-radius: {btn}px;\n}}\n\n\
                     #tags.active {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n\
                     #tags.occupied {{\n    color: {accent};\n}}\n\n\
                     #tags.urgent {{\n    background-color: {fg};\n    color: {solid_bg};\n}}\n\n\
                     #tags.empty {{\n    color: {dim};\n}}\n\n\
                     #overview {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n"
                ));
            }
            if !ids.is_empty() {
                out.push_str(&format!(
                    "{} {{\n    padding: 0px 10px;\n    margin: 4px 2px;\n    color: {fg};\n    \
                     background-color: {chip};\n    border-radius: {btn}px;\n}}\n",
                    ids.join(",\n")
                ));
            }
        }
        Backend::Waybar => {
            out.push_str(&format!(
                "window#waybar {{\n    background-color: {bg};\n    color: {fg};\n    border-radius: {r}px;\n}}\n\n",
                r = bar.radius
            ));
            if workspaces {
                out.push_str(&format!(
                    "#workspaces button {{\n    padding: 0 8px;\n    margin: 4px 2px;\n    color: {fg};\n    \
                     background: transparent;\n    border: none;\n    border-radius: {btn}px;\n    \
                     min-height: 0;\n}}\n\n\
                     #workspaces button.active {{\n    background-color: {accent};\n    color: {solid_bg};\n}}\n\n\
                     #workspaces button.urgent {{\n    background-color: {fg};\n    color: {solid_bg};\n}}\n\n\
                     #workspaces button.empty {{\n    color: {dim};\n}}\n\n"
                ));
            }
            if !ids.is_empty() {
                out.push_str(&format!(
                    "{} {{\n    padding: 0 10px;\n    margin: 4px 2px;\n    color: {fg};\n    \
                     background-color: {chip};\n    border-radius: {btn}px;\n}}\n",
                    ids.join(",\n")
                ));
            }
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
