//! MangoWM Mod CLI interface.

use anyhow::{anyhow, bail, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::bar::buttons::{import_picture, run_click, BarButton, Click, ClickAction, Tools};
use crate::bar::generate::{bar_dir, warnings, waybar_module_name, write_bar_files};
use crate::bar::login::{
    disable_unmanaged_bars, login_state, set_login_start, unmanaged_bar_lines, LoginState,
};
use crate::bar::model::{Backend, BarConfig, KNOWN_MODULES};
use crate::bar::process::{foreign_bars, log_file, start_bar, stop_bar, stop_foreign};
use crate::core::actions::get_actions;
use crate::core::items::{Bind, Monitor, Rule};
use crate::core::manager::{detect_data_dir, ApplyResult, ConfigManager};
use crate::core::schema::{get_schema_pages, search_options, OptionItem};
use crate::palette::{
    apply_bar_colors, palette_from_file, restart_bar_if_running, stage_window_colors, window_colors,
};
use crate::presets::{
    all_presets, apply_preset, current_preset_id, delete_user_preset, find_any_preset,
    overwrite_user_preset, rename_user_preset, save_user_preset,
};
use crate::wallpaper::{current_wallpaper, set_wallpaper};

/// Rule kinds `rule add` accepts (monitor rules go through `monitor`).
pub const RULE_ADD_KINDS: &[&str] = &[
    "window_rule",
    "window_rule_once",
    "tag_rule",
    "layer_rule",
    "device_rule",
];

#[derive(Parser, Debug)]
#[command(
    name = "mangowm-mod",
    about = "MangoWM settings manager and Mangobar/Waybar bar builder",
    version
)]
pub struct Cli {
    #[arg(long, global = true, help = "Path to mango config.conf")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, help = "Path to mango-mod.conf")]
    pub managed: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "Data directory for backups, staged changes and bar settings"
    )]
    pub data_dir: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "Write files but do not ask MangoWM to reload"
    )]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Show paths and staged changes")]
    Status,

    #[command(about = "Get a setting's current value")]
    Get { key: String },

    #[command(about = "Stage a setting (written by `apply`)")]
    Set { key: String, value: String },

    #[command(
        about = "Stage removing a managed setting (Mango falls back to config.conf or its default)"
    )]
    Reset { key: String },

    #[command(about = "Drop all staged changes")]
    Discard,

    #[command(about = "Write staged changes to mango-mod.conf and reload MangoWM")]
    Apply,

    #[command(about = "List MangoWM settings (all pages, one page, or a search)")]
    Schema {
        #[arg(help = "Page id, e.g. look or input")]
        page: Option<String>,
        #[arg(long, help = "Search keys and labels")]
        search: Option<String>,
    },

    #[command(about = "List, apply and save presets")]
    Preset {
        #[command(subcommand)]
        action: PresetCommands,
    },

    #[command(about = "Key binds managed by mangowm-mod")]
    Bind {
        #[command(subcommand)]
        action: BindCommands,
    },

    #[command(about = "Window, tag, layer and device rules")]
    Rule {
        #[command(subcommand)]
        action: RuleCommands,
    },

    #[command(about = "Monitor rules (resolution, position, scale, ...)")]
    Monitor {
        #[command(subcommand)]
        action: MonitorCommands,
    },

    #[command(about = "Mango's wallpaper (shown with swaybg)")]
    Wallpaper {
        #[command(subcommand)]
        action: WallpaperCommands,
    },

    #[command(about = "Colours matched to the wallpaper")]
    Colors {
        #[command(subcommand)]
        action: ColorsCommands,
    },

    #[command(about = "Programs started by MangoWM (exec_once / exec)")]
    Autostart {
        #[command(subcommand)]
        action: AutostartCommands,
    },

    #[command(about = "Mangobar / Waybar builder and controls")]
    Bar {
        #[command(subcommand)]
        action: BarCommands,
    },

    #[command(about = "List and restore backups")]
    Backup {
        #[command(subcommand)]
        action: BackupCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum PresetCommands {
    #[command(about = "List all available presets")]
    List,

    #[command(about = "Apply a preset by ID or name (applies at once)")]
    Apply { name: String },

    #[command(about = "Save the current settings as your own preset")]
    Save {
        name: String,
        #[arg(long, help = "Include the current wallpaper")]
        wallpaper: bool,
    },

    #[command(about = "Overwrite your preset with the current settings")]
    Update { name: String },

    #[command(about = "Rename your preset")]
    Rename { name: String, new_name: String },

    #[command(about = "Delete your preset (built-in presets can't be deleted)")]
    Delete { name: String },
}

#[derive(Subcommand, Debug)]
pub enum WallpaperCommands {
    #[command(about = "Set the wallpaper to a picture file (applies at once)")]
    Set { path: PathBuf },
}

#[derive(Subcommand, Debug)]
pub enum ColorsCommands {
    #[command(
        about = "Match window borders and/or the bar to a picture (default: both, the current wallpaper)"
    )]
    FromWallpaper {
        #[arg(long, help = "Window border colours (staged; written by `apply`)")]
        windows: bool,
        #[arg(
            long,
            help = "Bar colours (saved, files written, restarted if running)"
        )]
        bar: bool,
        #[arg(long, help = "Also apply the staged window colours at once")]
        apply: bool,
        #[arg(long, help = "Only print the palette; change nothing")]
        preview: bool,
        #[arg(help = "Picture file (default: Mango's wallpaper, else Plasma's)")]
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum BindCommands {
    #[command(about = "Add a bind: MODS KEY ACTION [ARGS...], e.g. SUPER+SHIFT Return spawn foot")]
    Add {
        #[arg(help = "Modifiers joined with + (SUPER, CTRL, SHIFT, ALT, HYPER) or NONE")]
        mods: String,
        #[arg(help = "XKB key name, e.g. Return, q, F1")]
        key: String,
        #[arg(help = "Mango action (see `bind actions`)")]
        action: String,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Action arguments"
        )]
        args: Vec<String>,
        #[arg(
            long,
            default_value = "",
            help = "Bind flags: s, l, r, p, c (e.g. l = also when locked)"
        )]
        flags: String,
        #[arg(long, default_value = "default", help = "Key mode the bind belongs to")]
        mode: String,
        #[arg(long, help = "Replace a managed bind with the same keys")]
        replace: bool,
    },

    #[command(about = "List managed binds")]
    List,

    #[command(about = "Remove a managed bind by its number in `bind list`")]
    Remove { number: usize },

    #[command(about = "List MangoWM actions")]
    Actions,
}

