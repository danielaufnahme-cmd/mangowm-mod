//! Plain helpers behind the widgets (no GTK here, so they are unit-tested).

use anyhow::{anyhow, Result};
use std::str::FromStr;

use mangowm_mod::core::items::Rule;
use mangowm_mod::core::values::parse_color;

/// Any color Mango or the bar accepts, as RGBA floats 0..1.
pub fn color_to_rgba(text: &str) -> Option<[f32; 4]> {
    let c = parse_color(text)?;
    let hex = &c[1..];
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let a = if hex.len() == 8 { byte(6)? } else { 255 };
    Some([
        byte(0)? as f32 / 255.0,
        byte(2)? as f32 / 255.0,
        byte(4)? as f32 / 255.0,
        a as f32 / 255.0,
    ])
}

/// RGBA floats as `#rrggbbaa` (the engine turns it into Mango's 0xRRGGBBAA).
pub fn rgba_to_hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        b(c[0]),
        b(c[1]),
        b(c[2]),
        b(c[3])
    )
}

/// `layout_name` of a "Default Layout" tag_rule value.
pub fn layout_of(tag_rule: &str) -> Option<String> {
    tag_rule.split(',').find_map(|pair| {
        let (k, v) = pair.split_once(':')?;
        (k.trim() == "layout_name").then(|| v.trim().to_string())
    })
}

/// The "Default Layout" value for `layout`, keeping the other items of the
/// current `id:*` rule (master_count, ...).
pub fn default_layout_value(current: Option<&str>, layout: &str) -> String {
    let mut parts: Vec<(String, String)> = current
        .filter(|c| c.split(',').any(|p| p.trim() == "id:*"))
        .map(|c| {
            c.split(',')
                .filter_map(|p| p.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec![("id".into(), "*".into())]);
    match parts.iter_mut().find(|(k, _)| k == "layout_name") {
        Some(p) => p.1 = layout.to_string(),
        None => parts.push(("layout_name".into(), layout.to_string())),
    }
    parts
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join(",")
}

/// Optional form field: empty means `None`.
pub fn parse_optional<T: FromStr>(what: &str, text: &str) -> Result<Option<T>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse()
        .map(Some)
        .map_err(|_| anyhow!("{} must be a number, got {:?}", what, t))
}

/// 1-based number of `rules[index]` among the rules of `kinds`, as the
/// CLI's `rule list` / `monitor list` count them.
pub fn rule_number(rules: &[Rule], kinds: &[&str], index: usize) -> Option<usize> {
    if !kinds.contains(&rules.get(index)?.kind.as_str()) {
        return None;
    }
    Some(
        rules[..=index]
            .iter()
            .filter(|r| kinds.contains(&r.kind.as_str()))
            .count(),
    )
}

/// "2026-10-31 14:02" from a backup's RFC 3339 time.
pub fn short_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_round_trip() {
        let c = color_to_rgba("0x89b4faff").unwrap();
        assert_eq!(rgba_to_hex(c), "#89b4faff");
        assert_eq!(rgba_to_hex(color_to_rgba("#313244").unwrap()), "#313244ff");
        assert!(color_to_rgba("blue").is_none());
    }

    #[test]
    fn default_layout_keeps_other_items() {
        assert_eq!(default_layout_value(None, "tile"), "id:*,layout_name:tile");
        assert_eq!(
            default_layout_value(Some("id:*,layout_name:tile,master_count:2"), "scroller"),
            "id:*,layout_name:scroller,master_count:2"
        );
        // a rule for one tag is not the default layout: start fresh
        assert_eq!(
            default_layout_value(Some("id:2,layout_name:tile"), "grid"),
            "id:*,layout_name:grid"
        );
        assert_eq!(
            layout_of("id:*,layout_name:monocle").as_deref(),
            Some("monocle")
        );
        assert_eq!(layout_of("id:*"), None);
    }

    #[test]
    fn optional_numbers() {
        assert_eq!(parse_optional::<u32>("width", " ").unwrap(), None);
        assert_eq!(parse_optional::<i64>("x", "-1920").unwrap(), Some(-1920));
        assert!(parse_optional::<u32>("width", "wide").is_err());
    }

    #[test]
    fn rule_numbers_count_only_their_kinds() {
        let rules = vec![
            Rule::parse("window_rule", "app_id:mpv,is_floating:1").unwrap(),
            Rule::parse("monitor_rule", "name:eDP-1,scale:1.5").unwrap(),
            Rule::parse("layer_rule", "layer_name:rofi,no_blur:1").unwrap(),
        ];
        let kinds = &["window_rule", "layer_rule"];
        assert_eq!(rule_number(&rules, kinds, 0), Some(1));
        assert_eq!(rule_number(&rules, kinds, 1), None);
        assert_eq!(rule_number(&rules, kinds, 2), Some(2));
        assert_eq!(rule_number(&rules, &["monitor_rule"], 1), Some(1));
    }
}
