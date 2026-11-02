//! Wallpaper page: pick Mango's wallpaper from a thumbnail grid. Like a
//! preset, a pick is applied at once (the single managed swaybg line).
//! Above the grid, "Match colours" colours the window borders and the bar
//! after the wallpaper (src/palette.rs).

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::{gio, glib};

use mangowm_mod::bar::process::LiveUpdate;
use mangowm_mod::palette::{
    apply_bar_colors_live, palette_from_file, stage_window_colors,
    MatchSettings, Palette,
};
use mangowm_mod::wallpaper::{
    bundled_dir, collect_wallpapers, current_wallpaper, mango_wallpaper, plasma_wallpaper_in,
    set_wallpaper, WallpaperEntry,
};

use crate::ctx::Ctx;
use crate::presets::{rounded_rect, thumb, Pixels};
use crate::window::report_apply;

/// Thumbnails are scaled to this width (enough for a HiDPI card).
const THUMB_WIDTH: i32 = 480;

/// Cached thumbnail of `path`: `~/.cache/mangowm-mod/thumbs/<hash>.png`, keyed
/// by path, size and modification time so a changed picture gets a new one.
fn cache_file(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok()?.hash(&mut h);
    Some(
        dirs::cache_dir()?
            .join("mangowm-mod")
            .join("thumbs")
            .join(format!("{:016x}.png", h.finish())),
    )
}

/// Make the thumbnail if it is not cached yet (runs on a worker thread).
fn make_thumb(path: &Path) -> Option<PathBuf> {
    let out = cache_file(path)?;
    if out.is_file() {
        return Some(out);
    }
    let pb = Pixbuf::from_file_at_scale(path, THUMB_WIDTH, -1, true).ok()?;
    std::fs::create_dir_all(out.parent()?).ok()?;
    let tmp = out.with_extension("png.tmp");
    pb.savev(&tmp, "png", &[]).ok()?;
    std::fs::rename(&tmp, &out).ok()?;
    Some(out)
}

/// Thumbnail area: a soft placeholder until the picture is loaded, then the
/// picture (cropped to fill) fading in.
fn preview(e: &WallpaperEntry) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_height(130)
        .content_width(200)
        .css_classes(["preset-preview"])
        .build();
    let pixbuf: Rc<RefCell<Option<Pixbuf>>> = Rc::default();
    let pb = pixbuf.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
        cr.clip();
        let Some(pb) = &*pb.borrow() else {
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.12);
            let _ = cr.paint();
            return;
        };
        let (pw, ph) = (pb.width() as f64, pb.height() as f64);
        let k = (w / pw).max(h / ph);
        cr.translate((w - pw * k) / 2.0, (h - ph * k) / 2.0);
        cr.scale(k, k);
        cr.set_source_pixbuf(pb, 0.0, 0.0);
        let _ = cr.paint();
    });

    let show = {
        let area = area.clone();
        move |loaded: Option<Pixbuf>| {
            let Some(loaded) = loaded else { return };
            *pixbuf.borrow_mut() = Some(loaded);
            let target = adw::CallbackAnimationTarget::new({
                let area = area.clone();
                move |v| area.set_opacity(v)
            });
            adw::TimedAnimation::new(&area, 0.3, 1.0, 150, target).play();
            area.queue_draw();
        }
    };
    let embedded = e.bundled.then(|| thumb(&e.name)).flatten();
    let path = e.path.clone();
    // decode off the main thread: big pictures take long, even the cached ones add up
    glib::spawn_future_local(async move {
        let pixels = gio::spawn_blocking(move || {
            let pb = match embedded {
                Some(bytes) => {
                    let stream =
                        gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
                    Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()
                }
                None => make_thumb(&path).and_then(|c| Pixbuf::from_file(c).ok()),
            };
            pb.map(|pb| Pixels::of(&pb))
        })
        .await
        .ok()
        .flatten();
        show(pixels.map(Pixels::pixbuf));
    });
    area
}

fn chip(text: &str, class: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(["preset-chip", class])
        .build()
}

struct Card {
    widget: gtk::Widget,
    path: PathBuf,
    mango: gtk::Label,
}