#[derive(Subcommand, Debug)]
pub enum RuleCommands {
    #[command(about = "Add a rule: KIND \"key:value,key:value\"")]
    Add {
        #[arg(help = "window_rule, window_rule_once, tag_rule, layer_rule or device_rule")]
        kind: String,
        #[arg(help = "e.g. \"app_id:mpv,is_floating:1\"")]
        fields: String,
    },

    #[command(about = "List managed rules")]
    List,

    #[command(about = "Remove a managed rule by its number in `rule list`")]
    Remove { number: usize },
}

#[derive(Subcommand, Debug)]
pub enum MonitorCommands {
    #[command(
        about = "Add or update the monitor rule for an output, e.g. eDP-1",
        long_about = "Add or update the monitor rule for an output, e.g. eDP-1. If a managed \
                      rule for that output exists it is updated in place: options you leave \
                      out keep their current value."
    )]
    Add {
        name: String,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        #[arg(long, help = "Refresh rate in Hz")]
        refresh: Option<f64>,
        #[arg(long, allow_negative_numbers = true)]
        x: Option<i64>,
        #[arg(long, allow_negative_numbers = true)]
        y: Option<i64>,
        #[arg(long)]
        scale: Option<f64>,
        #[arg(long, help = "Transform 0-7 (rotation / flip)")]
        transform: Option<u8>,
        #[arg(long, value_name = "true|false", help = "Variable refresh rate")]
        vrr: Option<bool>,
        #[arg(long, help = "Disable this output")]
        disable: bool,
        #[arg(long, help = "Make this the primary output")]
        primary: bool,
    },

    #[command(about = "List managed monitor rules")]
    List,

    #[command(about = "Remove a managed monitor rule by its number in `monitor list`")]
    Remove { number: usize },

    #[command(
        about = "Remove duplicate monitor rules, keeping the first per output (the one Mango uses)"
    )]
    Dedupe,
}

#[derive(Subcommand, Debug)]
pub enum AutostartCommands {
    #[command(about = "Add a command started with MangoWM (exec_once)")]
    Add {
        #[arg(long, help = "Run on every config reload (exec) instead of once")]
        every_reload: bool,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    #[command(about = "List managed autostart commands")]
    List,

    #[command(about = "Remove an autostart command by its number in `autostart list`")]
    Remove { number: usize },
}

/// Bar choices; every flag given is saved to `<data_dir>/bar.json`.
#[derive(Args, Debug, Default)]
pub struct BarFlags {
    #[arg(long, value_enum)]
    pub backend: Option<Backend>,
    #[arg(long)]
    pub height: Option<u32>,
    #[arg(long, allow_negative_numbers = true, help = "Corner radius; -1 = pill")]
    pub radius: Option<i32>,
    #[arg(long, help = "Waybar: top or bottom")]
    pub position: Option<String>,
    #[arg(long, help = "Background color, e.g. #1e1e2eeb")]
    pub bg: Option<String>,
    #[arg(long, help = "Text color")]
    pub fg: Option<String>,
    #[arg(long, help = "Accent color (active workspace)")]
    pub accent: Option<String>,
    #[arg(long)]
    pub font: Option<String>,
    #[arg(long)]
    pub font_size: Option<u32>,
    #[arg(long, help = "top, bottom or overlay")]
    pub layer: Option<String>,
    #[arg(long, help = "Left modules, comma-separated")]
    pub left: Option<String>,
    #[arg(long, help = "Center modules, comma-separated")]
    pub center: Option<String>,
    #[arg(long, help = "Right modules, comma-separated")]
    pub right: Option<String>,
    #[arg(
        long,
        value_name = "true|false",
        help = "Waybar: use the mango/* modules"
    )]
    pub waybar_mango_modules: Option<bool>,
}

