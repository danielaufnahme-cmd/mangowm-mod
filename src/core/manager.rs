//! MangoWM ConfigManager: coordinate reading, staging, writing, backups, and hot reload.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::backup::BackupStore;
use super::fsutil::{atomic_write, read_text};
use super::ipc::{MangoIpc, ReloadStatus};
use super::items::{Bind, Rule};
use super::managed::{
    has_source_line, read_managed, render, source_value, with_source_line, ManagedState,
};
use super::parser::{parse_config, ParsedConfig};
use super::schema::{find_option, search_options};
use super::values::format_option_value;

/// Where Mango installs its fallback config (`$sysconfdir/mango/config.conf`,
/// documented as /etc/mango/config.conf). The others are extra places some
/// packages may use; they are only read, never written.
pub const SYSTEM_CONFIG_CANDIDATES: &[&str] = &[
    "/etc/mango/config.conf",
    "/usr/local/etc/mango/config.conf",
    "/usr/etc/mango/config.conf",
    "/etc/xdg/mango/config.conf",
    "/usr/share/mango/config.conf",
    "/usr/local/share/mango/config.conf",
    "/run/current-system/sw/etc/mango/config.conf",
];

/// Written when there is no config.conf and no system default to copy.
/// The binds are the essential ones from Mango's own default config.conf,
/// so the session stays usable.
pub const MINIMAL_CONFIG: &str = "\
# MangoWM config created by MangoWM Mod (no system default config was found).
# Reference: https://mangowm.github.io/  Edit freely; mangowm-mod only adds the
# source= line at the end and writes its own settings to mango-mod.conf.

bind=SUPER,r,reload_config
bind=Alt,Return,spawn,foot
bind=Alt,space,spawn,rofi -show drun
bind=ALT,q,killclient,
bind=SUPER,m,quit
";

#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// True only when Mango confirmed the reload.
    pub live_applied: bool,
    pub reload: Option<ReloadStatus>,
    pub backup_id: Option<String>,
    /// Set when config.conf did not exist and was created (copied from this source).
    pub created_config: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StagedFile {
    managed_path: PathBuf,
    state: ManagedState,
}

pub struct ConfigManager {
    pub config_path: PathBuf,
    pub managed_path: PathBuf,
    pub data_dir: PathBuf,
    pub backups: BackupStore,
    pub ipc: MangoIpc,

    user: ParsedConfig,
    /// What is in mango-mod.conf now.
    managed: ManagedState,
    /// Working copy with staged changes (saved to `staged.json` between runs).
    staged: ManagedState,
}

/// Mango always reads `$HOME/.config/mango/config.conf` (it does not look at
/// XDG_CONFIG_HOME; MangoWM 0.18.4 `src/config/load.c`), unless started with `-c`.
pub fn detect_config(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(env) = std::env::var_os("MANGOWM_MOD_CONFIG") {
        return PathBuf::from(env);
    }
    dirs::home_dir()
        .map(|h| h.join(".config").join("mango").join("config.conf"))
        .unwrap_or_else(|| PathBuf::from(".config/mango/config.conf"))
}

/// Data directory: `--data-dir`, `$MANGOWM_MOD_DATA`, else `$XDG_DATA_HOME/mangowm-mod`
/// (`~/.local/share/mangowm-mod`).
pub fn detect_data_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(d) = explicit {
        return d.to_path_buf();
    }
    if let Some(env) = std::env::var_os("MANGOWM_MOD_DATA") {
        return PathBuf::from(env);
    }
    dirs::data_dir()
        .map(|d| d.join("mangowm-mod"))
        .unwrap_or_else(|| PathBuf::from(".local/share/mangowm-mod"))
}

/// First readable system default config, if any.
pub fn find_system_config() -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("MANGOWM_MOD_SYSTEM_CONFIG") {
        let p = PathBuf::from(env);
        return p.is_file().then_some(p);
    }
    SYSTEM_CONFIG_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

