//! Plain helpers behind the widgets (no GTK here, so they are unit-tested).

use anyhow::{anyhow, Result};
use std::str::FromStr;

use mangowm_mod::core::items::{Monitor, Rule};
use mangowm_mod::core::values::format_float;
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

/// Common output scales offered in the Monitors form.
pub const SCALE_PRESETS: &[f64] = &[1.0, 1.25, 1.5, 1.75, 2.0];

/// Labels of the scale dropdown: "Not set", the presets, then "Custom".
pub fn scale_labels() -> Vec<String> {
    let mut v = vec!["Not set".to_string()];
    v.extend(
        SCALE_PRESETS
            .iter()
            .map(|s| format!("{}  ({}%)", format_float(*s), (s * 100.0).round())),
    );
    v.push("Custom".to_string());
    v
}

/// Dropdown position for a scale: 0 = not set, then the presets, last = custom.
pub fn scale_choice(scale: Option<f64>) -> usize {
    match scale {
        None => 0,
        Some(s) => SCALE_PRESETS
            .iter()
            .position(|p| (p - s).abs() < 1e-6)
            .map(|i| i + 1)
            .unwrap_or(SCALE_PRESETS.len() + 1),
    }
}

/// The scale a dropdown position (and the custom value) stands for.
pub fn scale_from_choice(choice: usize, custom: f64) -> Option<f64> {
    match choice {
        0 => None,
        i if i <= SCALE_PRESETS.len() => Some(SCALE_PRESETS[i - 1]),
        _ => Some((custom * 100.0).round() / 100.0),
    }
}

/// Labels of the transform dropdown: "Not set", then `rr` 0-7.
pub const TRANSFORM_LABELS: &[&str] = &[
    "Not set",
    "Normal",
    "Rotate 90°",
    "Rotate 180°",
    "Rotate 270°",
    "Flipped",
    "Flipped, 90°",
    "Flipped, 180°",
    "Flipped, 270°",
];

/// The first managed `monitor_rule` per output name, in file order, with how
/// many later rules for the same name Mango ignores.
pub fn monitor_entries(rules: &[Rule]) -> Vec<(Rule, usize)> {
    let mut out: Vec<(Rule, usize)> = Vec::new();
    for r in rules.iter().filter(|r| r.kind == "monitor_rule") {
        match out
            .iter_mut()
            .find(|(f, _)| f.get("name").is_some() && f.get("name") == r.get("name"))
        {
            Some(e) => e.1 += 1,
            None => out.push((r.clone(), 0)),
        }
    }
    out
}

/// One-line description of a monitor rule, e.g. "2560 × 1440 @ 239.97 Hz · Scale 1".
pub fn monitor_summary(rule: &Rule) -> String {
    let m = Monitor::from_rule(rule);
    let mut parts: Vec<String> = Vec::new();
    if m.disable {
        parts.push("Disabled".into());
    }
    match (rule.get("width"), rule.get("height"), rule.get("refresh")) {
        (Some(w), Some(h), Some(r)) => parts.push(format!("{} × {} @ {} Hz", w, h, r)),
        (Some(w), Some(h), None) => parts.push(format!("{} × {}", w, h)),
        (_, _, Some(r)) => parts.push(format!("{} Hz", r)),
        _ => {}
    }
    if let Some(s) = rule.get("scale") {
        parts.push(format!("Scale {}", s));
    }
    if rule.get("x").is_some() || rule.get("y").is_some() {
        parts.push(format!(
            "Position {}, {}",
            rule.get("x").unwrap_or("0"),
            rule.get("y").unwrap_or("0")
        ));
    }
    if let Some(t) = m.transform.filter(|t| *t > 0) {
        if let Some(l) = TRANSFORM_LABELS.get(t as usize + 1) {
            parts.push(l.to_string());
        }
    }
    match m.vrr {
        Some(true) => parts.push("VRR on".into()),
        Some(false) => parts.push("VRR off".into()),
        None => {}
    }
    if m.primary {
        parts.push("Primary".into());
    }
    let known = [
        "name", "width", "height", "refresh", "scale", "x", "y", "rr", "vrr", "disable", "primary",
    ];
    parts.extend(
        rule.fields
            .iter()
            .filter(|(k, _)| !known.contains(&k.as_str()))
            .map(|(k, v)| format!("{}:{}", k, v)),
    );
    if parts.is_empty() {
        "Mango's defaults".into()
    } else {
        parts.join("  ·  ")
    }
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

    #[test]
    fn scale_choices_round_trip() {
        assert_eq!(scale_labels().len(), SCALE_PRESETS.len() + 2);
        assert_eq!(scale_labels()[2], "1.25  (125%)");
        assert_eq!(scale_choice(None), 0);
        assert_eq!(scale_choice(Some(1.0)), 1);
        assert_eq!(scale_choice(Some(1.5)), 3);
        assert_eq!(scale_choice(Some(2.0)), 5);
        assert_eq!(scale_choice(Some(1.7)), 6);
        for i in 1..=SCALE_PRESETS.len() {
            assert_eq!(scale_choice(scale_from_choice(i, 0.0)), i);
        }
        assert_eq!(scale_from_choice(0, 3.0), None);
        assert_eq!(scale_from_choice(6, 1.6666), Some(1.67));
    }

    #[test]
    fn monitor_entries_one_per_output() {
        let rules: Vec<Rule> = [
            ("monitor_rule", "name:DP-4,scale:1.5"),
            ("window_rule", "app_id:mpv,is_floating:1"),
            ("monitor_rule", "name:DP-4,scale:2"),
            ("monitor_rule", "name:eDP-1,scale:1"),
            ("monitor_rule", "name:DP-4,scale:3"),
        ]
        .iter()
        .map(|(k, v)| Rule::parse(k, v).unwrap())
        .collect();
        let e = monitor_entries(&rules);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].0.get("scale"), Some("1.5"));
        assert_eq!(e[0].1, 2);
        assert_eq!(e[1].0.get("name"), Some("eDP-1"));
        assert_eq!(e[1].1, 0);
    }

    #[test]
    fn monitor_summaries() {
        let r = Rule::parse(
            "monitor_rule",
            "name:DP-4,width:2560,height:1440,refresh:239.97,scale:1",
        )
        .unwrap();
        assert_eq!(monitor_summary(&r), "2560 × 1440 @ 239.97 Hz  ·  Scale 1");
        let r = Rule::parse("monitor_rule", "name:X,rr:1,vrr:1,hdr:1").unwrap();
        assert_eq!(monitor_summary(&r), "Rotate 90°  ·  VRR on  ·  hdr:1");
        let r = Rule::parse("monitor_rule", "name:X").unwrap();
        assert_eq!(monitor_summary(&r), "Mango's defaults");
    }
}
