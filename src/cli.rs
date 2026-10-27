//! MangoWM Mod CLI interface.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

use crate::bar::{start_mangobar, write_bar_files, Mangobar};
use crate::core::manager::ConfigManager;
use crate::core::schema::get_schema_pages;
use crate::presets::{find_preset, list_presets};

#[derive(Parser, Debug)]
#[command(
    name = "mangowm-mod",
    about = "MangoWM and Mangobar settings and ricing manager",
    version
)]
pub struct Cli {
    #[arg(long, global = true, help = "Path to mango config.conf")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, help = "Path to mango-mod.conf")]
    pub managed: Option<PathBuf>,

    #[arg(long, global = true, help = "Data directory for backups and presets")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, global = true, help = "Run without applying live changes")]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Show current status and pending changes")]
    Status,

    #[command(about = "Get a configuration value")]
    Get { key: String },

    #[command(about = "Stage a configuration setting")]
    Set { key: String, value: String },

    #[command(about = "Reset a configuration setting to default")]
    Reset { key: String },

    #[command(about = "Apply staged changes to disk and reload MangoWM")]
    Apply,

    #[command(about = "Manage and apply presets for MangoWM & Mangobar")]
    Preset {
        #[command(subcommand)]
        action: PresetCommands,
    },

    #[command(about = "Mangobar bar builder and controls")]
    Bar {
        #[command(subcommand)]
        action: BarCommands,
    },

    #[command(about = "Manage automatic backups")]
    Backup {
        #[command(subcommand)]
        action: BackupCommands,
    },

    #[command(about = "List settings schema pages and options")]
    Schema,
}

#[derive(Subcommand, Debug)]
pub enum PresetCommands {
    #[command(about = "List all available presets")]
    List,

    #[command(about = "Apply a preset by ID or name")]
    Apply { name: String },
}

#[derive(Subcommand, Debug)]
pub enum BarCommands {
    #[command(about = "Generate Mangobar config and launch")]
    Start,

    #[command(about = "Stop running Mangobar")]
    Stop,

    #[command(about = "Generate Mangobar config.jsonc and style.css files")]
    Generate {
        #[arg(short, long, help = "Output directory")]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum BackupCommands {
    #[command(about = "List backups")]
    List,

    #[command(about = "Restore a backup by ID")]
    Restore { id: String },
}

pub fn run_cli(cli: Cli) -> Result<()> {
    let mut mgr = ConfigManager::new(
        cli.config.as_deref(),
        cli.managed.as_deref(),
        cli.data_dir.as_deref(),
        cli.dry_run,
    )?;

    let command = match cli.command {
        Some(cmd) => cmd,
        None => Commands::Status,
    };

    match command {
        Commands::Status => {
            println!("{}", "=== MangoWM Mod ===".bold().cyan());
            println!("Config Path:  {}", mgr.config_path.display());
            println!("Managed Path: {}", mgr.managed_path.display());
            println!("Data Dir:     {}", mgr.data_dir.display());

            let changes = mgr.pending_changes();
            if changes.is_empty() {
                println!("Staged:       {}", "none".green());
            } else {
                println!("Staged Changes ({}):", changes.len());
                for (k, v) in changes {
                    if let Some(val) = v {
                        println!("  {} = {}", k.yellow(), val.green());
                    } else {
                        println!("  {} (reset)", k.red());
                    }
                }
            }

            let backups = mgr.backups.list();
            println!("Backups:      {}", backups.len());
        }

        Commands::Get { key } => {
            if let Some(val) = mgr.get(&key) {
                println!("{}", val);
            } else {
                eprintln!("{}: key not found", "error".red());
                std::process::exit(1);
            }
        }

        Commands::Set { key, value } => {
            mgr.set(&key, &value)?;
            println!("Staged: {} = {}", key.yellow(), value.green());
            println!("Run {} to write changes and reload compositor.", "mangowm-mod apply".bold());
        }

        Commands::Reset { key } => {
            mgr.reset(&key);
            println!("Staged reset for {}", key.yellow());
        }

        Commands::Apply => {
            let res = mgr.apply()?;
            if res.ok {
                println!("{}", "Successfully applied changes to MangoWM!".green().bold());
                if let Some(bid) = res.backup_id {
                    println!("Backup created: {}", bid);
                }
            } else {
                eprintln!("{}", "Apply completed with errors:".red().bold());
                for err in res.errors {
                    eprintln!("  - {}", err);
                }
            }
        }

        Commands::Preset { action } => match action {
            PresetCommands::List => {
                println!("{}", "Available Presets:".bold().cyan());
                for p in list_presets() {
                    println!("  - {:<16} {}", p.id.yellow(), p.name.bold());
                    if !p.description.is_empty() {
                        println!("    {}", p.description.dimmed());
                    }
                }
            }

            PresetCommands::Apply { name } => {
                let preset = find_preset(&name)
                    .ok_or_else(|| anyhow!("Preset {:?} not found", name))?;

                for (k, v) in &preset.settings {
                    mgr.set(k, v)?;
                }

                let res = mgr.apply()?;
                if res.ok {
                    println!("Applied preset: {}", preset.name.green().bold());
                } else {
                    eprintln!("Failed to apply preset: {:?}", res.errors);
                }
            }
        },

        Commands::Bar { action } => match action {
            BarCommands::Start => {
                let bar = Mangobar::default();
                write_bar_files(&bar, None::<&str>)?;
                start_mangobar(None::<&str>)?;
                println!("{}", "Mangobar started with MangoWM Mod config!".green());
            }

            BarCommands::Stop => {
                crate::bar::stop_mangobar();
                println!("Stopped Mangobar.");
            }

            BarCommands::Generate { out } => {
                let bar = Mangobar::default();
                let dir = write_bar_files(&bar, out.as_deref())?;
                println!("Mangobar files written to {}", dir.display().to_string().green());
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
                println!("{}", format!("Restored backup {} successfully:", id).green());
                for f in files {
                    println!("  - {}", f);
                }
                let _ = mgr.ipc.reload();
            }
        },

        Commands::Schema => {
            let pages = get_schema_pages();
            println!("{}", "MangoWM Settings Schema:".bold().cyan());
            for p in pages {
                println!("\n=== {} ({}) ===", p.title.bold(), p.id.yellow());
                println!("  {}", p.description.dimmed());
                for opt in p.options {
                    println!("    {:<26} {:<10} {}", opt.key.cyan(), opt.r#type.dimmed(), opt.label);
                }
            }
        }
    }

    Ok(())
}
