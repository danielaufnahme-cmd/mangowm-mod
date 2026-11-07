//! The user's own presets: the current values of the keys the built-in presets
//! control (plus, optionally, the wallpaper), saved as
//! `<data dir>/presets/<id>.json` in the same format as a built-in [`Preset`],
//! so they apply through the same [`apply_preset`](super::apply_preset).

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

use super::{list_presets, Preset};
use crate::core::fsutil::atomic_write;
use crate::core::manager::ConfigManager;
use crate::core::schema::find_option;
use crate::core::values::format_option_value;
use crate::wallpaper::swaybg_image;

/// User presets live in `<data dir>/presets`.
pub const USER_PRESET_DIR: &str = "presets";

/// Id of the preset applied (or saved) last, in `<data dir>`.
const CURRENT_FILE: &str = "current-preset";

const MAX_NAME: usize = 40;

pub fn user_preset_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(USER_PRESET_DIR)
}

fn user_preset_file(data_dir: &Path, id: &str) -> PathBuf {
    user_preset_dir(data_dir).join(format!("{}.json", id))
}

/// Every key a built-in preset sets.
pub fn preset_keys() -> Vec<String> {
    let mut keys: Vec<String> = list_presets()
        .into_iter()
        .flat_map(|p| p.settings.into_keys())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The user's presets, by name. Files that do not parse are skipped.
pub fn list_user_presets(data_dir: &Path) -> Vec<Preset> {
    let Ok(entries) = std::fs::read_dir(user_preset_dir(data_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<Preset> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let mut p: Preset = serde_json::from_str(&text).ok()?;
            // the file name is the id, whatever the JSON says
            p.id = path.file_stem()?.to_str()?.to_string();
            p.user = true;
            Some(p)
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Built-in presets, then the user's.
pub fn all_presets(data_dir: &Path) -> Vec<Preset> {
    let mut all = list_presets();
    all.extend(list_user_presets(data_dir));
    all
}

/// A built-in or user preset by id or name.
pub fn find_any_preset(data_dir: &Path, name_or_id: &str) -> Option<Preset> {
    all_presets(data_dir)
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id))
}

fn find_user_preset(data_dir: &Path, name_or_id: &str) -> Result<Preset> {
    let p = find_any_preset(data_dir, name_or_id)
        .ok_or_else(|| anyhow!("preset {:?} not found", name_or_id))?;
    if !p.user {
        bail!("{} is a built-in preset and can't be changed", p.name);
    }
    Ok(p)
}

/// `name` trimmed; an error if it is empty, too long, or another preset's name
/// or id (`except` is the user preset being renamed).
fn check_name(data_dir: &Path, name: &str, except: Option<&str>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("the preset needs a name");
    }
    if name.chars().count() > MAX_NAME {
        bail!("the name is longer than {} characters", MAX_NAME);
    }
    let taken = all_presets(data_dir).into_iter().find(|p| {
        Some(p.id.as_str()) != except
            && (p.name.eq_ignore_ascii_case(name) || p.id.eq_ignore_ascii_case(name))
    });
    if let Some(p) = taken {
        bail!("there is already a preset called {}", p.name);
    }
    Ok(name.to_string())
}

/// File-name id for `name`: lowercase letters and digits joined by `-`,
/// numbered when another preset has it.
fn new_id(data_dir: &Path, name: &str) -> String {
    let mut base = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_alphanumeric() {
            base.push(c);
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    let base = match base.trim_end_matches('-') {
        "" => "preset",
        b => b,
    }
    .to_string();
    let taken: Vec<String> = all_presets(data_dir).into_iter().map(|p| p.id).collect();
    let free = |id: &String| !taken.iter().any(|t| t.eq_ignore_ascii_case(id));
    std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{}-{}", base, n)))
        .find(free)
        .unwrap_or(base)
}

/// The wallpaper the staged config shows: the staged swaybg line, else the
/// one in config.conf.
pub fn staged_wallpaper(mgr: &ConfigManager) -> Option<PathBuf> {
    mgr.staged()
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

/// The current (staged) values of [`preset_keys`]; keys with no value are left out.
fn capture(mgr: &ConfigManager, p: &mut Preset, wallpaper: bool) {
    p.settings = preset_keys()
        .into_iter()
        .filter_map(|k| mgr.get(&k).map(|v| (k, v)))
        .collect();
    p.wallpaper = if wallpaper {
        staged_wallpaper(mgr)
    } else {
        None
    };
}

fn write_user_preset(data_dir: &Path, p: &Preset) -> Result<()> {
    let path = user_preset_file(data_dir, &p.id);
    atomic_write(&path, &serde_json::to_string_pretty(p)?)
        .with_context(|| format!("failed to save preset {}", p.name))
}

/// Save the current settings as a new user preset called `name`; it becomes
/// the current preset.
pub fn save_user_preset(mgr: &ConfigManager, name: &str, wallpaper: bool) -> Result<Preset> {
    let name = check_name(&mgr.data_dir, name, None)?;
    let mut p = Preset {
        id: new_id(&mgr.data_dir, &name),
        name,
        description: "Saved from your settings".into(),
        user: true,
        ..Default::default()
    };
    capture(mgr, &mut p, wallpaper);
    write_user_preset(&mgr.data_dir, &p)?;
    set_current_preset(&mgr.data_dir, &p.id)?;
    Ok(p)
}

/// Replace a user preset's values with the current settings (the wallpaper
/// too, when it was saved with one); it becomes the current preset.
pub fn overwrite_user_preset(mgr: &ConfigManager, name_or_id: &str) -> Result<Preset> {
    let mut p = find_user_preset(&mgr.data_dir, name_or_id)?;
    let wallpaper = p.wallpaper.is_some();
    capture(mgr, &mut p, wallpaper);
    write_user_preset(&mgr.data_dir, &p)?;
    set_current_preset(&mgr.data_dir, &p.id)?;
    Ok(p)
}

/// Give a user preset a new name (its id and file stay the same).
pub fn rename_user_preset(data_dir: &Path, name_or_id: &str, new_name: &str) -> Result<Preset> {
    let mut p = find_user_preset(data_dir, name_or_id)?;
    p.name = check_name(data_dir, new_name, Some(&p.id))?;
    write_user_preset(data_dir, &p)?;
    Ok(p)
}

/// Delete a user preset's file. Built-in presets can't be deleted.
pub fn delete_user_preset(data_dir: &Path, name_or_id: &str) -> Result<Preset> {
    let p = find_user_preset(data_dir, name_or_id)?;
    let path = user_preset_file(data_dir, &p.id);
    std::fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))?;
    Ok(p)
}

/// Id of the preset applied or saved last, if any.
pub fn current_preset_id(data_dir: &Path) -> Option<String> {
    let id = std::fs::read_to_string(data_dir.join(CURRENT_FILE)).ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

pub fn set_current_preset(data_dir: &Path, id: &str) -> Result<()> {
    atomic_write(data_dir.join(CURRENT_FILE), &format!("{}\n", id))
}

/// The preset applied or saved last, if it still exists.
pub fn current_preset(data_dir: &Path) -> Option<Preset> {
    let id = current_preset_id(data_dir)?;
    all_presets(data_dir).into_iter().find(|p| p.id == id)
}

/// `value` in the form Mango parses, for comparing.
fn normalized(key: &str, value: &str) -> String {
    find_option(key)
        .and_then(|o| format_option_value(&o.r#type, &o.choices, value).ok())
        .unwrap_or_else(|| value.trim().to_string())
}

/// Whether the current (staged) settings differ from `p`: one of its values or
/// its wallpaper (`wallpaper`, from [`super::wallpaper_path`]) was changed.
pub fn preset_modified(mgr: &ConfigManager, p: &Preset, wallpaper: Option<&Path>) -> bool {
    let setting_changed = p.settings.iter().any(|(k, v)| {
        mgr.get(k)
            .map_or(true, |cur| normalized(k, &cur) != normalized(k, v))
    });
    let wallpaper_changed = wallpaper.is_some_and(|w| staged_wallpaper(mgr).as_deref() != Some(w));
    setting_changed || wallpaper_changed
}
