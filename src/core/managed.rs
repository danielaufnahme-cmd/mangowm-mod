//! MangoWM managed configuration (`mango-mod.conf`) and source line injector.
//!
//! Everything is written as real Mango config lines, grouped under section
//! comments so the file can be read back without any side data:
//!
//! ```text
//! # [settings]   key=value lines (incl. the "Default Layout" tag_rule)
//! # [autostart]  exec_once= / exec= lines
//! # [rules]      window_rule= / tag_rule= / layer_rule= / monitor_rule= / device_rule=
//! # [binds]      key_mode= and bind*= lines
//! # [other]      any other repeatable lines we keep but do not edit (env=, mousebind=, ...)
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use super::fsutil::read_text;
use super::items::{Bind, Rule, RULE_KINDS};
use super::parser::{is_list_keyword, strip_comment, strip_quotes};

pub const HEADER: &str =
    "# MangoWM Mod managed settings. Generated file: manual edits may be overwritten.\n";
pub const SOURCE_MARKER: &str =
    "# MangoWM Mod: keep this line last so its settings take precedence";

const SEC_SETTINGS: &str = "# [settings]";
const SEC_AUTOSTART: &str = "# [autostart]";
const SEC_RULES: &str = "# [rules]";
const SEC_BINDS: &str = "# [binds]";
const SEC_OTHER: &str = "# [other]";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ManagedState {
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub binds: Vec<Bind>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// `exec_once=` commands (run once when Mango starts).
    #[serde(default)]
    pub exec_once: Vec<String>,
    /// `exec=` commands (run on every config reload).
    #[serde(default)]
    pub exec: Vec<String>,
    /// Other repeatable lines kept verbatim, by keyword (env, mousebind, ...).
    #[serde(default)]
    pub lists: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    /// File without section comments (written by mangowm-mod 1.0).
    Legacy,
    Settings,
    Items,
}

fn split_line(raw: &str) -> Option<(String, String)> {
    let line = strip_comment(raw);
    let (k, v) = line.split_once('=')?;
    let k = k.trim();
    if k.is_empty() {
        return None;
    }
    Some((k.to_string(), strip_quotes(v.trim()).to_string()))
}

/// Parse managed-file text back into its state.
pub fn parse_managed(text: &str) -> ManagedState {
    let mut state = ManagedState::default();
    let mut section = Section::Legacy;
    let mut mode = "default".to_string();

    for raw in text.lines() {
        let t = raw.trim();
        match t {
            SEC_SETTINGS => {
                section = Section::Settings;
                continue;
            }
            SEC_AUTOSTART | SEC_RULES | SEC_BINDS | SEC_OTHER => {
                section = Section::Items;
                continue;
            }
            _ => {}
        }
        let Some((key, value)) = split_line(raw) else {
            continue;
        };

        // Everything in [settings] (incl. the "Default Layout" tag_rule) is a setting.
        if section == Section::Settings || !is_list_keyword(&key) {
            state.settings.insert(key, value);
            continue;
        }

        if key == "key_mode" {
            mode = value;
        } else if key.starts_with("bind") {
            if let Some(b) = Bind::parse(&key, &value, &mode) {
                state.binds.push(b);
            }
        } else if RULE_KINDS.contains(&key.as_str()) {
            if section == Section::Legacy
                && key == "tag_rule"
                && !state.settings.contains_key("tag_rule")
            {
                // 1.0 files stored the default layout as a plain tag_rule line
                state.settings.insert(key, value);
            } else if let Ok(r) = Rule::parse(&key, &value) {
                state.rules.push(r);
            } else {
                state.lists.entry(key).or_default().push(value);
            }
        } else if key == "exec_once" {
            state.exec_once.push(value);
        } else if key == "exec" {
            state.exec.push(value);
        } else {
            state.lists.entry(key).or_default().push(value);
        }
    }

    state
}

pub fn read_managed<P: AsRef<Path>>(path: P) -> ManagedState {
    let path = path.as_ref();
    if !path.is_file() {
        return ManagedState::default();
    }
    match read_text(path) {
        Ok(t) => parse_managed(&t),
        Err(_) => ManagedState::default(),
    }
}

pub fn render(state: &ManagedState) -> String {
    let mut out = String::from(HEADER);

    if !state.settings.is_empty() {
        out.push('\n');
        out.push_str(SEC_SETTINGS);
        out.push('\n');
        for (k, v) in &state.settings {
            out.push_str(&format!("{}={}\n", k, v));
        }
    }

    if !state.exec_once.is_empty() || !state.exec.is_empty() {
        out.push('\n');
        out.push_str(SEC_AUTOSTART);
        out.push('\n');
        for cmd in &state.exec_once {
            out.push_str(&format!("exec_once={}\n", cmd));
        }
        for cmd in &state.exec {
            out.push_str(&format!("exec={}\n", cmd));
        }
    }

    if !state.rules.is_empty() {
        out.push('\n');
        out.push_str(SEC_RULES);
        out.push('\n');
        for r in &state.rules {
            out.push_str(&r.to_config_line());
            out.push('\n');
        }
    }

    if !state.binds.is_empty() {
        out.push('\n');
        out.push_str(SEC_BINDS);
        out.push('\n');
        // key_mode is a state line: binds after it belong to that mode, so name
        // the mode explicitly in case the user's config left another one active.
        let mut current: Option<&str> = None;
        for b in &state.binds {
            if current != Some(b.mode.as_str()) {
                out.push_str(&format!("key_mode={}\n", b.mode));
                current = Some(b.mode.as_str());
            }
            out.push_str(&b.to_config_line());
            out.push('\n');
        }
        if current != Some("default") {
            out.push_str("key_mode=default\n");
        }
    }

    let other: Vec<_> = state.lists.iter().filter(|(_, v)| !v.is_empty()).collect();
    if !other.is_empty() {
        out.push('\n');
        out.push_str(SEC_OTHER);
        out.push('\n');
        for (kw, vals) in other {
            for v in vals {
                out.push_str(&format!("{}={}\n", kw, v));
            }
        }
    }

    out
}

/// The `source=` value that loads `managed_path` from `config_path`.
///
/// Mango resolves `./x` against the directory of the config it loaded, so a
/// relative path is only used when both files share a directory.
pub fn source_value(config_path: &Path, managed_path: &Path) -> String {
    let same_dir = match (config_path.parent(), managed_path.parent()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    match managed_path.file_name() {
        Some(name) if same_dir => format!("./{}", name.to_string_lossy()),
        _ => managed_path.display().to_string(),
    }
}

/// True when `text` already sources the managed file (any spacing, `source` or `source_optional`).
pub fn has_source_line(text: &str, config_path: &Path, managed_path: &Path) -> bool {
    let wanted = [
        source_value(config_path, managed_path),
        managed_path.display().to_string(),
    ];
    text.lines().any(|ln| {
        let line = strip_comment(ln);
        match line.split_once('=') {
            Some((k, v)) => {
                let k = k.trim();
                let v = strip_quotes(v.trim());
                (k == "source" || k == "source_optional") && wanted.iter().any(|w| w == v)
            }
            None => false,
        }
    })
}

/// Return `text` with the managed source line appended, or `None` if it is already there.
pub fn with_source_line(text: &str, config_path: &Path, managed_path: &Path) -> Option<String> {
    if has_source_line(text, config_path, managed_path) {
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

    body.push_str(&format!(
        "{}\nsource={}\n",
        SOURCE_MARKER,
        source_value(config_path, managed_path)
    ));
    Some(body)
}
