//! MangoWM items: key binds, rules (window/tag/layer/monitor/device) and monitors.
//!
//! Syntax checked against MangoWM 0.18.4 (`src/config/option_defs.c`,
//! `src/config/binding_parse.c`, docs `bindings/keys.md`, `configuration/toml.md`).

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use super::actions::is_known_action;

/// Modifier names `parse_mod()` accepts (case-insensitive), joined with `+`.
/// `code:<keycode>` (133/134, 37/105, 50/62, 64/108) is accepted as well.
pub const MODIFIERS: &[&str] = &[
    "super", "super_l", "super_r", "ctrl", "ctrl_l", "ctrl_r", "shift", "shift_l", "shift_r",
    "alt", "alt_l", "alt_r", "hyper", "hyper_l", "hyper_r", "none",
];

/// Flag letters allowed after `bind` (`bindl`, `bindr`, ...): regex `^bind[s|l|r|p|c]*$`.
/// s = keysym, l = works when locked, r = on release, p = pass through, c = allow conflict.
pub const BIND_FLAGS: &str = "slrpc";

pub const RULE_KINDS: &[&str] = &[
    "window_rule",
    "window_rule_once",
    "tag_rule",
    "layer_rule",
    "monitor_rule",
    "device_rule",
];

pub const WINDOW_RULE_KEYS: &[&str] = &[
    "app_id",
    "title",
    "is_floating",
    "animation_type_open",
    "animation_type_close",
    "tags",
    "monitor",
    "offset_x",
    "offset_y",
    "no_focus",
    "no_fade_in",
    "no_fade_out",
    "no_force_center",
    "width",
    "height",
    "no_border",
    "no_shadow",
    "no_radius",
    "no_animation",
    "is_open_silent",
    "is_tag_silent",
    "is_named_scratchpad",
    "is_unmanaged_global",
    "is_global",
    "scroller_proportion_single",
    "unfocused_opacity",
    "focused_opacity",
    "is_overlay",
    "shield_when_capture",
    "allow_shortcuts_inhibit",
    "ignore_maximize",
    "ignore_minimize",
    "no_size_hint",
    "idle_inhibit_when_focus",
    "vrr_only_fullscreen",
    "force_render",
    "activation_bypass",
    "is_term",
    "allow_csd",
    "force_fake_maximize",
    "force_tiled_state",
    "force_tearing",
    "no_swallow",
    "confine_pointer",
    "no_blur",
    "scroller_proportion",
    "is_fullscreen",
    "is_fake_fullscreen",
    "global_key_binding",
];

pub const TAG_RULE_KEYS: &[&str] = &[
    "id",
    "layout_name",
    "monitor_name",
    "monitor_make",
    "monitor_model",
    "monitor_serial",
    "no_render_border",
    "open_as_floating",
    "no_hide",
    "master_count",
    "master_factor",
    "scroller_default_proportion",
    "scroller_default_proportion_single",
    "scroller_ignore_proportion_single",
];

pub const LAYER_RULE_KEYS: &[&str] = &[
    "layer_name",
    "animation_type_open",
    "animation_type_close",
    "shield_when_capture",
    "no_blur",
    "no_animation",
    "no_shadow",
];

pub const MONITOR_RULE_KEYS: &[&str] = &[
    "name",
    "make",
    "model",
    "serial",
    "width",
    "height",
    "refresh",
    "x",
    "y",
    "scale",
    "rr",
    "vrr",
    "hdr",
    "hdr_min_lum",
    "hdr_max_lum",
    "hdr_max_avg_lum",
    "hdr_force",
    "icc",
    "disable",
    "primary",
    "custom",
];

pub const DEVICE_RULE_KEYS: &[&str] = &[
    "name",
    "type",
    "repeat_rate",
    "repeat_delay",
    "kb_rules",
    "kb_model",
    "kb_layout",
    "kb_variant",
    "kb_options",
    "natural_scrolling",
    "accel_profile",
    "accel_speed",
    "left_handed",
    "middle_button_emulation",
    "scroll_method",
    "scroll_button",
    "click_method",
    "send_events_mode",
    "tap_to_click",
    "tap_and_drag",
    "drag_lock",
    "button_map",
    "disable_while_typing",
    "monitor",
    "map_focus_monitor",
];

pub fn rule_keys(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "window_rule" | "window_rule_once" => WINDOW_RULE_KEYS,
        "tag_rule" => TAG_RULE_KEYS,
        "layer_rule" => LAYER_RULE_KEYS,
        "monitor_rule" => MONITOR_RULE_KEYS,
        "device_rule" => DEVICE_RULE_KEYS,
        _ => return None,
    })
}