fn apply(ctx: &Ctx, parent: &gtk::Widget, path: &Path) {
    let auto = MatchSettings::load(&ctx.data_dir);
    if !auto.auto || !(auto.windows || auto.bar) {
        set_now(ctx, parent, path, None);
        return;
    }
    // match the colours too: read the picture off the main thread first
    let (ctx, parent, path) = (ctx.clone(), parent.clone(), path.to_path_buf());
    glib::spawn_future_local(async move {
        let p = path.clone();
        let palette = match gio::spawn_blocking(move || palette_from_file(&p)).await {
            Ok(Ok(palette)) => Some(palette),
            Ok(Err(e)) => {
                ctx.toast(&format!("Colours: {:#}", e));
                None
            }
            Err(_) => None,
        };
        let colors = palette.map(|p| (p, auto));
        set_now(&ctx, &parent, &path, colors.as_ref());
    });
}

/// Set the wallpaper; with `colors`, also its matched window colours (staged
/// first, so the same apply writes them) and bar colours.
fn set_now(ctx: &Ctx, parent: &gtk::Widget, path: &Path, colors: Option<&(Palette, MatchSettings)>) {
    let mut what = "Wallpaper set";
    if let Some((palette, _)) = colors.filter(|(_, c)| c.windows) {
        let staged = stage_window_colors(&mut ctx.mgr.borrow_mut(), palette);
        match staged {
            Ok(()) => what = "Wallpaper and window colours set",
            Err(e) => ctx.toast(&format!("Window colours: {:#}", e)),
        }
    }
    let res = set_wallpaper(&mut ctx.mgr.borrow_mut(), path);
    match res {
        Ok(r) => report_apply(ctx, parent, &r, Some(what)),
        Err(e) => ctx.toast(&format!("Wallpaper: {:#}", e)),
    }
    if let Some((palette, _)) = colors.filter(|(_, c)| c.bar) {
        bar_colors(ctx, palette);
    }
    ctx.changed();
}

/// Save the bar colours and show them in the running bar (reload Waybar,
/// restart Mangobar). The restart waits for the new bar, so it runs off the
/// UI thread; the toast comes when it is done.
fn bar_colors(ctx: &Ctx, palette: &Palette) {
    let (ctx, palette) = (ctx.clone(), palette.clone());
    glib::spawn_future_local(async move {
        let data_dir = ctx.data_dir.clone();
        // the offscreen screenshot mode never touches the real bar
        let live = std::env::var("GDK_BACKEND").as_deref() != Ok("broadway");
        let res = gio::spawn_blocking(move || {
            if live {
                apply_bar_colors_live(&palette, &data_dir)
            } else {
                mangowm_mod::palette::save_bar_colors(&palette, &data_dir)
                    .map(|_| LiveUpdate::Written)
            }
        })
        .await;
        match res {
            Ok(Ok(LiveUpdate::Restarted(_))) => ctx.toast("Bar colours set · bar restarted"),
            Ok(Ok(LiveUpdate::Reloaded(_))) => ctx.toast("Bar colours set · bar reloaded"),
            Ok(Ok(_)) => ctx.toast("Bar colours set"),
            Ok(Err(e)) => ctx.toast(&format!("Bar colours: {:#}", e)),
            Err(_) => ctx.toast("Bar colours: the update failed"),
        }
    });
}

/// Apply at once; ask first only when other staged changes would go with it.
fn pick(ctx: &Ctx, parent: &gtk::Widget, path: &Path) {
    if !ctx.mgr.borrow().is_dirty() {
        apply(ctx, parent, path);
        return;
    }
    let dialog = adw::AlertDialog::builder()
        .heading("Set Wallpaper?")
        .body("Your other staged changes are applied too, and MangoWM is reloaded.")
        .default_response("apply")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("apply", "Apply");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    let (ctx, par, path) = (ctx.clone(), parent.clone(), path.to_path_buf());
    dialog.connect_response(Some("apply"), move |_, _| apply(&ctx, &par, &path));
    dialog.present(Some(parent));
}

