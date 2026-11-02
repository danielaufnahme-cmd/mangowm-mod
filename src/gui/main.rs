//! MangoWM Mod settings window (GTK4 / libadwaita).
//!
//! Same engine and the same staging model as the CLI: edits are staged in
//! `<data_dir>/staged.json`, the Apply bar writes mango-mod.conf and reloads.

mod apply_bar;
mod backups;
mod bar;
mod ctx;
mod items;
mod logic;
mod presets;
mod rows;
mod screenshot;
mod wallpaper;
mod window;

use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use clap::Parser;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.danielaufnahme_cmd.MangowmMod";
const STYLE: &str = include_str!("style.css");

/// Path options match the CLI's global options.
#[derive(Parser, Debug, Default)]
#[command(
    name = "mangowm-mod-gui",
    about = "MangoWM settings window (MangoWM Mod)",
    version
)]
pub struct GuiArgs {
    #[arg(long, help = "Path to mango config.conf")]
    pub config: Option<PathBuf>,

    #[arg(long, help = "Path to mango-mod.conf")]
    pub managed: Option<PathBuf>,

    #[arg(
        long,
        help = "Data directory for backups, staged changes and bar settings"
    )]
    pub data_dir: Option<PathBuf>,

    #[arg(long, help = "Write files but do not ask MangoWM to reload")]
    pub dry_run: bool,

    #[command(flatten)]
    pub dev: screenshot::DevArgs,
}

fn main() -> glib::ExitCode {
    let started = std::time::Instant::now();
    let args = GuiArgs::parse();
    if args.dev.screenshot.is_some() && std::env::var("GDK_BACKEND").as_deref() != Ok("broadway") {
        eprintln!("--screenshot only runs offscreen: set GDK_BACKEND=broadway");
        return glib::ExitCode::from(2);
    }

    let mut flags = gio::ApplicationFlags::empty();
    if args.dev.screenshot.is_some() {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    let args = Rc::new(args);

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| app.quit()
    ));
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);

    let a = args.clone();
    app.connect_startup(move |_| {
        if a.dev.screenshot.is_some() {
            screenshot::prepare_style(a.dev.dark);
        }
        let provider = gtk::CssProvider::new();
        provider.load_from_string(STYLE);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    app.connect_activate(move |app| {
        if let Some(win) = app.active_window() {
            win.present();
            return;
        }
        let win = window::MainWindow::new(app, &args);
        // the handlers only hold weak refs to the state: keep it as long as the window lives
        if let Some(state) = win.state.clone() {
            win.window.connect_destroy(move |_| {
                let _ = &state;
            });
        }
        win.window.present();
        if ctx::profiling() {
            ctx::since(started, "start -> window presented");
            glib::idle_add_local_once(move || ctx::since(started, "start -> main loop idle"));
        }
        if args.dev.screenshot.is_some() {
            screenshot::run(app, win, &args.dev);
        }
    });

    // GTK must not see our options; clap has handled them.
    let argv0: Vec<String> = std::env::args().take(1).collect();
    app.run_with_args(&argv0)
}
