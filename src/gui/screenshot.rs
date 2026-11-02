//! Hidden developer mode: render the window offscreen to a PNG.
//!
//! Only runs with GDK_BACKEND=broadway (never on the real desktop), e.g.
//! `gtk4-broadwayd :5 & GDK_BACKEND=broadway BROADWAY_DISPLAY=:5
//! mangowm-mod-gui --dry-run --config ... --screenshot out.png --page binds`.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use clap::Args;
use gtk::{gdk, glib, graphene, gsk};

use crate::ctx::timed;
use crate::window::MainWindow;

#[derive(Args, Debug, Default)]
pub struct DevArgs {
    #[arg(long, hide = true)]
    pub screenshot: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub page: Option<String>,
    #[arg(long, hide = true)]
    pub search: Option<String>,
    /// Change a setting through its row widget, as a click would.
    #[arg(long = "set", hide = true, value_name = "KEY=VALUE")]
    pub sets: Vec<String>,
    /// Press the Apply bar's Apply button after the --set changes.
    #[arg(long, hide = true)]
    pub apply: bool,
    #[arg(long, hide = true)]
    pub dark: bool,
    #[arg(long, hide = true, value_name = "WxH")]
    pub size: Option<String>,
    /// Scroll the open page down by this many pixels.
    #[arg(long, hide = true)]
    pub scroll: Option<f64>,
    /// Open the expander rows with this title.
    #[arg(long, hide = true)]
    pub expand: Vec<String>,
}

/// Every descendant of `w` (depth first).
fn descendants(w: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    let mut child = w.first_child();
    while let Some(c) = child {
        out.push(c.clone());
        descendants(&c, out);
        child = c.next_sibling();
    }
}

pub fn prepare_style(dark: bool) {
    // broadway without a browser never ticks the frame clock: no transitions
    if let Some(s) = gtk::Settings::default() {
        s.set_gtk_enable_animations(false);
    }
    adw::StyleManager::default().set_color_scheme(if dark {
        adw::ColorScheme::ForceDark
    } else {
        adw::ColorScheme::ForceLight
    });
}

pub fn run(app: &adw::Application, win: MainWindow, dev: &DevArgs) {
    if let Some((w, h)) = dev.size.as_deref().and_then(|s| s.split_once('x')) {
        if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
            win.window.set_default_size(w, h);
        }
    }
    let app = app.clone();
    let win = Rc::new(win);
    let out = dev.screenshot.clone().unwrap_or_default();
    let (page, search, sets, apply) = (
        dev.page.clone(),
        dev.search.clone(),
        dev.sets.clone(),
        dev.apply,
    );
    let (scroll, expand) = (dev.scroll, dev.expand.clone());
    glib::timeout_add_local_once(Duration::from_millis(800), move || {
        if let Some(st) = &win.state {
            if let Some(p) = &page {
                st.open_page(p);
            }
            for s in &sets {
                let (k, v) = s.split_once('=').unwrap_or((s, ""));
                match st.row(k) {
                    Some(r) => timed(&format!("change {}={}", k, v), || r.simulate(v)),
                    None => eprintln!("screenshot: no row for {}", k),
                }
            }
            if apply {
                st.apply_bar.apply.emit_clicked();
            }
            if let Some(q) = &search {
                st.search.set_text(q);
                st.on_search(q);
            }
        }
        let mut all = Vec::new();
        if let Some(content) = win.window.content() {
            descendants(&content, &mut all);
        }
        for w in &all {
            if let Some(e) = w.downcast_ref::<adw::ExpanderRow>() {
                if e.is_mapped() && expand.iter().any(|t| *t == e.title()) {
                    e.set_expanded(true);
                }
            }
        }
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            if let Some(px) = scroll {
                // the visible page's scroller: the tallest mapped one
                if let Some(sw) = all
                    .iter()
                    .filter_map(|w| w.downcast_ref::<gtk::ScrolledWindow>())
                    .filter(|sw| sw.is_mapped())
                    .max_by_key(|sw| sw.vadjustment().upper() as i64)
                {
                    sw.vadjustment().set_value(px);
                }
            }
            match capture(&win.window, &out) {
                Ok(p) => eprintln!("screenshot: {}", p.display()),
                Err(e) => eprintln!("screenshot failed: {}", e),
            }
            app.quit();
        });
    });
}

fn capture(win: &adw::ApplicationWindow, out: &std::path::Path) -> Result<PathBuf, String> {
    let target = win.content().ok_or("no content")?;
    let (w, h) = (target.width(), target.height());
    if !win.is_mapped() || w <= 0 {
        return Err("window not mapped".into());
    }
    // layout only runs on frame-clock ticks, which broadway rarely sends
    target.allocate(w, h, -1, None);
    let parent = target.parent().ok_or("no parent")?;
    let inner = gtk::Snapshot::new();
    parent.snapshot_child(&target, &inner);
    let content = inner.to_node().ok_or("nothing rendered")?;
    let snap = gtk::Snapshot::new();
    // the window background lives on the toplevel, which is not snapshotted
    let bg = if adw::StyleManager::default().is_dark() {
        "#222226"
    } else {
        "#fafafb"
    };
    snap.append_color(
        &gdk::RGBA::parse(bg).map_err(|e| e.to_string())?,
        &graphene::Rect::new(0.0, 0.0, w as f32, h as f32),
    );
    snap.append_node(&content);
    let node = snap.to_node().ok_or("empty snapshot")?;
    let display = gdk::Display::default().ok_or("no display")?;
    let renderer = gsk::CairoRenderer::new();
    renderer
        .realize_for_display(&display)
        .map_err(|e| e.to_string())?;
    let tex = renderer.render_texture(
        &node,
        Some(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32)),
    );
    renderer.unrealize();
    let out = std::path::absolute(out).map_err(|e| e.to_string())?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    tex.save_to_png(&out).map_err(|e| e.to_string())?;
    Ok(out)
}