fn card(ctx: &Ctx, e: &WallpaperEntry) -> Card {
    let bx = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    bx.append(&preview(e));
    let row = gtk::Box::builder().spacing(6).build();
    row.append(
        &gtk::Label::builder()
            .label(&e.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );
    if e.in_plasma {
        row.append(&chip("Current in Plasma", "wallpaper-plasma"));
    }
    let mango = chip("In Mango", "wallpaper-mango");
    mango.set_visible(e.in_mango);
    row.append(&mango);
    bx.append(&row);

    let btn = gtk::Button::builder()
        .child(&bx)
        .tooltip_text(e.path.display().to_string())
        .css_classes(["card", "wallpaper-card"])
        .build();
    let (c, path) = (ctx.clone(), e.path.clone());
    btn.connect_clicked(move |b| pick(&c, b.upcast_ref(), &path));
    Card {
        widget: btn.upcast(),
        path: std::fs::canonicalize(&e.path).unwrap_or_else(|_| e.path.clone()),
        mango,
    }
}

/// One colour of the palette strip: a rounded swatch over its role name.
fn swatch(name: &str, color: Option<mangowm_mod::palette::Rgb>) -> gtk::Box {
    let area = gtk::DrawingArea::builder()
        .content_height(40)
        .hexpand(true)
        .build();
    area.set_draw_func(move |_, cr, w, h| {
        rounded_rect(cr, 0.0, 0.0, w as f64, h as f64, 8.0);
        match color {
            Some(c) => cr.set_source_rgb(c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0),
            None => cr.set_source_rgba(0.5, 0.5, 0.5, 0.12),
        }
        let _ = cr.fill_preserve();
        // a faint edge, so a swatch close to the window colour still shows
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.25);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });
    let bx = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .css_classes(["palette-swatch"])
        .build();
    if let Some(c) = color {
        bx.set_tooltip_text(Some(&format!("{} {}", name, c.hex())));
    }
    bx.append(&area);
    bx.append(
        &gtk::Label::builder()
            .label(name)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["caption", "dim-label"])
            .build(),
    );
    bx
}

/// "Match colours to wallpaper": the palette of Mango's wallpaper (else
/// Plasma's), what to colour, and the button that does it.
struct Matcher {
    ctx: Ctx,
    group: adw::PreferencesGroup,
    strip: gtk::Box,
    windows: adw::SwitchRow,
    bar: adw::SwitchRow,
    auto: adw::SwitchRow,
    button: gtk::Button,
    /// Picture the strip shows and its palette (None while reading or on error).
    shown: RefCell<Option<PathBuf>>,
    palette: RefCell<Option<Palette>>,
}

impl Matcher {
    fn new(ctx: &Ctx) -> Rc<Self> {
        let saved = MatchSettings::load(&ctx.data_dir);
        let button = gtk::Button::builder()
            .label("Apply Colours")
            .valign(gtk::Align::Center)
            .sensitive(false)
            .css_classes(["suggested-action"])
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Match Colours to Wallpaper")
            .description("No wallpaper found")
            .header_suffix(&button)
            .build();
        let strip = gtk::Box::builder()
            .spacing(8)
            .homogeneous(true)
            .margin_top(12)
            .margin_bottom(10)
            .margin_start(12)
            .margin_end(12)
            .build();
        let strip_row = adw::PreferencesRow::builder()
            .activatable(false)
            .focusable(false)
            .child(&strip)
            .build();
        group.add(&strip_row);
        let windows = adw::SwitchRow::builder()
            .title("Window borders")
            .subtitle("Focused border in the accent, the others from the palette (staged, then applied)")
            .active(saved.windows)
            .build();
        let bar = adw::SwitchRow::builder()
            .title("Bar")
            .subtitle("Bar colours; the bar is restarted if it is running")
            .active(saved.bar)
            .build();
        let auto = adw::SwitchRow::builder()
            .title("Update automatically when the wallpaper changes")
            .subtitle("Picking a wallpaper here also applies its colours")
            .active(saved.auto)
            .build();
        group.add(&windows);
        group.add(&bar);
        group.add(&auto);

        let m = Rc::new(Self {
            ctx: ctx.clone(),
            group,
            strip,
            windows,
            bar,
            auto,
            button,
            shown: RefCell::default(),
            palette: RefCell::default(),
        });
        for row in [&m.windows, &m.bar, &m.auto] {
            let w = Rc::downgrade(&m);
            row.connect_active_notify(move |_| {
                if let Some(m) = w.upgrade() {
                    m.save();
                }
            });
        }
        let w = Rc::downgrade(&m);
        m.button.connect_clicked(move |b| {
            if let Some(m) = w.upgrade() {
                m.apply(b.upcast_ref());
            }
        });
        m.fill(None);
        m.refresh();
        m
    }

    fn settings(&self) -> MatchSettings {
        MatchSettings {
            windows: self.windows.is_active(),
            bar: self.bar.is_active(),
            auto: self.auto.is_active(),
        }
    }

    fn save(&self) {
        let s = self.settings();
        if let Err(e) = s.save(&self.ctx.data_dir) {
            self.ctx.toast(&format!("{:#}", e));
        }
        self.update_button();
    }

    /// The button needs a palette and something to colour.
    fn update_button(&self) {
        let s = self.settings();
        self.button
            .set_sensitive(self.palette.borrow().is_some() && (s.windows || s.bar));
    }

