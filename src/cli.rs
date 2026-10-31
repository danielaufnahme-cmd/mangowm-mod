//! MangoWM Mod CLI interface.

use anyhow::{anyhow, bail, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::bar::generate::{bar_dir, warnings, waybar_module_name, write_bar_files};
use crate::bar::model::{Backend, BarConfig, KNOWN_MODULES};
use crate::bar::process::{log_file, start_bar, stop_bar};
use crate::core::actions::get_actions;
use crate::core::items::{Bind, Monitor, Rule};
use crate::core::manager::{detect_data_dir, ApplyResult, ConfigManager};
use crate::core::schema::{get_schema_pages, search_options, OptionItem};
use crate::presets::{find_preset, list_presets};

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

    #[command(about = "List and apply presets")]
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
    #[command(about = "Add a monitor rule for an output, e.g. eDP-1")]
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
    #[arg(long)]
    pub radius: Option<u32>,
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
        #[command(flatten)]
        flags: BarFlags,
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

        BarCommands::Start { flags } => {
            let bar = bar_with_flags(data_dir, &flags)?;
            let dir = write_bar_files(&bar, None::<&Path>)?;
            print_bar_warnings(&bar);
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

        BarCommands::Modules => {
            println!("{}", "Bar modules (Waybar name in brackets):".bold().cyan());
            for (name, desc) in KNOWN_MODULES {
                let wb = waybar_module_name(name, true).unwrap_or_else(|| "-".into());
                println!("  {:<16} {:<44} [{}]", name.cyan(), desc, wb);
            }
            println!("  Also: clock#date, clock#time, battery#<device>, custom/<name>");
            println!("  Waybar without mango/* modules: workspaces -> ext/workspaces; layout, window, keymode and keyboardlayout are left out");
        }
    }
    Ok(())
}

pub fn run_cli(cli: Cli) -> Result<()> {
    let command = cli.command.unwrap_or(Commands::Status);

    // Bar commands work without any Mango config.
    if let Commands::Bar { action } = command {
        let data_dir = detect_data_dir(cli.data_dir.as_deref());
        return run_bar(action, &data_dir);
    }

    let mut mgr = ConfigManager::new(
        cli.config.as_deref(),
        cli.managed.as_deref(),
        cli.data_dir.as_deref(),
        cli.dry_run,
    )?;

    match command {
        Commands::Bar { .. } => unreachable!("handled above"),

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
                for p in list_presets() {
                    println!("  - {:<16} {}", p.id.yellow(), p.name.bold());
                    if !p.description.is_empty() {
                        println!("    {}", p.description.dimmed());
                    }
                }
            }

            PresetCommands::Apply { name } => {
                let preset =
                    find_preset(&name).ok_or_else(|| anyhow!("preset {:?} not found", name))?;
                for (k, v) in &preset.settings {
                    mgr.set(k, v)?;
                }
                let res = mgr.apply()?;
                println!("Preset: {}", preset.name.green().bold());
                if !print_apply(&res) {
                    std::process::exit(1);
                }
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
                let rule = mon.to_rule()?;
                let line = rule.to_config_line();
                let earlier =
                    mgr.staged().rules.iter().position(|r| {
                        r.kind == "monitor_rule" && r.get("name") == rule.get("name")
                    });
                mgr.add_rule(rule)?;
                println!("Staged: {}", line.green());
                if earlier.is_some() {
                    println!(
                        "{}: a managed monitor rule for {} already exists and Mango uses the \
                         first match; remove the old one with `mangowm-mod monitor remove`",
                        "warning".yellow(),
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
                for (i, r) in rules.iter().enumerate() {
                    println!("  {:>3}. {}", i + 1, r.to_config_line());
                }
            }

            MonitorCommands::Remove { number } => {
                let r = mgr.remove_rule(&["monitor_rule"], number)?;
                println!("Staged removal: {}", r.to_config_line().red());
                staged_hint();
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
