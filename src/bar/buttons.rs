//! Custom bar buttons: an emoji or picture that plays a sound, runs a
//! command and/or opens an app on a left or right click.
//!
//! The bar config only calls `mangowm-mod bar button run <id> left|right`;
//! that helper reads the saved button from bar.json, so editing an action
//! needs no bar restart (only the icon and the placement do).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::login::{cli_binary, shell_quote};
use crate::core::fsutil::find_in_path;
use crate::core::manager::detect_data_dir;

/// Bar module of button `id`: `custom/btn-<id>`.
pub const MODULE_PREFIX: &str = "custom/btn-";

/// What one click does; all parts run together, any can be empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClickAction {
    /// Audio file played with pw-play or paplay.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound: String,
    /// Shell command (`sh -c`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// Desktop file id of an app, e.g. `org.gnome.Nautilus.desktop`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app: String,
}

impl ClickAction {
    pub fn is_empty(&self) -> bool {
        self.sound.is_empty() && self.command.trim().is_empty() && self.app.is_empty()
    }

    /// "sound, command" for the GUI.
    pub fn summary(&self) -> String {
        let parts: Vec<&str> = [
            (!self.sound.is_empty()).then_some("sound"),
            (!self.command.trim().is_empty()).then_some("command"),
            (!self.app.is_empty()).then_some("app"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BarButton {
    /// Short id; the module is `custom/btn-<id>`.
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub emoji: String,
    /// Picture copied into `<data_dir>/bar-icons` (Waybar only); wins over
    /// the emoji in Waybar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
    /// Tooltip (Waybar); the button's text when it has no icon.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub left: ClickAction,
    #[serde(default)]
    pub right: ClickAction,
    /// Right click runs the left click's action.
    #[serde(default = "default_true")]
    pub right_same: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Click {
    Left,
    Right,
}

impl Click {
    pub fn name(self) -> &'static str {
        match self {
            Click::Left => "left",
            Click::Right => "right",
        }
    }
}

impl BarButton {
    pub fn module(&self) -> String {
        format!("{}{}", MODULE_PREFIX, self.id)
    }

    /// The action a click runs ("same as left" applied).
    pub fn action(&self, click: Click) -> &ClickAction {
        match click {
            Click::Right if !self.right_same => &self.right,
            _ => &self.left,
        }
    }

    /// Text the bar shows when no picture is drawn: emoji, else label.
    pub fn text(&self) -> &str {
        if !self.emoji.is_empty() {
            &self.emoji
        } else if !self.label.is_empty() {
            &self.label
        } else {
            "?"
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id) {
            bail!("invalid button id {:?}", self.id);
        }
        for (what, text) in [("emoji", &self.emoji), ("label", &self.label)] {
            // both bars read braces as placeholders
            if text.contains(['{', '}', '\n']) {
                bail!("button {} must not contain braces or line breaks", what);
            }
        }
        if self.emoji.chars().count() > 16 {
            bail!("button emoji is too long");
        }
        if self.label.chars().count() > 64 {
            bail!("button label must be 64 characters or less");
        }
        Ok(())
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Button id of a `custom/btn-<id>` module.
pub fn button_id(module: &str) -> Option<&str> {
    module.strip_prefix(MODULE_PREFIX).filter(|id| valid_id(id))
}

/// Next free id: 1, 2, 3, ...
pub fn next_id(buttons: &[BarButton]) -> String {
    (1..)
        .map(|n: u32| n.to_string())
        .find(|id| !buttons.iter().any(|b| &b.id == id))
        .unwrap_or_default()
}

/// Where button pictures are kept, so they survive the original moving.
pub fn icons_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bar-icons")
}

/// Copy a picture into [`icons_dir`] as `<id>-<file name>`; returns the copy.
pub fn import_picture(data_dir: &Path, id: &str, src: &Path) -> Result<PathBuf> {
    if !src.is_file() {
        bail!("{} is not a file", src.display());
    }
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .context("picture has no file name")?;
    let dir = icons_dir(data_dir);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let dest = dir.join(format!("{}-{}", id, name));
    if dest != src {
        fs::copy(src, &dest)
            .with_context(|| format!("failed to copy {} to {}", src.display(), dest.display()))?;
    }
    Ok(dest)
}

/// The bar's click command: `<cli> bar button run <id> <click>`, with
/// `--data-dir` when bar.json is not in the default place.
pub fn click_command(data_dir: Option<&Path>, id: &str, click: Click) -> String {
    let mut cmd = format!(
        "{} bar button run {} {}",
        shell_quote(&cli_binary().display().to_string()),
        id,
        click.name()
    );
    if let Some(d) = data_dir.filter(|d| *d != detect_data_dir(None)) {
        cmd.push_str(&format!(" --data-dir {}", shell_quote(&d.display().to_string())));
    }
    cmd
}

/// Programs a click uses (found in PATH; tests pass fakes).
#[derive(Debug, Clone, Default)]
pub struct Tools {
    /// pw-play or paplay.
    pub player: Option<PathBuf>,
    /// gtk-launch (takes a desktop id).
    pub launcher: Option<PathBuf>,
    /// `gio launch <desktop file>` when gtk-launch is missing.
    pub gio: Option<PathBuf>,
    pub shell: PathBuf,
}

impl Tools {
    pub fn detect() -> Self {
        Self {
            player: find_in_path("pw-play").or_else(|| find_in_path("paplay")),
            launcher: find_in_path("gtk-launch"),
            gio: find_in_path("gio"),
            shell: PathBuf::from("/bin/sh"),
        }
    }
}

/// `<data dir>/applications/<id>` in the XDG data dirs.
fn desktop_file(id: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = dirs::data_dir().into_iter().collect();
    let sys = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    dirs.extend(sys.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.into_iter()
        .map(|d| d.join("applications").join(id))
        .find(|p| p.is_file())
}

/// The programs one click starts, as argv lists. A part whose tool is
/// missing is reported in the second list.
pub fn click_argvs(action: &ClickAction, tools: &Tools) -> (Vec<Vec<String>>, Vec<String>) {
    let (mut out, mut missing) = (Vec::new(), Vec::new());
    if !action.sound.is_empty() {
        match &tools.player {
            Some(p) => out.push(vec![p.display().to_string(), action.sound.clone()]),
            None => missing.push("no pw-play or paplay to play the sound".to_string()),
        }
    }
    if !action.command.trim().is_empty() {
        out.push(vec![
            tools.shell.display().to_string(),
            "-c".to_string(),
            action.command.clone(),
        ]);
    }
    if !action.app.is_empty() {
        let id = action.app.trim_end_matches(".desktop");
        if let Some(l) = &tools.launcher {
            out.push(vec![l.display().to_string(), id.to_string()]);
        } else if let (Some(g), Some(file)) = (&tools.gio, desktop_file(&action.app)) {
            out.push(vec![
                g.display().to_string(),
                "launch".to_string(),
                file.display().to_string(),
            ]);
        } else {
            missing.push(format!("cannot open {}: no gtk-launch or gio", action.app));
        }
    }
    (out, missing)
}

/// Start the click's sound, command and app together, detached (own
/// process group, no waiting), so the bar is never blocked.
pub fn run_click(button: &BarButton, click: Click, tools: &Tools) -> Result<Vec<u32>> {
    let (argvs, missing) = click_argvs(button.action(click), tools);
    let mut pids = Vec::new();
    let mut errors = missing;
    for argv in argvs {
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        match cmd.spawn() {
            Ok(child) => pids.push(child.id()),
            Err(e) => errors.push(format!("{}: {}", argv[0], e)),
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("; "));
    }
    Ok(pids)
}