/// Mango reads config lines into a 512-byte buffer.
const MAX_LINE: usize = 500;

fn check_field(what: &str, v: &str) -> Result<()> {
    if v.contains(['\n', '\r']) {
        bail!("{} must be a single line", what);
    }
    Ok(())
}

fn default_mode() -> String {
    "default".into()
}

fn is_default_mode(m: &str) -> bool {
    m == "default"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Bind {
    /// Bind flags after `bind`, e.g. "l" for `bindl` (see [`BIND_FLAGS`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flags: String,
    /// Key mode the bind belongs to (`key_mode=` line before it). Default: "default".
    #[serde(default = "default_mode", skip_serializing_if = "is_default_mode")]
    pub mode: String,
    #[serde(default)]
    pub mods: Vec<String>,
    pub key: String,
    pub action: String,
    #[serde(default)]
    pub args: String,
}

impl Bind {
    pub fn new(mods: &str, key: &str, action: &str, args: &str) -> Self {
        Bind {
            flags: String::new(),
            mode: default_mode(),
            mods: split_mods(mods),
            key: key.trim().to_string(),
            action: action.trim().to_string(),
            args: args.trim().to_string(),
        }
    }

    pub fn keyword(&self) -> String {
        format!("bind{}", self.flags)
    }

    /// Modifiers as Mango expects them: joined with `+`, `NONE` when empty.
    pub fn mods_string(&self) -> String {
        if self.mods.is_empty() {
            "NONE".to_string()
        } else {
            self.mods.join("+")
        }
    }

    pub fn to_config_line(&self) -> String {
        let mut line = format!(
            "{}={},{},{}",
            self.keyword(),
            self.mods_string(),
            self.key,
            self.action
        );
        if !self.args.is_empty() {
            line.push(',');
            line.push_str(&self.args);
        }
        line
    }

    /// Same mode, modifiers and key (case-insensitive, like Mango's parser).
    pub fn same_combo(&self, other: &Bind) -> bool {
        let norm = |b: &Bind| {
            let mut m: Vec<String> = b
                .mods
                .iter()
                .map(|s| s.to_lowercase())
                .filter(|s| s != "none")
                .collect();
            m.sort();
            (b.mode.clone(), m, b.key.to_lowercase())
        };
        norm(self) == norm(other)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.flags.chars().all(|c| BIND_FLAGS.contains(c)) {
            bail!(
                "invalid bind flags {:?}: use letters from {:?}",
                self.flags,
                BIND_FLAGS
            );
        }
        if self.mode.is_empty() || self.mode.contains([',', '=', ' ']) || self.mode.len() > 27 {
            bail!("invalid key mode {:?}", self.mode);
        }
        for m in &self.mods {
            let lm = m.to_lowercase();
            let is_code = lm.strip_prefix("code:").is_some_and(|n| {
                matches!(n, "133" | "134" | "37" | "105" | "50" | "62" | "64" | "108")
            });
            if !MODIFIERS.contains(&lm.as_str()) && !is_code {
                bail!(
                    "unknown modifier {:?}: use {} (joined with +)",
                    m,
                    "SUPER, CTRL, SHIFT, ALT, HYPER or NONE"
                );
            }
        }
        if self.key.is_empty() || self.key.contains([',', '+', ' ']) {
            bail!(
                "invalid key {:?}: use one XKB key name such as Return, q, F1 or code:24",
                self.key
            );
        }
        if !is_known_action(&self.action) {
            bail!(
                "unknown action {:?}: run `mangowm-mod bind actions` to list Mango's actions",
                self.action
            );
        }
        check_field("bind arguments", &self.args)?;
        if self.to_config_line().len() > MAX_LINE {
            bail!(
                "bind line is too long for Mango (max {} characters)",
                MAX_LINE
            );
        }
        Ok(())
    }

    /// Parse the value of a `bind*=` line (`mods,key,action[,args]`).
    pub fn parse(keyword: &str, value: &str, mode: &str) -> Option<Bind> {
        let flags = keyword.strip_prefix("bind")?;
        if !flags.chars().all(|c| BIND_FLAGS.contains(c)) {
            return None;
        }
        let mut parts = value.splitn(4, ',');
        let mods = parts.next()?.trim();
        let key = parts.next()?.trim();
        let action = parts.next()?.trim();
        let args = parts.next().unwrap_or("").trim();
        let mut b = Bind::new(mods, key, action, args);
        b.flags = flags.to_string();
        b.mode = mode.to_string();
        Some(b)
    }
}

/// Split "SUPER+SHIFT" (or "super shift") into modifiers; "NONE"/"" means none.
pub fn split_mods(mods: &str) -> Vec<String> {
    mods.split(['+', ' '])
        .map(str::trim)
        .filter(|m| !m.is_empty() && !m.eq_ignore_ascii_case("none"))
        .map(|m| m.to_uppercase())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Rule {
    /// `window_rule`, `window_rule_once`, `tag_rule`, `layer_rule`, `monitor_rule` or `device_rule`.
    pub kind: String,
    /// Ordered `key:value` pairs.
    pub fields: Vec<(String, String)>,
}

impl Rule {
    /// Build a rule from "key:value,key:value" text.
    pub fn parse(kind: &str, text: &str) -> Result<Rule> {
        let mut fields = Vec::new();
        for pair in text.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair
                .split_once(':')
                .ok_or_else(|| anyhow!("rule items must be key:value, got {:?}", pair))?;
            fields.push((k.trim().to_string(), v.trim().to_string()));
        }
        let rule = Rule {
            kind: kind.trim().to_string(),
            fields,
        };
        rule.validate()?;
        Ok(rule)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn body(&self) -> String {
        self.fields
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn to_config_line(&self) -> String {
        format!("{}={}", self.kind, self.body())
    }

    pub fn validate(&self) -> Result<()> {
        let keys = rule_keys(&self.kind).ok_or_else(|| {
            anyhow!(
                "unknown rule type {:?}: use one of {}",
                self.kind,
                RULE_KINDS.join(", ")
            )
        })?;
        if self.fields.is_empty() {
            bail!("a {} needs at least one key:value pair", self.kind);
        }
        for (k, v) in &self.fields {
            if !keys.contains(&k.as_str()) {
                bail!(
                    "unknown {} key {:?}: valid keys are {}",
                    self.kind,
                    k,
                    keys.join(", ")
                );
            }
            if v.is_empty() || v.contains(',') {
                bail!(
                    "{} value for {:?} must be non-empty and contain no comma",
                    self.kind,
                    k
                );
            }
            check_field("rule value", v)?;
            if k == "layout_name" && !super::schema::LAYOUTS.contains(&v.as_str()) {
                bail!(
                    "unknown layout {:?}: choose one of {}",
                    v,
                    super::schema::LAYOUTS.join(", ")
                );
            }
            if (k == "animation_type_open" || k == "animation_type_close")
                && !["none", "fade", "slide", "zoom"].contains(&v.as_str())
            {
                bail!("{} must be none, fade, slide or zoom", k);
            }
        }
        if self.to_config_line().len() > MAX_LINE {
            bail!(
                "rule line is too long for Mango (max {} characters)",
                MAX_LINE
            );
        }
        Ok(())
    }
}

/// Convenience builder for a `monitor_rule`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Monitor {
    /// Output name, e.g. eDP-1 (Mango matches it as a regex).
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub refresh: Option<f64>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub scale: Option<f64>,
    /// Transform 0-7 (`rr`).
    pub transform: Option<u8>,
    pub vrr: Option<bool>,
    pub disable: bool,
    pub primary: bool,
}

impl Monitor {
    pub fn to_rule(&self) -> Result<Rule> {
        if self.name.trim().is_empty() {
            bail!("monitor name is required (see `wlr-randr` or `mmsg get all-monitors`)");
        }
        let mut f: Vec<(String, String)> = vec![("name".into(), self.name.trim().into())];
        let mut push = |k: &str, v: Option<String>| {
            if let Some(v) = v {
                f.push((k.into(), v));
            }
        };
        push("width", self.width.map(|v| v.to_string()));
        push("height", self.height.map(|v| v.to_string()));
        push("refresh", self.refresh.map(super::values::format_float));
        push("x", self.x.map(|v| v.to_string()));
        push("y", self.y.map(|v| v.to_string()));
        push("scale", self.scale.map(super::values::format_float));
        push("rr", self.transform.map(|v| v.to_string()));
        push(
            "vrr",
            self.vrr.map(|v| if v { "1" } else { "0" }.to_string()),
        );
        if self.disable {
            push("disable", Some("1".into()));
        }
        if self.primary {
            push("primary", Some("1".into()));
        }
        if self.transform.is_some_and(|t| t > 7) {
            bail!("transform must be 0-7");
        }
        if self.width.is_some() != self.height.is_some() {
            bail!("set both width and height (or neither)");
        }
        let rule = Rule {
            kind: "monitor_rule".into(),
            fields: f,
        };
        rule.validate()?;
        Ok(rule)
    }

    pub fn to_config_line(&self) -> String {
        self.to_rule()
            .map(|r| r.to_config_line())
            .unwrap_or_default()
    }
}
