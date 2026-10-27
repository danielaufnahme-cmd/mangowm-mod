//! Process manager for Mangobar.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use super::generate::mangobar_dir;

pub fn start_mangobar<P: AsRef<Path>>(bar_dir: Option<P>) -> Result<()> {
    let dir = bar_dir
        .map(|p| p.as_ref().to_path_buf())
        .unwrap_or_else(mangobar_dir);
    let cfg = dir.join("config.jsonc");
    let css = dir.join("style.css");

    stop_mangobar();

    let child = Command::new("mangobar")
        .arg("-c")
        .arg(&cfg)
        .arg("-s")
        .arg(&css)
        .spawn();

    match child {
        Ok(_) => Ok(()),
        Err(e) => bail!("Failed to start mangobar: {}", e),
    }
}

pub fn stop_mangobar() {
    let _ = Command::new("killall").arg("mangobar").output();
}