impl BarFlags {
    /// (key for BarConfig::set, value) for every flag given; backend first.
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(b) = self.backend {
            out.push(("backend", b.binary().to_string()));
        }
        let mut add = |k: &'static str, v: Option<String>| {
            if let Some(v) = v {
                out.push((k, v));
            }
        };
        add("height", self.height.map(|v| v.to_string()));
        add("radius", self.radius.map(|v| v.to_string()));
        add("bg", self.bg.clone());
        add("fg", self.fg.clone());
        add("accent", self.accent.clone());
        add("font", self.font.clone());
        add("font-size", self.font_size.map(|v| v.to_string()));
        add("layer", self.layer.clone());
        add("position", self.position.clone());
        add("left", self.left.clone());
        add("center", self.center.clone());
        add("right", self.right.clone());
        add(
            "waybar-mango-modules",
            self.waybar_mango_modules.map(|v| v.to_string()),
        );
        out
    }
}

#[derive(Subcommand, Debug)]
pub enum BarCommands {
    #[command(about = "Write the bar's config.jsonc and style.css")]
    Generate {
        #[arg(short, long, help = "Output directory")]
        out: Option<PathBuf>,
        #[command(flatten)]
        flags: BarFlags,
    },

    #[command(about = "Generate the bar files and start the bar")]
    Start {
        #[arg(
            long,
            help = "First stop a Waybar/Mangobar that mangowm-mod did not start"
        )]
        replace: bool,
        #[command(flatten)]
        flags: BarFlags,
    },

    #[command(about = "Start the bar at login (staged exec_once in mango-mod.conf; run `apply`)")]
    Login {
        #[arg(value_name = "on|off", help = "Leave out to show the state")]
        state: Option<String>,
    },

    #[command(
        about = "Comment out exec_once=waybar/mangobar in config.conf (backup first), so only this bar starts"
    )]
    Migrate {
        #[arg(long, help = "Only show the lines")]
        check: bool,
    },

    #[command(about = "Stop the bar started by mangowm-mod")]
    Stop,

    #[command(about = "Show the saved bar settings")]
    Show,

    #[command(about = "Set a bar option: a field, a top-level key or <module>.<option>")]
    Set { key: String, value: String },

    #[command(about = "Remove a top-level or <module>.<option> override")]
    Unset { key: String },

    #[command(about = "List the bar modules")]
    Modules,

    #[command(
        about = "Custom bar buttons: an emoji or picture that plays a sound, runs a command or opens an app"
    )]
    Button {
        #[command(subcommand)]
        action: ButtonCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ButtonCommands {
    #[command(about = "List the custom buttons")]
    List,

    #[command(about = "Add a button (run `bar generate` or `bar start` to show it)")]
    Add {
        #[arg(long, help = "Emoji shown in the bar")]
        emoji: Option<String>,
        #[arg(
            long,
            help = "Picture shown in the bar (Waybar only; copied to the data dir)"
        )]
        picture: Option<PathBuf>,
        #[arg(long, help = "Tooltip (Waybar); the text when there is no icon")]
        label: Option<String>,
        #[arg(long, default_value = "right", help = "left, center or right")]
        section: String,
        #[arg(long, help = "Left click: audio file to play")]
        sound: Option<String>,
        #[arg(long, help = "Left click: shell command")]
        command: Option<String>,
        #[arg(long, help = "Left click: app desktop id, e.g. firefox.desktop")]
        app: Option<String>,
        #[arg(
            long,
            help = "Right click: audio file (any right-* flag gives right click its own action)"
        )]
        right_sound: Option<String>,
        #[arg(long, help = "Right click: shell command")]
        right_command: Option<String>,
        #[arg(long, help = "Right click: app desktop id")]
        right_app: Option<String>,
    },

    #[command(about = "Remove a button")]
    Remove { id: String },

    #[command(about = "Run a button's click action (what the bar calls)")]
    Run {
        id: String,
        #[arg(value_enum)]
        click: Click,
    },
}

#[derive(Subcommand, Debug)]
pub enum BackupCommands {
    #[command(about = "List backups")]
    List,

    #[command(about = "Restore a backup by ID and reload MangoWM")]
    Restore { id: String },
}

fn staged_hint() {
    println!(
        "Run {} to write it and reload MangoWM.",
        "mangowm-mod apply".bold()
    );
}

