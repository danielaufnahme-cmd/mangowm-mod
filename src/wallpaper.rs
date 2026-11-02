//! Mango's wallpaper: one managed `exec_once=swaybg -m fill -i '<path>'` line
//! (shared by presets and the Wallpaper page), plus the pictures to pick from.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::core::fsutil::find_in_path;
use crate::core::manager::{ApplyResult, ConfigManager};

/// Wallpapers bundled with the app live in `<data dir>/wallpapers` (install.sh).
pub const WALLPAPER_DIR: &str = "wallpapers";

/// The owner's own wallpaper folders, relative to $HOME.
pub const USER_DIRS: &[&str] = &["Pictures/Wallpapers", "Pictures/wallpapers"];

const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

/// Folder with the bundled wallpapers: the installed copy in
/// `<data dir>/wallpapers`, else the source checkout this binary was built from.
pub fn bundled_dir(data_dir: &Path) -> PathBuf {
    let installed = data_dir.join(WALLPAPER_DIR);
    if installed.is_dir() {
        return installed;
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(WALLPAPER_DIR)
}

/// `exec_once` command that shows `path` (Mango runs it with `sh -c`).
pub fn swaybg_command(path: &Path) -> String {
    let p = path.display().to_string().replace('\'', r"'\''");
    format!("swaybg -m fill -i '{}'", p)
}

pub fn is_swaybg(cmd: &str) -> bool {
    cmd.split_whitespace().next() == Some("swaybg")
}

/// The image a swaybg command shows (`-i PATH`, `-i 'PATH'`, `--image=PATH`).
pub fn swaybg_image(cmd: &str) -> Option<PathBuf> {
    if !is_swaybg(cmd) {
        return None;
    }
    let rest = cmd
        .split_once(" -i ")
        .or_else(|| cmd.split_once(" --image "))
        .or_else(|| cmd.split_once(" --image="))
        .map(|(_, r)| r.trim_start())?;
    let path = if let Some(q) = rest.strip_prefix('\'') {
        // single-quoted, with '\'' for a quote inside
        let mut out = String::new();
        let mut s = q;
        loop {
            let end = s.find('\'')?;
            out.push_str(&s[..end]);
            s = &s[end + 1..];
            match s.strip_prefix(r"\''") {
                Some(more) => {
                    out.push('\'');
                    s = more;
                }
                None => break,
            }
        }
        out
    } else if let Some(q) = rest.strip_prefix('"') {
        q.split('"').next()?.to_string()
    } else {
        rest.split_whitespace().next()?.to_string()
    };
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// The wallpaper Mango shows: the managed swaybg line if there is one, else
/// the one in the user's own config.conf.
pub fn mango_wallpaper(mgr: &ConfigManager) -> Option<PathBuf> {
    mgr.managed()
        .exec_once
        .iter()
        .find_map(|c| swaybg_image(c))
        .or_else(|| {
            mgr.user_config()
                .get_all("exec_once")
                .into_iter()
                .find_map(swaybg_image)
        })
}

/// Stage `path` as the wallpaper: replaces every managed swaybg line.
pub fn stage_wallpaper(mgr: &mut ConfigManager, path: &Path) -> Result<()> {
    while let Some(i) = mgr.staged().exec_once.iter().position(|c| is_swaybg(c)) {
        mgr.remove_autostart(i + 1)?;
    }
    mgr.add_autostart(&swaybg_command(path), false)
}

/// Show `path` now: start a new swaybg, then stop the old ones a moment later
/// (so the screen never flashes empty). Runs detached; nothing waits for it.
fn set_live_wallpaper(path: &Path) -> std::io::Result<()> {
    let script = "old=$(pgrep -u \"$(id -u)\" -x swaybg); \
                  setsid -f swaybg -m fill -i \"$1\" >/dev/null 2>&1; \
                  sleep 0.4; [ -z \"$old\" ] || kill $old";
    let mut child = Command::new("sh")
        .args(["-c", script, "sh"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// After an apply that staged `path`: warn about anything that keeps it from
/// showing, and switch it live when Mango confirmed the reload.
pub fn finish_wallpaper(mgr: &ConfigManager, res: &mut ApplyResult, path: &Path) {
    if find_in_path("swaybg").is_none() {
        res.warnings
            .push("swaybg is not installed, so the wallpaper is not shown".into());
        return;
    }
    if let Some(own) = mgr
        .user_config()
        .get_all("exec_once")
        .into_iter()
        .find(|c| is_swaybg(c))
    {
        res.warnings.push(format!(
            "config.conf also starts a wallpaper (exec_once={}); at login both run and either \
             may end up on top. Remove that line to always get the chosen wallpaper",
            own
        ));
    }
    if res.live_applied {
        if let Err(e) = set_live_wallpaper(path) {
            res.warnings
                .push(format!("could not switch the wallpaper now: {}", e));
        }
    }
}

/// Make `path` Mango's wallpaper (also applies anything else that is staged).
pub fn set_wallpaper(mgr: &mut ConfigManager, path: &Path) -> Result<ApplyResult> {
    if !path.is_file() {
        bail!("{} is not a file", path.display());
    }
    let path = std::path::absolute(path)?;
    stage_wallpaper(mgr, &path)?;
    let mut res = mgr.apply()?;
    finish_wallpaper(mgr, &mut res, &path);
    Ok(res)
}

/// Plasma's desktop wallpaper from `plasma-org.kde.plasma.desktop-appletsrc`:
/// the first `Image=` under `[Containments][N][Wallpaper][org.kde.image][General]`.
pub fn plasma_wallpaper(appletsrc: &str) -> Option<PathBuf> {
    let mut in_image = false;
    for line in appletsrc.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_image = line.starts_with("[Containments][")
                && line.ends_with("][Wallpaper][org.kde.image][General]");
            continue;
        }
        if !in_image {
            continue;
        }
        if let Some(v) = line.strip_prefix("Image=") {
            let v = v.trim();
            let path = percent_decode(v.strip_prefix("file://").unwrap_or(v));
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| s.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Plasma's current wallpaper for the user at `home`, if it is a picture file.
pub fn plasma_wallpaper_in(home: &Path) -> Option<PathBuf> {
    let rc = home
        .join(".config")
        .join("plasma-org.kde.plasma.desktop-appletsrc");
    let path = plasma_wallpaper(&std::fs::read_to_string(rc).ok()?)?;
    path.is_file().then_some(path)
}

/// The wallpaper to match colours to: Mango's, else Plasma's.
pub fn current_wallpaper(mgr: &ConfigManager, home: &Path) -> Option<PathBuf> {
    mango_wallpaper(mgr)
        .filter(|p| p.is_file())
        .or_else(|| plasma_wallpaper_in(home))
}

#[derive(Debug, Clone, PartialEq)]
pub struct WallpaperEntry {
    pub path: PathBuf,
    /// File name without extension.
    pub name: String,
    pub bundled: bool,
    pub in_plasma: bool,
    pub in_mango: bool,
}

fn is_picture(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn pictures_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| is_picture(p)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Everything the Wallpaper page shows: Plasma's current one first, then the
/// bundled wallpapers, then the user's own folders (each picture once), and
/// Mango's current one if it is in none of those.
pub fn collect_wallpapers(
    bundled_dir: &Path,
    home: &Path,
    plasma: Option<&Path>,
    mango: Option<&Path>,
) -> Vec<WallpaperEntry> {
    let key = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let plasma_key = plasma.map(key);
    let mango_key = mango.map(key);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut push = |path: PathBuf, bundled: bool, out: &mut Vec<WallpaperEntry>| {
        let k = key(&path);
        if !seen.insert(k.clone()) {
            return;
        }
        out.push(WallpaperEntry {
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bundled,
            in_plasma: plasma_key.as_ref() == Some(&k),
            in_mango: mango_key.as_ref() == Some(&k),
            path,
        });
    };
    if let Some(p) = plasma.filter(|p| is_picture(p)) {
        push(p.to_path_buf(), false, &mut out);
    }
    for p in pictures_in(bundled_dir) {
        push(p, true, &mut out);
    }
    for d in USER_DIRS {
        for p in pictures_in(&home.join(d)) {
            push(p, false, &mut out);
        }
    }
    if let Some(p) = mango.filter(|p| is_picture(p)) {
        push(p.to_path_buf(), false, &mut out);
    }
    out
}
