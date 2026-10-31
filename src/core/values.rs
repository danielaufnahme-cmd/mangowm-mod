//! Color and value utilities for MangoWM configuration.

use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::sync::LazyLock;

static HEX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9a-fA-F]+$").unwrap());

fn clamp_byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

pub fn parse_color(text: &str) -> Option<String> {
    let t = text.trim();
    if let Some(stripped) = t.strip_prefix('#') {
        if HEX_RE.is_match(stripped) {
            let h = stripped.to_lowercase();
            if h.len() == 6 || h.len() == 8 {
                return Some(format!("#{}", h));
            }
        }
    }

    if (t.starts_with("0x") || t.starts_with("0X")) && t.len() == 10 && HEX_RE.is_match(&t[2..]) {
        let h = t[2..].to_lowercase();
        // Mango order: 0xRRGGBBAA
        return Some(format!("#{}", h));
    }

    if t.starts_with("rgba(") && t.ends_with(')') {
        let inner = &t[5..t.len() - 1];
        if inner.contains(',') {
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 4 {
                let r: f64 = parts[0].parse().ok()?;
                let g: f64 = parts[1].parse().ok()?;
                let b: f64 = parts[2].parse().ok()?;
                let a: f64 = parts[3].parse().ok()?;
                let a_byte = if a <= 1.0 { a * 255.0 } else { a };
                return Some(format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    clamp_byte(r),
                    clamp_byte(g),
                    clamp_byte(b),
                    clamp_byte(a_byte)
                ));
            }
        } else if HEX_RE.is_match(inner) && (inner.len() == 6 || inner.len() == 8) {
            return Some(format!("#{}", inner.to_lowercase()));
        }
    }

    None
}

pub fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

pub fn format_bool(val: bool) -> &'static str {
    if val {
        "true"
    } else {
        "false"
    }
}

pub fn format_float(val: f64) -> String {
    let s = format!("{:.4}", val);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

pub fn format_value(type_name: &str, raw: &str) -> Result<String> {
    format_option_value(type_name, &[], raw)
}

/// Turn user input into the exact text Mango parses for an option of this type.
pub fn format_option_value(type_name: &str, choices: &[String], raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.contains('\n') || raw.contains('\r') {
        bail!("value must be a single line");
    }
    match type_name {
        "boolean" => {
            // Mango reads booleans with atoi(), so "true" would silently mean 0
            let b = parse_bool(raw).ok_or_else(|| anyhow!("invalid boolean: {}", raw))?;
            Ok(if b { "1" } else { "0" }.to_string())
        }
        "color" => {
            // Mango reads colors with strtol(), so "#rrggbb" is invalid: write 0xRRGGBBAA
            let c = parse_color(raw).ok_or_else(|| anyhow!("invalid color: {}", raw))?;
            let hex = &c[1..];
            Ok(if hex.len() == 6 {
                format!("0x{}ff", hex)
            } else {
                format!("0x{}", hex)
            })
        }
        "int" | "integer" => {
            let num: i64 = raw
                .parse()
                .map_err(|_| anyhow!("invalid integer: {}", raw))?;
            Ok(num.to_string())
        }
        "float" => {
            let num: f64 = raw
                .parse()
                .map_err(|_| anyhow!("invalid number: {}", raw))?;
            if !num.is_finite() {
                bail!("invalid number: {}", raw);
            }
            Ok(format_float(num))
        }
        "enum" => {
            if choices.iter().any(|c| c == raw) {
                Ok(raw.to_string())
            } else {
                bail!(
                    "invalid value {:?}: choose one of {}",
                    raw,
                    choices.join(", ")
                )
            }
        }
        "curve" => {
            // animation_curve_*: exactly four comma-separated numbers (bezier points)
            let nums = parse_number_list(raw)?;
            if nums.len() != 4 {
                bail!("a curve needs exactly 4 numbers, e.g. 0.46,1.0,0.29,1");
            }
            Ok(nums
                .iter()
                .map(|n| format_float(*n))
                .collect::<Vec<_>>()
                .join(","))
        }
        "list" => {
            let items: Vec<&str> = raw.split(',').map(str::trim).collect();
            if items.iter().any(|i| i.is_empty()) {
                bail!("empty item in list: {}", raw);
            }
            Ok(items.join(","))
        }
        "tag_rule" => format_tag_rule(raw),
        _ => {
            if raw.is_empty() {
                bail!("value must not be empty (use reset to remove a setting)");
            }
            Ok(raw.to_string())
        }
    }
}

fn parse_number_list(raw: &str) -> Result<Vec<f64>> {
    raw.split(',')
        .map(|p| {
            let p = p.trim();
            p.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| anyhow!("invalid number in list: {:?}", p))
        })
        .collect()
}

/// Validate a `tag_rule` value such as `id:*,layout_name:scroller`.
pub fn format_tag_rule(raw: &str) -> Result<String> {
    let mut parts = Vec::new();
    for pair in raw.split(',') {
        let (k, v) = pair
            .split_once(':')
            .ok_or_else(|| anyhow!("tag_rule items must be key:value, got {:?}", pair))?;
        let (k, v) = (k.trim(), v.trim());
        if !super::items::TAG_RULE_KEYS.contains(&k) {
            bail!("unknown tag_rule key {:?}", k);
        }
        if k == "layout_name" && !super::schema::LAYOUTS.contains(&v) {
            bail!(
                "unknown layout {:?}: choose one of {}",
                v,
                super::schema::LAYOUTS.join(", ")
            );
        }
        parts.push(format!("{}:{}", k, v));
    }
    Ok(parts.join(","))
}
