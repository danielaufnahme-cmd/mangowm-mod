//! MangoWM configuration parser (`config.conf`).

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static ASSIGN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([^=]+?)\s*=\s*(.*)$").unwrap());
// `$name` or `${name}`; names start with a letter or `_` (docs: configuration/basics.md)
static VAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});

/// Keywords that may repeat (each line adds an item instead of replacing a value).
/// Names checked against MangoWM 0.18.4 `parse_option()`; `bind` also takes the
/// flag forms `bindl`, `bindr`, ... (see [`is_list_keyword`]).
pub const LIST_KEYWORDS: &[&str] = &[
    "bind",
    "mousebind",
    "axisbind",
    "gesturebind",
    "switchbind",
    "window_rule",
    "window_rule_once",
    "tag_rule",
    "layer_rule",
    "monitor_rule",
    "device_rule",
    "env",
    "exec_once",
    "exec",
    "source",
    "source_optional",
    "key_mode",
];

/// True for repeatable keywords, including `bind` with flag letters (`bindl`, `bindsr`, ...).
pub fn is_list_keyword(key: &str) -> bool {
    LIST_KEYWORDS.contains(&key)
        || key
            .strip_prefix("bind")
            .is_some_and(|f| !f.is_empty() && f.chars().all(|c| "slrpc".contains(c)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedConfig {
    pub entries: Vec<Entry>,
    pub variables: HashMap<String, String>,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

impl ParsedConfig {
    pub fn values(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for e in &self.entries {
            map.insert(e.key.clone(), e.value.clone());
        }
        map
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        for e in self.entries.iter().rev() {
            if e.key == key {
                return Some(&e.value);
            }
        }
        None
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.key == key)
            .map(|e| e.value.as_str())
            .collect()
    }
}

/// Remove a comment the way Mango does: a line whose first character is `#`
/// is a comment, and an inline `#` starts a comment only when it follows
/// whitespace and is not inside single or double quotes.
pub fn strip_comment(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return String::new();
    }
    let mut quote: Option<char> = None;
    let mut prev_space = false;
    for (i, c) in trimmed.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == '#' && prev_space => return trimmed[..i].trim().to_string(),
            None => {}
        }
        prev_space = c.is_whitespace();
    }
    trimmed.to_string()
}

/// Mango strips one pair of matching surrounding quotes from values.
pub fn strip_quotes(value: &str) -> &str {
    let b = value.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

pub fn resolve_vars(text: &str, variables: &HashMap<String, String>) -> String {
    if !text.contains('$') || variables.is_empty() {
        return text.to_string();
    }

    VAR_RE
        .replace_all(text, |caps: &regex::Captures| {
            let name = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map_or("", |m| m.as_str());
            if let Some(val) = variables.get(name) {
                val.clone()
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

pub fn expand_path<P: AsRef<Path>>(raw: &str, base_dir: P) -> PathBuf {
    let raw = raw.trim();
    let expanded = if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(stripped)
        } else {
            PathBuf::from(raw)
        }
    } else {
        PathBuf::from(raw)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        base_dir.as_ref().join(expanded)
    }
}

pub struct Parser {
    pub result: ParsedConfig,
    exclude: HashSet<PathBuf>,
    follow_sources: bool,
    visited: HashSet<PathBuf>,
}

impl Parser {
    pub fn new(exclude: HashSet<PathBuf>, follow_sources: bool) -> Self {
        Self {
            result: ParsedConfig::default(),
            exclude,
            follow_sources,
            visited: HashSet::new(),
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => path.to_path_buf(),
        };

        if self.visited.contains(&canonical) || self.exclude.contains(&canonical) {
            return;
        }

        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.result
                    .warnings
                    .push(format!("cannot read {}: {}", path.display(), e));
                return;
            }
        };

        self.visited.insert(canonical);
        self.result.files.push(path.display().to_string());
        self.parse_text(&text, path.to_string_lossy().as_ref());
    }

    pub fn parse_text(&mut self, text: &str, path: &str) {
        let base_dir = Path::new(path).parent().unwrap_or_else(|| Path::new("."));

        for (lineno, raw) in text.lines().enumerate() {
            let line = strip_comment(raw);
            if line.is_empty() {
                continue;
            }

            if let Some(caps) = ASSIGN_RE.captures(&line) {
                let key = caps.get(1).unwrap().as_str().trim();
                let value = strip_quotes(caps.get(2).unwrap().as_str().trim());

                if key == "var" {
                    if let Some((vname, vval)) = value.split_once(',') {
                        self.result
                            .variables
                            .insert(vname.trim().to_string(), vval.trim().to_string());
                    }
                    continue;
                }

                let resolved_val = resolve_vars(value, &self.result.variables);

                if key == "source" || key == "source_optional" {
                    if !self.follow_sources {
                        self.result.entries.push(Entry {
                            key: key.to_string(),
                            value: resolved_val,
                            file: path.to_string(),
                            line: lineno + 1,
                        });
                        continue;
                    }
                    let target = expand_path(&resolved_val, base_dir);
                    if key == "source_optional" && !target.exists() {
                        continue;
                    }
                    self.parse_file(target);
                    continue;
                }

                self.result.entries.push(Entry {
                    key: key.to_string(),
                    value: resolved_val,
                    file: path.to_string(),
                    line: lineno + 1,
                });
            }
        }
    }
}

pub fn parse_config<P: AsRef<Path>>(path: P, exclude: &[PathBuf]) -> ParsedConfig {
    let mut exclude_set = HashSet::new();
    for p in exclude {
        if let Ok(c) = p.canonicalize() {
            exclude_set.insert(c);
        } else {
            exclude_set.insert(p.clone());
        }
    }
    let mut parser = Parser::new(exclude_set, true);
    parser.parse_file(path);
    parser.result
}

pub fn parse_text(text: &str, path: &str) -> ParsedConfig {
    let mut parser = Parser::new(HashSet::new(), false);
    parser.parse_text(text, path);
    parser.result
}
