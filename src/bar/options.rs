//! Per-module options the Bar page shows, with their name in each program.
//!
//! Options are stored under the Mangobar key (mangobar `rtconfig.c`); the
//! Waybar generator renames them (`icons` -> `format-icons`, ...) and drops
//! the ones Waybar does not have. Waybar-only options keep Waybar's key.

use serde_json::Value;

use super::model::{module_base, Backend};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptKind {
    Text,
    Int,
    Bool,
    /// Comma-separated strings, stored as a JSON array.
    List,
    /// Comma-separated numbers, stored as a JSON array.
    IntList,
}

#[derive(Debug, Clone, Copy)]
pub struct ModuleOpt {
    /// Stored key (Mangobar's name, or Waybar's for Waybar-only options).
    pub key: &'static str,
    pub label: &'static str,
    pub kind: OptKind,
    pub mangobar: bool,
    /// Waybar's key, `None` when Waybar has no such option.
    pub waybar: Option<&'static str>,
}

impl ModuleOpt {
    pub fn supported(&self, backend: Backend) -> bool {
        match backend {
            Backend::Mangobar => self.mangobar,
            Backend::Waybar => self.waybar.is_some(),
        }
    }

    /// The key the given program reads.
    pub fn key_for(&self, backend: Backend) -> Option<&'static str> {
        match backend {
            Backend::Mangobar => self.mangobar.then_some(self.key),
            Backend::Waybar => self.waybar,
        }
    }
}

const fn both(key: &'static str, label: &'static str, kind: OptKind) -> ModuleOpt {
    ModuleOpt {
        key,
        label,
        kind,
        mangobar: true,
        waybar: Some(key),
    }
}

const fn mangobar(key: &'static str, label: &'static str, kind: OptKind) -> ModuleOpt {
    ModuleOpt {
        key,
        label,
        kind,
        mangobar: true,
        waybar: None,
    }
}

const fn waybar(key: &'static str, label: &'static str, kind: OptKind) -> ModuleOpt {
    ModuleOpt {
        key,
        label,
        kind,
        mangobar: false,
        waybar: Some(key),
    }
}

const fn renamed(
    key: &'static str,
    wb: &'static str,
    label: &'static str,
    kind: OptKind,
) -> ModuleOpt {
    ModuleOpt {
        key,
        label,
        kind,
        mangobar: true,
        waybar: Some(wb),
    }
}

use OptKind::*;

/// Every module has these.
const COMMON: &[ModuleOpt] = &[
    both("format", "Format", Text),
    both("format-alt", "Alternative format (on click)", Text),
    both("max-length", "Max length", Int),
    both("on-click", "On click", Text),
    both("on-click-middle", "On middle click", Text),
    both("on-click-right", "On right click", Text),
    both("on-scroll-up", "On scroll up", Text),
    both("on-scroll-down", "On scroll down", Text),
];

const WORKSPACES: &[ModuleOpt] = &[
    mangobar("hide-empty", "Hide empty tags", Bool),
    mangobar("pinned", "Always show tags (e.g. 1,2,3)", IntList),
    mangobar("tag-names", "Tag names (comma-separated)", List),
    mangobar("overview-label", "Overview label", Text),
    waybar("sort-by-id", "Sort by ID", Bool),
    waybar("ignore-hidden", "Hide empty tags", Bool),
];
const TRAY: &[ModuleOpt] = &[
    both("icon-size", "Icon size", Int),
    both("spacing", "Spacing", Int),
];
const INTERVAL: &[ModuleOpt] = &[both("interval", "Update interval (s)", Int)];
const BACKLIGHT: &[ModuleOpt] = &[
    renamed("icons", "format-icons", "Icons (comma-separated)", List),
    both("device", "Device", Text),
    both("scroll-step", "Scroll step", Int),
];
const PULSEAUDIO: &[ModuleOpt] = &[
    both("format-muted", "Format when muted", Text),
    renamed("icons", "format-icons", "Icons (comma-separated)", List),
    mangobar("icon-muted", "Muted icon", Text),
    mangobar("icon-bluetooth", "Bluetooth icon", Text),
    both("scroll-step", "Scroll step", Int),
    both("max-volume", "Max volume", Int),
];
const CLOCK: &[ModuleOpt] = &[waybar("interval", "Update interval (s)", Int)];
const NETWORK: &[ModuleOpt] = &[
    both("format-wifi", "Format on Wi-Fi", Text),
    both("format-ethernet", "Format on ethernet", Text),
    both("format-disconnected", "Format when disconnected", Text),
    mangobar("icon-wifi", "Wi-Fi icon", Text),
    mangobar("icon-ethernet", "Ethernet icon", Text),
    mangobar("icon-disconnected", "Disconnected icon", Text),
    renamed("signal-icons", "format-icons", "Signal icons (comma-separated)", List),
    both("interval", "Update interval (s)", Int),
];
const BATTERY: &[ModuleOpt] = &[
    renamed("icons", "format-icons", "Icons (comma-separated)", List),
    mangobar("icon-charging", "Charging icon", Text),
    mangobar("icon-full", "Full icon", Text),
    mangobar("icon-discharging", "Discharging icon", Text),
    mangobar("icon-ac", "On AC icon", Text),
    mangobar("hide-on-ac", "Hide on AC", Bool),
    renamed("device", "bat", "Device", Text),
    waybar("interval", "Update interval (s)", Int),
];
const CUSTOM: &[ModuleOpt] = &[
    both("exec", "Command", Text),
    both("interval", "Update interval (s)", Int),
    both("signal", "Refresh signal (SIGRTMIN+N)", Int),
];

/// Options of one module (common ones first).
pub fn module_opts(name: &str) -> Vec<&'static ModuleOpt> {
    let specific: &[ModuleOpt] = if name.starts_with("custom/") {
        CUSTOM
    } else {
        match module_base(name) {
            "workspaces" => WORKSPACES,
            "tray" => TRAY,
            "cpu" | "memory" => INTERVAL,
            "backlight" => BACKLIGHT,
            "pulseaudio" => PULSEAUDIO,
            "clock" => CLOCK,
            "network" => NETWORK,
            "battery" => BATTERY,
            _ => &[],
        }
    };
    COMMON.iter().chain(specific).collect()
}

pub fn find_opt(module: &str, key: &str) -> Option<&'static ModuleOpt> {
    module_opts(module).into_iter().find(|o| o.key == key)
}

/// Parse text typed for an option of `kind`.
pub fn parse_opt_value(kind: OptKind, text: &str) -> anyhow::Result<Value> {
    let t = text.trim();
    let list = || -> Vec<&str> {
        t.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    };
    Ok(match kind {
        Text => Value::String(text.to_string()),
        Int => Value::from(
            t.parse::<i64>()
                .map_err(|_| anyhow::anyhow!("{:?} is not a whole number", t))?,
        ),
        Bool => Value::Bool(
            crate::core::values::parse_bool(t)
                .ok_or_else(|| anyhow::anyhow!("{:?} is not true or false", t))?,
        ),
        // a JSON array is taken as typed
        List | IntList if t.starts_with('[') => serde_json::from_str(t)
            .map_err(|e| anyhow::anyhow!("invalid list {:?}: {}", t, e))?,
        List => Value::from(list()),
        IntList => Value::from(
            list()
                .iter()
                .map(|s| s.parse::<i64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| anyhow::anyhow!("{:?} is not a list of numbers", t))?,
        ),
    })
}

/// Show a stored value in a text field (lists comma-separated).
pub fn format_opt_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}