    fn fill(&self, palette: Option<&Palette>) {
        while let Some(c) = self.strip.first_child() {
            self.strip.remove(&c);
        }
        let roles = palette.map(|p| p.roles());
        for (i, name) in ["Background", "Surface", "Text", "Accent", "Secondary", "Urgent", "Muted"]
            .iter()
            .enumerate()
        {
            self.strip
                .append(&swatch(name, roles.as_ref().map(|r| r[i].1)));
        }
    }

    /// Read the palette of the current wallpaper if it changed.
    fn refresh(self: &Rc<Self>) {
        let home = dirs::home_dir().unwrap_or_default();
        let now = current_wallpaper(&self.ctx.mgr.borrow(), &home);
        if *self.shown.borrow() == now {
            return;
        }
        self.shown.replace(now.clone());
        self.palette.replace(None);
        self.button.set_sensitive(false);
        let Some(path) = now else {
            self.fill(None);
            self.group.set_description(Some("No wallpaper found"));
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.group.set_description(Some(&format!("Reading {}…", name)));
        let w = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let p = path.clone();
            let res = gio::spawn_blocking(move || palette_from_file(&p)).await;
            let Some(m) = w.upgrade() else { return };
            if m.shown.borrow().as_ref() != Some(&path) {
                return; // the wallpaper changed again meanwhile
            }
            match res {
                Ok(Ok(palette)) => {
                    m.fill(Some(&palette));
                    let note = if palette.neutral {
                        " (no strong colour in it: a neutral palette)"
                    } else {
                        ""
                    };
                    m.group
                        .set_description(Some(&format!("Colours from {}{}", name, note)));
                    m.palette.replace(Some(palette));
                    m.update_button();
                }
                Ok(Err(e)) => {
                    m.fill(None);
                    m.group.set_description(Some(&format!("{:#}", e)));
                }
                Err(_) => m.fill(None),
            }
        });
    }

    fn apply(self: &Rc<Self>, parent: &gtk::Widget) {
        let Some(palette) = self.palette.borrow().clone() else {
            return;
        };
        let s = self.settings();
        if s.bar {
            bar_colors(&self.ctx, &palette);
        }
        if !s.windows {
            return;
        }
        let (ctx, par) = (self.ctx.clone(), parent.clone());
        let go = move || {
            let res = stage_window_colors(&mut ctx.mgr.borrow_mut(), &palette)
                .and_then(|_| ctx.mgr.borrow_mut().apply());
            match res {
                Ok(r) => report_apply(&ctx, &par, &r, Some("Window colours applied")),
                Err(e) => ctx.toast(&format!("Window colours: {:#}", e)),
            }
            ctx.changed();
        };
        if !self.ctx.mgr.borrow().is_dirty() {
            go();
            return;
        }
        let dialog = adw::AlertDialog::builder()
            .heading("Apply Window Colours?")
            .body("Your other staged changes are applied too, and MangoWM is reloaded.")
            .default_response("apply")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("apply", "Apply");
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
        dialog.connect_response(Some("apply"), move |_, _| go());
        dialog.present(Some(parent));
    }
}

pub fn page(ctx: &Ctx) -> gtk::Widget {
    let home = dirs::home_dir().unwrap_or_default();
    let entries = {
        let mgr = ctx.mgr.borrow();
        collect_wallpapers(
            &bundled_dir(&mgr.data_dir),
            &home,
            plasma_wallpaper_in(&home).as_deref(),
            mango_wallpaper(&mgr).as_deref(),
        )
    };
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(3)
        .column_spacing(12)
        .row_spacing(12)
        .valign(gtk::Align::Start)
        .build();
    let cards: Vec<Card> = entries.iter().map(|e| card(ctx, e)).collect();
    for c in &cards {
        flow.append(&c.widget);
    }

    let matcher = Matcher::new(ctx);

    // keep the "In Mango" mark and the palette right after a preset or a pick
    let c = ctx.clone();
    let m = matcher.clone();
    ctx.on_change(move || {
        m.refresh();
        let now = mango_wallpaper(&c.mgr.borrow())
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
        for card in &cards {
            card.mango.set_visible(now.as_ref() == Some(&card.path));
        }
    });

    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    col.append(
        &gtk::Label::builder()
            .label(format!(
                "Click a picture to make it Mango's wallpaper; it is set right away. \
                 Shows the bundled wallpapers and ~/{}.",
                mangowm_mod::wallpaper::USER_DIRS.join(" and ~/")
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    col.prepend(&matcher.group);
    col.append(&flow);
    let clamp = adw::Clamp::builder()
        .maximum_size(980)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .child(&col)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build()
        .upcast()
}

