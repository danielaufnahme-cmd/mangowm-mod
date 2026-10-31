//! MangoWM IPC: live reload through `mmsg`.
//!
//! Verified against MangoWM 0.18.4: `mmsg dispatch reload_config` connects to
//! the socket in `$MANGO_INSTANCE_SIGNATURE`, exits non-zero when it cannot,
//! and the compositor answers `{"success":true}` for a known dispatch.
//! Mango also reloads by itself when `auto_reload_config=1` (the default).

use std::fmt;
use std::process::{Command, Stdio};

use super::fsutil::find_in_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadStatus {
    /// Mango confirmed the reload.
    Reloaded,
    /// Dry run: nothing was sent.
    DryRun,
    /// `mmsg` is not installed / not in PATH.
    MmsgMissing,
    /// No running Mango session reachable from here.
    NotRunning(String),
    /// mmsg ran but Mango did not confirm.
    Failed(String),
}

impl ReloadStatus {
    pub fn is_live(&self) -> bool {
        matches!(self, ReloadStatus::Reloaded)
    }
}

impl fmt::Display for ReloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReloadStatus::Reloaded => write!(f, "MangoWM reloaded the config"),
            ReloadStatus::DryRun => write!(f, "dry run: MangoWM was not asked to reload"),
            ReloadStatus::MmsgMissing => write!(
                f,
                "`mmsg` was not found in PATH, so MangoWM was not reloaded. It ships with MangoWM; \
                 install MangoWM from your distribution or build it from source. Changes load on \
                 the next Mango start (or right away if auto_reload_config=1)"
            ),
            ReloadStatus::NotRunning(why) => write!(
                f,
                "MangoWM does not seem to be running here ({}); changes load the next time Mango \
                 starts. Run this from a terminal inside Mango to reload live",
                why
            ),
            ReloadStatus::Failed(why) => write!(f, "MangoWM did not confirm the reload: {}", why),
        }
    }
}

pub struct MangoIpc {
    pub dry_run: bool,
}

impl MangoIpc {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    fn is_dry_run(&self) -> bool {
        self.dry_run || std::env::var_os("MANGOWM_MOD_DRY_RUN").is_some()
    }

    /// Ask the running compositor to reload its config.
    pub fn reload(&self) -> ReloadStatus {
        self.dispatch("reload_config")
    }

    /// Run `mmsg dispatch <command>` and report what happened.
    pub fn dispatch(&self, command: &str) -> ReloadStatus {
        if self.is_dry_run() {
            return ReloadStatus::DryRun;
        }
        if find_in_path("mmsg").is_none() {
            return ReloadStatus::MmsgMissing;
        }
        if std::env::var_os("MANGO_INSTANCE_SIGNATURE").is_none() {
            return ReloadStatus::NotRunning("MANGO_INSTANCE_SIGNATURE is not set".into());
        }

        let output = Command::new("mmsg")
            .arg("dispatch")
            .arg(command)
            .stdin(Stdio::null())
            .output();
        match output {
            Err(e) => ReloadStatus::Failed(format!("could not run mmsg: {}", e)),
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let compact: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();
                if out.status.success() && compact.contains("\"success\":true") {
                    ReloadStatus::Reloaded
                } else if !out.status.success() && stderr.contains("connect") {
                    ReloadStatus::NotRunning(format!("cannot connect: {}", stderr.trim()))
                } else {
                    let msg = if stdout.trim().is_empty() {
                        stderr.trim()
                    } else {
                        stdout.trim()
                    };
                    ReloadStatus::Failed(if msg.is_empty() {
                        format!("mmsg exited with {}", out.status)
                    } else {
                        msg.to_string()
                    })
                }
            }
        }
    }
}
