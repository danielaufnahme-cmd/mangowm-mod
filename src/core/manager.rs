//! MangoWM ConfigManager: coordinate reading, staging, writing, backups, and hot reload.

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::backup::BackupStore;
use super::fsutil::{atomic_write, read_text};
use super::ipc::MangoIpc;
use super::managed::{read_managed, render, with_source_line, ManagedState};
use super::parser::{parse_config, ParsedConfig};

#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub live_applied: bool,
    pub backup_id: Option<String>,
}

pub struct ConfigManager {
    pub config_path: PathBuf,
    pub managed_path: PathBuf,
    pub data_dir: PathBuf,
    pub backups: BackupStore,
    pub ipc: MangoIpc,

    user: ParsedConfig,
    managed: ManagedState,

    pending_set: HashMap<String, String>,
    pending_reset: HashSet<String>,
}

pub fn detect_config(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Ok(env) = std::env::var("MANGOWM_MOD_CONFIG") {
        return PathBuf::from(env);
    }
    dirs::home_dir()
        .map(|h| h.join(".config").join("mango").join("config.conf"))
        .unwrap_or_else(|| PathBuf::from(".config/mango/config.conf"))
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

        let managed_path = if let Some(m) = managed_path {
            m.to_path_buf()
        } else {
            parent_dir.join("mango-mod.conf")
        };

        let data_dir = if let Some(d) = data_dir {
            d.to_path_buf()
        } else if let Ok(env) = std::env::var("MANGOWM_MOD_DATA") {
            PathBuf::from(env)
        } else {
            dirs::data_local_dir()
                .map(|d| d.join("mangowm-mod"))
                .unwrap_or_else(|| PathBuf::from(".local/share/mangowm-mod"))
        };

        let backups = BackupStore::new(&data_dir);
        let ipc = MangoIpc::new(dry_run);

        let mut mgr = Self {
            config_path,
            managed_path,
            data_dir,
            backups,
            ipc,
            user: ParsedConfig::default(),
            managed: ManagedState::default(),
            pending_set: HashMap::new(),
            pending_reset: HashSet::new(),
        };

        mgr.reload()?;
        Ok(mgr)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.user = parse_config(&self.config_path, &[self.managed_path.clone()]);
        self.managed = read_managed(&self.managed_path);
        self.pending_set.clear();
        self.pending_reset.clear();
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if self.pending_reset.contains(key) {
            return self.user.get(key).map(|s| s.to_string());
        }
        if let Some(val) = self.pending_set.get(key) {
            return Some(val.clone());
        }
        if let Some(val) = self.managed.settings.get(key) {
            return Some(val.clone());
        }
        self.user.get(key).map(|s| s.to_string())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if key.is_empty() || value.contains('\n') {
            bail!("invalid key or value");
        }
        self.pending_reset.remove(key);
        self.pending_set.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn reset(&mut self, key: &str) {
        self.pending_set.remove(key);
        if self.managed.settings.contains_key(key) {
            self.pending_reset.insert(key.to_string());
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.pending_set.is_empty() || !self.pending_reset.is_empty()
    }

    pub fn pending_changes(&self) -> Vec<(String, Option<String>)> {
        let mut changes = Vec::new();
        for (k, v) in &self.pending_set {
            changes.push((k.clone(), Some(v.clone())));
        }
        for k in &self.pending_reset {
            changes.push((k.clone(), None));
        }
        changes.sort();
        changes
    }

    pub fn apply(&mut self) -> Result<ApplyResult> {
        let mut result = ApplyResult::default();
        if !self.is_dirty() && self.managed_path.is_file() {
            result.ok = true;
            return Ok(result);
        }

        let mut next_state = self.managed.clone();
        for (k, v) in self.pending_set.drain() {
            next_state.settings.insert(k, v);
        }
        for k in self.pending_reset.drain() {
            next_state.settings.remove(&k);
        }

        let content = render(&next_state);

        let mut files_to_backup = vec![self.managed_path.clone()];
        if self.config_path.is_file() {
            files_to_backup.push(self.config_path.clone());
        }

        match self.backups.create("Apply changes", &files_to_backup) {
            Ok(id) => result.backup_id = Some(id),
            Err(e) => result.errors.push(format!("Backup failed: {}", e)),
        }

        atomic_write(&self.managed_path, &content)?;

        if self.config_path.is_file() {
            if let Ok(main_text) = read_text(&self.config_path) {
                if let Some(new_content) =
                    with_source_line(&main_text, &self.config_path, &self.managed_path)
                {
                    let _ = atomic_write(&self.config_path, &new_content);
                }
            }
        }

        let _ = self.ipc.reload();
        result.live_applied = true;
        self.managed = next_state;
        result.ok = result.errors.is_empty();
        Ok(result)
    }
}