/// Print an apply result; returns false when it had errors.
fn print_apply(res: &ApplyResult) -> bool {
    if let Some(id) = &res.backup_id {
        println!("Backup created: {}", id);
    }
    if let Some(origin) = &res.created_config {
        println!("Created config.conf from {}", origin);
    }
    if let Some(status) = &res.reload {
        println!("Reload: {}", status);
    }
    for w in &res.warnings {
        if res.reload.as_ref().map(|s| s.to_string()) != Some(w.clone()) {
            println!("{}: {}", "warning".yellow(), w);
        }
    }
    for e in &res.errors {
        eprintln!("{}: {}", "error".red(), e);
    }
    if res.errors.is_empty() {
        println!("{}", "Changes applied.".green().bold());
    } else {
        eprintln!("{}", "Apply finished with errors.".red().bold());
    }
    res.errors.is_empty()
}

fn print_option(opt: &OptionItem) {
    let default = if opt.default.is_empty() {
        "-"
    } else {
        opt.default.as_str()
    };
    println!(
        "    {:<34} {:<8} {:<12} {}",
        opt.key.cyan(),
        opt.r#type.dimmed(),
        default,
        opt.label
    );
    if !opt.choices.is_empty() {
        println!("    {:<34} choices: {}", "", opt.choices.join(", "));
    }
}

fn load_bar(data_dir: &Path) -> Result<(BarConfig, PathBuf)> {
    let path = BarConfig::default_path(data_dir);
    Ok((BarConfig::load(&path)?, path))
}

/// Apply the given flags to the saved bar settings and save them.
fn bar_with_flags(data_dir: &Path, flags: &BarFlags) -> Result<BarConfig> {
    let (mut bar, path) = load_bar(data_dir)?;
    let pairs = flags.pairs();
    for (k, v) in &pairs {
        bar.set(k, v)?;
    }
    if !pairs.is_empty() {
        bar.save(&path)?;
    }
    Ok(bar)
}

fn print_bar_warnings(bar: &BarConfig) {
    for w in warnings(bar) {
        println!("{}: {}", "warning".yellow(), w);
    }
}

fn run_bar(action: BarCommands, data_dir: &Path) -> Result<()> {
    match action {
        BarCommands::Generate { out, flags } => {
            let bar = bar_with_flags(data_dir, &flags)?;
            let dir = write_bar_files(&bar, out.as_deref())?;
            println!(
                "{} files written to {}",
                bar.backend.binary(),
                dir.display().to_string().green()
            );
            print_bar_warnings(&bar);
        }

        BarCommands::Start { replace, flags } => {
            let bar = bar_with_flags(data_dir, &flags)?;
            let dir = write_bar_files(&bar, None::<&Path>)?;
            print_bar_warnings(&bar);
            let others = foreign_bars(&bar, data_dir);
            if replace {
                for msg in stop_foreign(&others)? {
                    println!("{}", msg);
                }
            } else {
                for b in &others {
                    println!(
                        "{}: another bar is running (PID {}: {}); use --replace to stop it",
                        "warning".yellow(),
                        b.pid,
                        b.command
                    );
                }
            }
            let pid = start_bar(&bar, Some(&dir), data_dir)?;
            println!(
                "{} started (PID {}) with {}; log: {}",
                bar.backend.binary().green(),
                pid,
                dir.display(),
                log_file(data_dir).display()
            );
        }

        BarCommands::Stop => {
            let (bar, _) = load_bar(data_dir)?;
            match stop_bar(&bar, None, data_dir)? {
                Some(msg) => println!("{}", msg),
                None => println!("No bar started by mangowm-mod is running."),
            }
        }

        BarCommands::Show => {
            let (bar, path) = load_bar(data_dir)?;
            println!("{}", "=== Bar settings ===".bold().cyan());
            println!("Saved in:   {}", path.display());
            println!("Files in:   {}", bar_dir(bar.backend).display());
            println!("{}", serde_json::to_string_pretty(&bar)?);
            print_bar_warnings(&bar);
        }

        BarCommands::Set { key, value } => {
            let (mut bar, path) = load_bar(data_dir)?;
            bar.set(&key, &value)?;
            bar.save(&path)?;
            println!("Bar: {} = {}", key.yellow(), value.green());
            println!(
                "Run {} or {} to use it.",
                "mangowm-mod bar generate".bold(),
                "mangowm-mod bar start".bold()
            );
        }

        BarCommands::Unset { key } => {
            let (mut bar, path) = load_bar(data_dir)?;
            if bar.unset(&key) {
                bar.save(&path)?;
                println!("Bar: removed {}", key.yellow());
            } else {
                println!("Bar: {} was not set", key);
            }
        }

        BarCommands::Login { .. } | BarCommands::Migrate { .. } => {
            unreachable!("needs the Mango config; handled in run_bar_login")
        }

        BarCommands::Modules => {
            println!("{}", "Bar modules (Waybar name in brackets):".bold().cyan());
            for (name, desc) in KNOWN_MODULES {
                let wb = waybar_module_name(name, true).unwrap_or_else(|| "-".into());
                println!("  {:<16} {:<44} [{}]", name.cyan(), desc, wb);
            }
            println!("  Also: clock#date, clock#time, battery#<device>, custom/<name>");
            println!("  Waybar without mango/* modules: workspaces -> ext/workspaces, window -> custom/mango-window; layout, keymode and keyboardlayout are left out");
        }

        BarCommands::Button { action } => run_bar_button(action, data_dir)?,
    }
    Ok(())
}

