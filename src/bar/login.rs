//! Start the MangoWM Mod bar when Mango starts, and retire the old bar line.
//!
//! The login line is an `exec_once=<mangowm-mod> bar start` autostart entry
//! in mango-mod.conf, staged like any other autostart change. A bare
//! `exec_once=waybar` in config.conf would start a second bar; it can be
//! commented out (after a backup) so only the Mod's bar starts.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::backup::BackupStore;
use crate::core::fsutil::{atomic_write, find_in_path, read_text};
use crate::core::manager::{detect_data_dir, ConfigManager};
use crate::core::parser::{strip_comment, strip_quotes};

const CLI_NAME: &str = "mangowm-mod";

/// Comment put after a line [`disable_unmanaged_bars`] turned off.
pub const DISABLED_NOTE: &str = "# disabled by MangoWM Mod: its bar starts from mango-mod.conf";

/// The CLI binary: `$PATH` first, else next to the running program (the GUI
/// is installed beside it), else the bare name.
pub(crate) fn cli_binary() -> PathBuf {
    if let Some(p) = find_in_path(CLI_NAME) {
        return p;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join(CLI_NAME)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(CLI_NAME))
}

pub(crate) fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%,".contains(c))
    {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// `exec_once` command that starts the saved bar: `<cli> bar start`, with
/// `--data-dir` when the bar settings are not in the default place.
pub fn login_command(data_dir: &Path) -> String {
    let mut cmd = format!(
        "{} bar start",
        shell_quote(&cli_binary().display().to_string())
    );
    if data_dir != detect_data_dir(None) {
        cmd.push_str(&format!(
            " --data-dir {}",
            shell_quote(&data_dir.display().to_string())
        ));
    }
    cmd
}

/// True for an autostart command that starts the MangoWM Mod bar.
pub fn is_login_command(cmd: &str) -> bool {
    let mut words = cmd.split_whitespace();
    let first = words.next().unwrap_or("").trim_matches('\'');
    Path::new(first).file_name().is_some_and(|n| n == CLI_NAME)
        && words.next() == Some("bar")
        && words.next() == Some("start")
}

/// Stage adding (or removing) the login line in mango-mod.conf.
/// Returns true when something was staged.
pub fn set_login_start(mgr: &mut ConfigManager, enabled: bool) -> Result<bool> {
    let want = login_command(&mgr.data_dir);
    let ours: Vec<(usize, String)> = mgr
        .staged()
        .exec_once
        .iter()
        .enumerate()
        .filter(|(_, c)| is_login_command(c))
        .map(|(i, c)| (i, c.clone()))
        .collect();
    if enabled && ours.len() == 1 && ours[0].1 == want {
        return Ok(false);
    }
    if !enabled && ours.is_empty() {
        return Ok(false);
    }
    // remove from the end, so the numbers of the others stay right
    for (i, _) in ours.iter().rev() {
        mgr.remove_autostart(i + 1)?;
    }
    if enabled {
        mgr.add_autostart(&want, false)?;
    }
    Ok(true)
}

/// Where the login line is: written to mango-mod.conf, staged only, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    Applied,
    Staged,
    Removing,
    Off,
}

pub fn login_state(mgr: &ConfigManager) -> LoginState {
    let staged = mgr.staged().exec_once.iter().any(|c| is_login_command(c));
    let applied = mgr.managed().exec_once.iter().any(|c| is_login_command(c));
    match (staged, applied) {
        (true, true) => LoginState::Applied,
        (true, false) => LoginState::Staged,
        (false, true) => LoginState::Removing,
        (false, false) => LoginState::Off,
    }
}

/// True for a config line that starts Waybar or Mangobar on its own
/// (`exec_once=waybar`, `exec=mangobar -c ...`), not the Mod's files.
fn is_unmanaged_bar_line(line: &str) -> bool {
    let body = strip_comment(line);
    let Some((key, value)) = body.split_once('=') else {
        return false;
    };
    if !matches!(key.trim(), "exec_once" | "exec") {
        return false;
    }
    let value = strip_quotes(value.trim());
    if value.contains("mango-mod") {
        return false;
    }
    let first = value.split_whitespace().next().unwrap_or("");
    Path::new(first)
        .file_name()
        .is_some_and(|n| n == "waybar" || n == "mangobar")
}

/// Lines of config.conf (1-based number, text) that start another bar.
pub fn unmanaged_bar_lines(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| is_unmanaged_bar_line(l))
        .map(|(i, l)| (i + 1, l.to_string()))
        .collect()
}

/// Comment out the lines [`unmanaged_bar_lines`] finds in `config_path`,
/// after a backup (restorable from Backups). Returns the backup ID and the
/// number of lines, or `None` when there was nothing to change.
pub fn disable_unmanaged_bars(
    config_path: &Path,
    backups: &BackupStore,
) -> Result<Option<(String, usize)>> {
    if !config_path.is_file() {
        return Ok(None);
    }
    let text = read_text(config_path)?;
    let found = unmanaged_bar_lines(&text);
    if found.is_empty() {
        return Ok(None);
    }
    // never change the file without a backup
    let id = backups
        .create(
            "Before turning off the old bar autostart",
            &[config_path.to_path_buf()],
        )
        .context("backup failed, config.conf was not changed")?;
    let mut out = String::with_capacity(text.len() + 80);
    for (i, line) in text.lines().enumerate() {
        if found.iter().any(|(n, _)| *n == i + 1) {
            out.push_str(&format!("# {}  {}\n", line.trim(), DISABLED_NOTE));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !text.ends_with('\n') {
        out.pop();
    }
    atomic_write(config_path, &out)?;
    Ok(Some((id, found.len())))
}
