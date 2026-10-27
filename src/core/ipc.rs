//! MangoWM IPC command execution.

use anyhow::Result;
use std::process::Command;

pub struct MangoIpc {
    pub dry_run: bool,
}

impl MangoIpc {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    pub fn reload(&self) -> Result<()> {
        if self.dry_run || std::env::var("MANGOWM_MOD_DRY_RUN").is_ok() {
            return Ok(());
        }

        // Try mango-msg reload if running
        let _ = Command::new("mango-msg").arg("reload").output();
        Ok(())
    }
}