impl ConfigManager {
    pub fn new(
        config_path: Option<&Path>,
        managed_path: Option<&Path>,
        data_dir: Option<&Path>,
        dry_run: bool,
    ) -> Result<Self> {
        let config_path = detect_config(config_path);
        let parent_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

        let managed_path = match managed_path {
            Some(m) => m.to_path_buf(),
            None => parent_dir.join("mango-mod.conf"),
        };
        let data_dir = detect_data_dir(data_dir);

        let mut mgr = Self {
            config_path,
            managed_path,
            backups: BackupStore::new(&data_dir),
            data_dir,
            ipc: MangoIpc::new(dry_run),
            user: ParsedConfig::default(),
            managed: ManagedState::default(),
            staged: ManagedState::default(),
        };
        mgr.reload()?;
        Ok(mgr)
    }

    fn staged_path(&self) -> PathBuf {
        self.data_dir.join("staged.json")
    }

    /// Re-read config.conf, mango-mod.conf and any staged changes.
    pub fn reload(&mut self) -> Result<()> {
        self.user = parse_config(&self.config_path, std::slice::from_ref(&self.managed_path));
        self.managed = read_managed(&self.managed_path);
        self.staged = self.managed.clone();
        if let Ok(text) = std::fs::read_to_string(self.staged_path()) {
            if let Ok(file) = serde_json::from_str::<StagedFile>(&text) {
                if file.managed_path == self.managed_path {
                    self.staged = file.state;
                }
            }
        }
        Ok(())
    }