fn run_bar_button(action: ButtonCommands, data_dir: &Path) -> Result<()> {
    match action {
        ButtonCommands::Run { id, click } => {
            let (bar, _) = load_bar(data_dir)?;
            let button = bar
                .button(&id)
                .ok_or_else(|| anyhow!("no bar button {:?}", id))?;
            run_click(button, click, &Tools::detect())?;
        }
        ButtonCommands::List => {
            let (bar, _) = load_bar(data_dir)?;
            if bar.buttons.is_empty() {
                println!("No custom buttons.");
            }
            for b in &bar.buttons {
                let place = bar.button_section(&b.id).unwrap_or("not in the bar");
                let right = if b.right_same {
                    "same as left".to_string()
                } else {
                    b.right.summary()
                };
                println!(
                    "  {:<4} {:<6} {:<10} left: {}; right: {}{}",
                    b.id.cyan(),
                    b.text(),
                    place,
                    b.left.summary(),
                    right,
                    if b.picture.is_empty() {
                        ""
                    } else {
                        " (picture)"
                    }
                );
            }
        }
        ButtonCommands::Add {
            emoji,
            picture,
            label,
            section,
            sound,
            command,
            app,
            right_sound,
            right_command,
            right_app,
        } => {
            let (mut bar, path) = load_bar(data_dir)?;
            let right = ClickAction {
                sound: right_sound.unwrap_or_default(),
                command: right_command.unwrap_or_default(),
                app: right_app.unwrap_or_default(),
            };
            let button = BarButton {
                id: String::new(),
                emoji: emoji.unwrap_or_default(),
                picture: String::new(),
                label: label.unwrap_or_default(),
                left: ClickAction {
                    sound: sound.unwrap_or_default(),
                    command: command.unwrap_or_default(),
                    app: app.unwrap_or_default(),
                },
                right_same: right.is_empty(),
                right,
            };
            let id = bar.add_button(button, &section)?;
            if let Some(p) = picture {
                let copy = import_picture(data_dir, &id, &p)?;
                if let Some(b) = bar.button_mut(&id) {
                    b.picture = copy.display().to_string();
                }
            }
            bar.validate()?;
            bar.save(&path)?;
            println!("Added button {} ({} section)", id.green(), section);
            print_bar_warnings(&bar);
        }
        ButtonCommands::Remove { id } => {
            let (mut bar, path) = load_bar(data_dir)?;
            if !bar.remove_button(&id) {
                bail!("no bar button {:?}", id);
            }
            bar.save(&path)?;
            println!("Removed button {}", id.yellow());
        }
    }
    Ok(())
}

/// `bar login` / `bar migrate`: these change Mango's config files.
fn run_bar_login(action: BarCommands, mgr: &mut ConfigManager) -> Result<()> {
    match action {
        BarCommands::Login { state } => {
            let Some(state) = state else {
                let text = match login_state(mgr) {
                    LoginState::Applied => "on (in mango-mod.conf)",
                    LoginState::Staged => "on (staged, run `mangowm-mod apply`)",
                    LoginState::Removing => "off (staged, run `mangowm-mod apply`)",
                    LoginState::Off => "off",
                };
                println!("Start bar at login: {}", text);
                return Ok(());
            };
            let enabled =
                crate::core::values::parse_bool(&state).ok_or_else(|| anyhow!("use on or off"))?;
            let (mut bar, path) = load_bar(&mgr.data_dir)?;
            if bar.start_at_login != enabled {
                bar.start_at_login = enabled;
                bar.save(&path)?;
            }
            if set_login_start(mgr, enabled)? {
                println!(
                    "Staged: start bar at login {}",
                    if enabled { "on".green() } else { "off".red() }
                );
                staged_hint();
            } else {
                println!("Nothing to change.");
            }
        }
        BarCommands::Migrate { check } => {
            let text = std::fs::read_to_string(&mgr.config_path).unwrap_or_default();
            let lines = unmanaged_bar_lines(&text);
            if lines.is_empty() {
                println!("{} starts no other bar.", mgr.config_path.display());
                return Ok(());
            }
            for (n, l) in &lines {
                println!("  {}:{}  {}", mgr.config_path.display(), n, l.trim());
            }
            if check {
                return Ok(());
            }
            if let Some((id, n)) = disable_unmanaged_bars(&mgr.config_path, &mgr.backups)? {
                println!("Backup created: {}", id);
                println!(
                    "Commented out {} line(s); takes effect at the next login.",
                    n
                );
            }
        }
        _ => unreachable!("only login and migrate"),
    }
    Ok(())
}

