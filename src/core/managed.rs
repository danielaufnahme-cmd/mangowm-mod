//! MangoWM managed configuration generator and source line injector.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use super::fsutil::read_text;
use super::parser::{parse_text, LIST_KEYWORDS};

pub const HEADER: &str = "# MangoWM Mod managed settings. Generated file: manual edits may be overwritten.\n";
pub const SOURCE_MARKER: &str = "# MangoWM Mod: keep this line last so its settings take precedence";
pub const EXTRAS_MARKER: &str = "# ---- MangoWM Mod: binds, rules, monitors, autostart ----";
pub const EXTRAS_PREFIX: &str = "# mangowm-mod-extras: ";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ManagedState {
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub lists: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binds: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub monitors: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub autostart: BTreeMap<String, String>,
}

pub fn read_managed<P: AsRef<Path>>(path: P) -> ManagedState {
    let path = path.as_ref();
    if !path.is_file() {
        return ManagedState::default();
    }
    let text = match read_text(path) {
        Ok(t) => t,
        Err(_) => return ManagedState::default(),
    };

    let marker_line = format!("{}\n", EXTRAS_MARKER);
    let (head, tail) = match text.find(&marker_line) {
        Some(pos) => (&text[..pos], &text[pos + marker_line.len()..]),
        None => (text.as_str(), ""),
    };

    let parsed = parse_text(head, &path.to_string_lossy());
    let mut state = ManagedState::default();

    for e in &parsed.entries {
        if LIST_KEYWORDS.contains(&e.key.as_str()) {
            state.lists.entry(e.key.clone()).or_default().push(e.value.clone());
        } else {
            state.settings.insert(e.key.clone(), e.value.clone());
        }
    }

    for line in tail.lines() {
        if let Some(json_str) = line.strip_prefix(EXTRAS_PREFIX) {
            if let Ok(extras) = serde_json::from_str::<ManagedState>(json_str) {
                state.binds = extras.binds;
                state.rules = extras.rules;
                state.monitors = extras.monitors;
                state.autostart = extras.autostart;
            }
            break;
        }
    }

    state
}

pub fn render(state: &ManagedState) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');

    for (k, v) in &state.settings {
        out.push_str(&format!("{}={}\n", k, v));
    }

    for (kw, vals) in &state.lists {
        if !vals.is_empty() {
            out.push('\n');
            for v in vals {
                out.push_str(&format!("{}={}\n", kw, v));
            }
        }
    }

    if !state.binds.is_empty()
        || !state.rules.is_empty()
        || !state.monitors.is_empty()
        || !state.autostart.is_empty()
    {
        out.push('\n');
        out.push_str(EXTRAS_MARKER);
        out.push('\n');
        let extras_json = serde_json::to_string(state).unwrap_or_default();
        out.push_str(&format!("{}{}\n", EXTRAS_PREFIX, extras_json));
    }

    out
}

pub fn with_source_line(text: &str, _config_path: &Path, managed_path: &Path) -> Option<String> {
    let target_str = format!("source={}", managed_path.display());
    let rel_target = format!("source=./{}", managed_path.file_name().unwrap().to_string_lossy());

    if text.lines().any(|ln| ln.trim() == target_str || ln.trim() == rel_target) {
        return None;
    }

    let mut body = text.to_string();
    while body.ends_with("\n\n") {
        body.pop();
    }
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    if !body.is_empty() {
        body.push('\n');
    }

    body.push_str(&format!("{}\n{}\n", SOURCE_MARKER, rel_target));
    Some(body)
}