    fn save_staged(&self) -> Result<()> {
        let path = self.staged_path();
        if self.staged == self.managed {
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("failed to clear {}", path.display()))?;
            }
            return Ok(());
        }
        let file = StagedFile {
            managed_path: self.managed_path.clone(),
            state: self.staged.clone(),
        };
        atomic_write(&path, &serde_json::to_string_pretty(&file)?)
    }

    /// The user's own config as parsed (excluding mango-mod.conf).
    pub fn user_config(&self) -> &ParsedConfig {
        &self.user
    }

    pub fn staged(&self) -> &ManagedState {
        &self.staged
    }

    pub fn managed(&self) -> &ManagedState {
        &self.managed
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(val) = self.staged.settings.get(key) {
            return Some(val.clone());
        }
        self.user.get(key).map(|s| s.to_string())
    }

    /// Stage a setting. The key must be a known Mango option; the value is
    /// converted to the form Mango parses (0/1, 0xRRGGBBAA, ...). Returns it.
    pub fn set(&mut self, key: &str, value: &str) -> Result<String> {
        let opt = find_option(key).ok_or_else(|| {
            let hints: Vec<String> = search_options(key)
                .into_iter()
                .take(5)
                .map(|o| o.key)
                .collect();
            if hints.is_empty() {
                anyhow!(
                    "unknown MangoWM option {:?} (run `mangowm-mod schema` for the list)",
                    key
                )
            } else {
                anyhow!(
                    "unknown MangoWM option {:?}; did you mean: {}?",
                    key,
                    hints.join(", ")
                )
            }
        })?;
        let value = format_option_value(&opt.r#type, &opt.choices, value)
            .with_context(|| format!("invalid value for {}", key))?;
        self.staged.settings.insert(key.to_string(), value.clone());
        self.save_staged()?;
        Ok(value)
    }

    /// Stage removal of a managed setting (Mango then uses config.conf or its default).
    pub fn reset(&mut self, key: &str) -> Result<bool> {
        let removed = self.staged.settings.remove(key).is_some();
        self.save_staged()?;
        Ok(removed)
    }

    /// Drop all staged changes.
    pub fn discard(&mut self) -> Result<()> {
        self.staged = self.managed.clone();
        self.save_staged()
    }

    // ---- binds ----

    /// Binds in the user's own config (for conflict checks; key modes are not tracked).
    pub fn user_binds(&self) -> Vec<Bind> {
        self.user
            .entries
            .iter()
            .filter_map(|e| Bind::parse(&e.key, &e.value, "default"))
            .collect()
    }

    pub fn add_bind(&mut self, bind: Bind, replace: bool) -> Result<()> {
        bind.validate()?;
        if !bind.flags.contains('c') {
            if let Some(b) = self.user_binds().iter().find(|b| b.same_combo(&bind)) {
                bail!(
                    "{}+{} is already bound in your config ({}). Remove it there, or add the \
                     bind with the `c` flag (--flags c) to allow the conflict",
                    bind.mods_string(),
                    bind.key,
                    b.to_config_line()
                );
            }
        }
        if let Some(pos) = self.staged.binds.iter().position(|b| b.same_combo(&bind)) {
            if !replace {
                bail!(
                    "{}+{} is already managed by mangowm-mod (bind #{}); use --replace",
                    bind.mods_string(),
                    bind.key,
                    pos + 1
                );
            }
            self.staged.binds[pos] = bind;
        } else {
            self.staged.binds.push(bind);
        }
        self.save_staged()
    }

    /// Remove a managed bind by its 1-based number from `bind list`.
    pub fn remove_bind(&mut self, number: usize) -> Result<Bind> {
        if number == 0 || number > self.staged.binds.len() {
            bail!("no managed bind #{} (see `mangowm-mod bind list`)", number);
        }
        let b = self.staged.binds.remove(number - 1);
        self.save_staged()?;
        Ok(b)
    }

    // ---- rules & monitors ----

    pub fn add_rule(&mut self, rule: Rule) -> Result<()> {
        rule.validate()?;
        if self.staged.rules.contains(&rule) {
            bail!("this rule is already managed");
        }
        self.staged.rules.push(rule);
        self.save_staged()
    }

    /// Remove a managed rule by its 1-based number among rules of `kinds`.
    pub fn remove_rule(&mut self, kinds: &[&str], number: usize) -> Result<Rule> {
        let idx = self
            .staged
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| kinds.contains(&r.kind.as_str()))
            .nth(number.wrapping_sub(1))
            .map(|(i, _)| i)
            .ok_or_else(|| anyhow!("no managed rule #{} of that type", number))?;
        let r = self.staged.rules.remove(idx);
        self.save_staged()?;
        Ok(r)
    }

    /// `monitor_rule` lines in the user's own config that match `name`.
    /// Mango applies only the first matching monitor rule, and those come first.
    pub fn user_monitor_rules_for(&self, name: &str) -> Vec<String> {
        self.user
            .get_all("monitor_rule")
            .into_iter()
            .filter(|v| {
                Rule::parse("monitor_rule", v)
                    .ok()
                    .and_then(|r| r.get("name").map(|n| n == name))
                    .unwrap_or(false)
            })
            .map(|v| format!("monitor_rule={}", v))
            .collect()
    }

    // ---- autostart ----

    pub fn add_autostart(&mut self, command: &str, every_reload: bool) -> Result<()> {
        let cmd = command.trim();
        if cmd.is_empty() || cmd.contains(['\n', '\r']) {
            bail!("the command must be a single non-empty line");
        }
        let list = if every_reload {
            &mut self.staged.exec
        } else {
            &mut self.staged.exec_once
        };
        if list.iter().any(|c| c == cmd) {
            bail!("already in autostart: {}", cmd);
        }
        list.push(cmd.to_string());
        self.save_staged()
    }

    /// Remove an autostart entry by its 1-based number in `autostart list`
    /// (exec_once entries first, then exec).
    pub fn remove_autostart(&mut self, number: usize) -> Result<(String, bool)> {
        let once = self.staged.exec_once.len();
        let removed = if number >= 1 && number <= once {
            (self.staged.exec_once.remove(number - 1), false)
        } else if number > once && number <= once + self.staged.exec.len() {
            (self.staged.exec.remove(number - once - 1), true)
        } else {
            bail!(
                "no autostart entry #{} (see `mangowm-mod autostart list`)",
                number
            );
        };
        self.save_staged()?;
        Ok(removed)
    }

    // ---- staging ----

    pub fn is_dirty(&self) -> bool {
        self.staged != self.managed
    }

    /// Setting changes: (key, Some(new)) for set, (key, None) for reset.
    pub fn pending_changes(&self) -> Vec<(String, Option<String>)> {
        let mut changes = Vec::new();
        for (k, v) in &self.staged.settings {
            if self.managed.settings.get(k) != Some(v) {
                changes.push((k.clone(), Some(v.clone())));
            }
        }
        for k in self.managed.settings.keys() {
            if !self.staged.settings.contains_key(k) {
                changes.push((k.clone(), None));
            }
        }
        changes.sort();
        changes
    }

    /// Human-readable summary of staged bind/rule/autostart changes.
    pub fn pending_item_changes(&self) -> Vec<String> {
        let mut out = Vec::new();
        let diff = |name: &str, old: Vec<String>, new: Vec<String>, out: &mut Vec<String>| {
            for n in new.iter().filter(|n| !old.contains(n)) {
                out.push(format!("+ {} {}", name, n));
            }
            for o in old.iter().filter(|o| !new.contains(o)) {
                out.push(format!("- {} {}", name, o));
            }
        };
        let lines = |v: &[Bind]| v.iter().map(|b| b.to_config_line()).collect::<Vec<_>>();
        diff(
            "bind",
            lines(&self.managed.binds),
            lines(&self.staged.binds),
            &mut out,
        );
        let rl = |v: &[Rule]| v.iter().map(|r| r.to_config_line()).collect::<Vec<_>>();
        diff(
            "rule",
            rl(&self.managed.rules),
            rl(&self.staged.rules),
            &mut out,
        );
        diff(
            "exec_once",
            self.managed.exec_once.clone(),
            self.staged.exec_once.clone(),
            &mut out,
        );
        diff(
            "exec",
            self.managed.exec.clone(),
            self.staged.exec.clone(),
            &mut out,
        );
        out
    }

    /// Create config.conf if it does not exist, so mango-mod.conf gets loaded.
    /// Returns where its content came from. Never touches an existing file.
    fn ensure_main_config(&self) -> Result<Option<String>> {
        if self.config_path.exists() {
            return Ok(None);
        }
        let toml = self.config_path.with_extension("toml");
        if toml.is_file() {
            bail!(
                "{} does not exist but {} does. Mango would stop reading your TOML config if a \
                 config.conf appeared, so it was not created. Add this to the [global] table of \
                 your config.toml (before any [bind.*] or [[rule.*]] table) to load MangoWM Mod: \
                 source = [\"{}\"]",
                self.config_path.display(),
                toml.display(),
                source_value(&self.config_path, &self.managed_path)
            );
        }
        let (content, origin) = match find_system_config() {
            Some(sys) => (read_text(&sys)?, sys.display().to_string()),
            None => (
                MINIMAL_CONFIG.to_string(),
                "a minimal built-in config".to_string(),
            ),
        };
        atomic_write(&self.config_path, &content)?;
        Ok(Some(origin))
    }

    /// Write staged changes to mango-mod.conf, make sure config.conf sources it,
    /// back up both files first, then ask Mango to reload.
    pub fn apply(&mut self) -> Result<ApplyResult> {
        let mut result = ApplyResult::default();

        let source_ok = read_text(&self.config_path)
            .map(|t| has_source_line(&t, &self.config_path, &self.managed_path))
            .unwrap_or(false);
        if !self.is_dirty() && self.managed_path.is_file() && source_ok {
            result.ok = true;
            return Ok(result);
        }

        let content = render(&self.staged);

        let mut files_to_backup = vec![self.managed_path.clone()];
        if self.config_path.is_file() {
            files_to_backup.push(self.config_path.clone());
        }
        match self.backups.create("Apply changes", &files_to_backup) {
            Ok(id) => result.backup_id = Some(id),
            Err(e) => {
                // Never overwrite files we could not back up.
                bail!("backup failed, nothing was changed: {}", e);
            }
        }

        match self.ensure_main_config() {
            Ok(created) => result.created_config = created,
            Err(e) => result.errors.push(e.to_string()),
        }

        atomic_write(&self.managed_path, &content)?;

        if self.config_path.is_file() {
            let main_text = read_text(&self.config_path)?;
            if let Some(new_content) =
                with_source_line(&main_text, &self.config_path, &self.managed_path)
            {
                atomic_write(&self.config_path, &new_content).with_context(|| {
                    format!(
                        "failed to add the source line to {}",
                        self.config_path.display()
                    )
                })?;
            }
        }

        let status = self.ipc.reload();
        result.live_applied = status.is_live();
        if !status.is_live() && status != ReloadStatus::DryRun {
            result.warnings.push(status.to_string());
        }
        result.reload = Some(status);

        self.managed = self.staged.clone();
        self.save_staged()?;
        self.user = parse_config(&self.config_path, std::slice::from_ref(&self.managed_path));
        result.ok = result.errors.is_empty();
        Ok(result)
    }
}