pub fn run_cli(cli: Cli) -> Result<()> {
    let command = cli.command.unwrap_or(Commands::Status);

    // Bar commands work without any Mango config (but login/migrate edit it).
    let command = match command {
        Commands::Bar { action }
            if !matches!(
                action,
                BarCommands::Login { .. } | BarCommands::Migrate { .. }
            ) =>
        {
            let data_dir = detect_data_dir(cli.data_dir.as_deref());
            return run_bar(action, &data_dir);
        }
        other => other,
    };

    let mut mgr = ConfigManager::new(
        cli.config.as_deref(),
        cli.managed.as_deref(),
        cli.data_dir.as_deref(),
        cli.dry_run,
    )?;

    match command {
        Commands::Bar { action } => run_bar_login(action, &mut mgr)?,

        Commands::Status => {
            println!("{}", "=== MangoWM Mod ===".bold().cyan());
            let exists = |p: &Path| if p.exists() { "" } else { " (missing)" };
            println!(
                "Config:   {}{}",
                mgr.config_path.display(),
                exists(&mgr.config_path)
            );
            println!(
                "Managed:  {}{}",
                mgr.managed_path.display(),
                exists(&mgr.managed_path)
            );
            println!("Data dir: {}", mgr.data_dir.display());

            let changes = mgr.pending_changes();
            let items = mgr.pending_item_changes();
            if changes.is_empty() && items.is_empty() {
                println!("Staged:   {}", "none".green());
            } else {
                println!("Staged changes ({}):", changes.len() + items.len());
                for (k, v) in changes {
                    match v {
                        Some(val) => println!("  {} = {}", k.yellow(), val.green()),
                        None => println!("  {} (reset)", k.red()),
                    }
                }
                for line in items {
                    println!("  {}", line);
                }
                staged_hint();
            }
            println!("Backups:  {}", mgr.backups.list().len());
        }

        Commands::Get { key } => match mgr.get(&key) {
            Some(val) => println!("{}", val),
            None => {
                eprintln!("{}: {} is not set", "error".red(), key);
                std::process::exit(1);
            }
        },

        Commands::Set { key, value } => {
            let written = mgr.set(&key, &value)?;
            println!("Staged: {}={}", key.yellow(), written.green());
            staged_hint();
        }

        Commands::Reset { key } => {
            if mgr.reset(&key)? {
                println!("Staged reset for {}", key.yellow());
                staged_hint();
            } else {
                println!("{} is not managed by mangowm-mod; nothing to reset", key);
            }
        }

        Commands::Discard => {
            mgr.discard()?;
            println!("Staged changes discarded.");
        }

        Commands::Apply => {
            let res = mgr.apply()?;
            if !print_apply(&res) {
                std::process::exit(1);
            }
        }

        Commands::Schema { page, search } => {
            if let Some(q) = search {
                let found = search_options(&q);
                if found.is_empty() {
                    bail!("no MangoWM option matches {:?}", q);
                }
                for opt in &found {
                    print_option(opt);
                }
                return Ok(());
            }
            let pages = get_schema_pages();
            if let Some(id) = &page {
                if !pages.iter().any(|p| &p.id == id) {
                    let ids: Vec<&str> = pages.iter().map(|p| p.id.as_str()).collect();
                    bail!("unknown page {:?}: use one of {}", id, ids.join(", "));
                }
            }
            for p in pages
                .iter()
                .filter(|p| page.as_ref().map_or(true, |id| &p.id == id))
            {
                println!("\n=== {} ({}) ===", p.title.bold(), p.id.yellow());
                println!("  {}", p.description.dimmed());
                for opt in &p.options {
                    print_option(opt);
                }
            }
        }

        Commands::Preset { action } => match action {
            PresetCommands::List => {
                println!("{}", "Available presets:".bold().cyan());
                let current = current_preset_id(&mgr.data_dir);
                for p in all_presets(&mgr.data_dir) {
                    let mut marks = String::new();
                    if p.user {
                        marks.push_str(" (yours)");
                    }
                    if current.as_deref() == Some(p.id.as_str()) {
                        marks.push_str(" (current)");
                    }
                    println!(
                        "  - {:<16} {}{}",
                        p.id.yellow(),
                        p.name.bold(),
                        marks.dimmed()
                    );
                    if !p.description.is_empty() {
                        println!("    {}", p.description.dimmed());
                    }
                }
            }

            PresetCommands::Apply { name } => {
                let preset = find_any_preset(&mgr.data_dir, &name)
                    .ok_or_else(|| anyhow!("preset {:?} not found", name))?;
                let res = apply_preset(&mut mgr, &preset)?;
                println!("Preset: {}", preset.name.green().bold());
                if !print_apply(&res) {
                    std::process::exit(1);
                }
            }

            PresetCommands::Save { name, wallpaper } => {
                let p = save_user_preset(&mgr, &name, wallpaper)?;
                println!("Saved preset {} ({})", p.name.green().bold(), p.id.yellow());
            }

            PresetCommands::Update { name } => {
                let p = overwrite_user_preset(&mgr, &name)?;
                println!("Updated preset {}", p.name.green().bold());
            }

            PresetCommands::Rename { name, new_name } => {
                let p = rename_user_preset(&mgr.data_dir, &name, &new_name)?;
                println!("Renamed to {}", p.name.green().bold());
            }

            PresetCommands::Delete { name } => {
                let p = delete_user_preset(&mgr.data_dir, &name)?;
                println!("Deleted preset {}", p.name.bold());
            }
        },

        Commands::Bind { action } => match action {
            BindCommands::Add {
                mods,
                key,
                action,
                args,
                flags,
                mode,
                replace,
            } => {
                let mut bind = Bind::new(&mods, &key, &action, &args.join(" "));
                bind.flags = flags;
                bind.mode = mode;
                let line = bind.to_config_line();
                mgr.add_bind(bind, replace)?;
                println!("Staged: {}", line.green());
                staged_hint();
            }

            BindCommands::List => {
                let binds = &mgr.staged().binds;
                if binds.is_empty() {
                    println!("No managed binds.");
                }
                for (i, b) in binds.iter().enumerate() {
                    let mode = if b.mode == "default" {
                        String::new()
                    } else {
                        format!("  [mode {}]", b.mode)
                    };
                    println!("  {:>3}. {}{}", i + 1, b.to_config_line(), mode.dimmed());
                }
                println!(
                    "{} binds in your own config ({})",
                    mgr.user_binds().len(),
                    mgr.config_path.display()
                );
            }

            BindCommands::Remove { number } => {
                let b = mgr.remove_bind(number)?;
                println!("Staged removal: {}", b.to_config_line().red());
                staged_hint();
            }

            BindCommands::Actions => {
                for a in get_actions() {
                    let arg = a.arg_placeholder.unwrap_or_default();
                    println!(
                        "  {:<34} {:<22} {}",
                        a.id.cyan(),
                        arg.dimmed(),
                        a.description
                    );
                }
            }
        },

        Commands::Rule { action } => match action {
            RuleCommands::Add { kind, fields } => {
                if kind == "monitor_rule" {
                    bail!("use `mangowm-mod monitor add` for monitor rules");
                }
                if !RULE_ADD_KINDS.contains(&kind.as_str()) {
                    bail!(
                        "unknown rule type {:?}: use one of {}",
                        kind,
                        RULE_ADD_KINDS.join(", ")
                    );
                }
                let rule = Rule::parse(&kind, &fields)?;
                let line = rule.to_config_line();
                mgr.add_rule(rule)?;
                println!("Staged: {}", line.green());
                staged_hint();
            }

            RuleCommands::List => {
                let rules: Vec<&Rule> = mgr
                    .staged()
                    .rules
                    .iter()
                    .filter(|r| RULE_ADD_KINDS.contains(&r.kind.as_str()))
                    .collect();
                if rules.is_empty() {
                    println!("No managed rules.");
                }
                for (i, r) in rules.iter().enumerate() {
                    println!("  {:>3}. {}", i + 1, r.to_config_line());
                }
            }

            RuleCommands::Remove { number } => {
                let r = mgr.remove_rule(RULE_ADD_KINDS, number)?;
                println!("Staged removal: {}", r.to_config_line().red());
                staged_hint();
            }
        },

        Commands::Monitor { action } => match action {
            MonitorCommands::Add {
                name,
                width,
                height,
                refresh,
                x,
                y,
                scale,
                transform,
                vrr,
                disable,
                primary,
            } => {
                let mon = Monitor {
                    name,
                    width,
                    height,
                    refresh,
                    x,
                    y,
                    scale,
                    transform,
                    vrr,
                    disable,
                    primary,
                };
                let up = mgr.upsert_monitor_rule(mon.to_rule()?, &[])?;
                let line = up.rule.to_config_line();
                match &up.replaced {
                    Some(old) if *old == up.rule => {
                        println!("Unchanged: {}", line.green())
                    }
                    Some(old) => println!(
                        "Staged: {} (replaces {})",
                        line.green(),
                        old.to_config_line().dimmed()
                    ),
                    None => println!("Staged: {}", line.green()),
                }
                if up.removed_duplicates > 0 {
                    println!(
                        "Removed {} older duplicate rule(s) for {}",
                        up.removed_duplicates,
                        mon.name.trim()
                    );
                }
                for own in mgr.user_monitor_rules_for(mon.name.trim()) {
                    println!(
                        "{}: your config already has `{}`. Mango uses the first matching monitor \
                         rule, so remove that line for this one to take effect",
                        "warning".yellow(),
                        own
                    );
                }
                staged_hint();
            }

            MonitorCommands::List => {
                let rules: Vec<&Rule> = mgr
                    .staged()
                    .rules
                    .iter()
                    .filter(|r| r.kind == "monitor_rule")
                    .collect();
                if rules.is_empty() {
                    println!("No managed monitor rules.");
                }
                let mut seen: Vec<&str> = Vec::new();
                let mut shadowed = 0;
                for (i, r) in rules.iter().enumerate() {
                    let name = r.get("name").unwrap_or_default();
                    if seen.contains(&name) {
                        shadowed += 1;
                        println!(
                            "  {:>3}. {}  {}",
                            i + 1,
                            r.to_config_line().dimmed(),
                            "(ignored: an earlier rule matches)".yellow()
                        );
                    } else {
                        seen.push(name);
                        println!("  {:>3}. {}", i + 1, r.to_config_line());
                    }
                }
                if shadowed > 0 {
                    println!("Run `mangowm-mod monitor dedupe` to remove the ignored duplicates.");
                }
            }

            MonitorCommands::Dedupe => {
                let n = mgr.dedupe_monitor_rules()?;
                if n == 0 {
                    println!("No duplicate monitor rules.");
                } else {
                    println!("Staged removal of {} duplicate monitor rule(s)", n);
                    staged_hint();
                }
            }

            MonitorCommands::Remove { number } => {
                let r = mgr.remove_rule(&["monitor_rule"], number)?;
                println!("Staged removal: {}", r.to_config_line().red());
                staged_hint();
            }
        },

        Commands::Wallpaper { action } => match action {
            WallpaperCommands::Set { path } => {
                let res = set_wallpaper(&mut mgr, &path)?;
                println!("Wallpaper: {}", path.display().to_string().green().bold());
                if !print_apply(&res) {
                    std::process::exit(1);
                }
            }
        },

        Commands::Colors { action } => match action {
            ColorsCommands::FromWallpaper {
                windows,
                bar,
                apply,
                preview,
                path,
            } => {
                let home = dirs::home_dir().unwrap_or_default();
                let path = match path.or_else(|| current_wallpaper(&mgr, &home)) {
                    Some(p) => p,
                    None => bail!("no wallpaper is set; give a picture file"),
                };
                let palette = palette_from_file(&path)?;
                println!("Palette of {}:", path.display().to_string().bold());
                for (name, c) in palette.roles() {
                    println!("  {:<10} {}", name, c.hex().truecolor(c.r, c.g, c.b));
                }
                if palette.neutral {
                    println!("  (no strong colour in the picture: neutral palette)");
                }
                if preview {
                    return Ok(());
                }
                let (windows, bar) = if windows || bar {
                    (windows, bar)
                } else {
                    (true, true)
                };
                if bar {
                    let cfg = apply_bar_colors(&palette, &mgr.data_dir, None)?;
                    let dir = bar_dir(cfg.backend);
                    println!("Bar colours saved; files written to {}", dir.display());
                    if let Some(pid) = restart_bar_if_running(&cfg, Some(&dir), &mgr.data_dir)? {
                        println!("Restarted {} (PID {})", cfg.backend.binary(), pid);
                    }
                }
                if windows {
                    stage_window_colors(&mut mgr, &palette)?;
                    for (key, value) in window_colors(&palette) {
                        let written = mgr.get(key).unwrap_or(value);
                        println!("Staged: {}={}", key.yellow(), written.green());
                    }
                    if apply {
                        if !print_apply(&mgr.apply()?) {
                            std::process::exit(1);
                        }
                    } else {
                        staged_hint();
                    }
                }
            }
        },

        Commands::Autostart { action } => match action {
            AutostartCommands::Add {
                every_reload,
                command,
            } => {
                let cmd = command.join(" ");
                mgr.add_autostart(&cmd, every_reload)?;
                let kw = if every_reload { "exec" } else { "exec_once" };
                println!("Staged: {}", format!("{}={}", kw, cmd.trim()).green());
                staged_hint();
            }

            AutostartCommands::List => {
                let st = mgr.staged();
                if st.exec_once.is_empty() && st.exec.is_empty() {
                    println!("No managed autostart commands.");
                }
                let all = st
                    .exec_once
                    .iter()
                    .map(|c| ("exec_once", c))
                    .chain(st.exec.iter().map(|c| ("exec", c)));
                for (i, (kw, c)) in all.enumerate() {
                    println!("  {:>3}. {}={}", i + 1, kw, c);
                }
            }

            AutostartCommands::Remove { number } => {
                let (cmd, every) = mgr.remove_autostart(number)?;
                let kw = if every { "exec" } else { "exec_once" };
                println!("Staged removal: {}", format!("{}={}", kw, cmd).red());
                staged_hint();
            }
        },

        Commands::Backup { action } => match action {
            BackupCommands::List => {
                let list = mgr.backups.list();
                if list.is_empty() {
                    println!("No backups found.");
                } else {
                    println!("{}", "Backups:".bold().cyan());
                    for b in list {
                        println!("  {}  [{}] ({})", b.id.yellow(), b.created, b.label);
                    }
                }
            }

            BackupCommands::Restore { id } => {
                let files = mgr.backups.restore(&id)?;
                println!("{}", format!("Restored backup {}:", id).green());
                for f in files {
                    println!("  - {}", f);
                }
                println!("Reload: {}", mgr.ipc.reload());
            }
        },
    }

    Ok(())
}
