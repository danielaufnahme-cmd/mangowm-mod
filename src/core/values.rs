//! Color and value utilities for MangoWM configuration.

use anyhow::Result;
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
            if h.len() == 6 {
                return Some(format!("#{}", h));
            } else if h.len() == 8 {
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
    if val { "true" } else { "false" }
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
    match type_name {
        "boolean" => {
            // Mango reads booleans with atoi(), so "true" would silently mean 0
            let b = parse_bool(raw).ok_or_else(|| anyhow::anyhow!("invalid boolean: {}", raw))?;
            Ok(if b { "1" } else { "0" }.to_string())
        }
        "color" => {
            // Mango reads colors with strtol(), so "#rrggbb" is invalid: write 0xRRGGBBAA
            let c = parse_color(raw).ok_or_else(|| anyhow::anyhow!("invalid color: {}", raw))?;
            let hex = &c[1..];
            Ok(if hex.len() == 6 { format!("0x{}ff", hex) } else { format!("0x{}", hex) })
        }
        "int" | "integer" => {
            let num: i64 = raw.trim().parse()?;
            Ok(num.to_string())
        }
        "float" => {
            let num: f64 = raw.trim().parse()?;
            Ok(format_float(num))
        }
        _ => Ok(raw.trim().to_string()),
    }
}
