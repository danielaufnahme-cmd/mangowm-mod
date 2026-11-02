//! Wallpaper page: pick Mango's wallpaper from a thumbnail grid. Like a
//! preset, a pick is applied at once (the single managed swaybg line).

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::{gio, glib};

use mangowm_mod::wallpaper::{
    bundled_dir, collect_wallpapers, mango_wallpaper, plasma_wallpaper_in, set_wallpaper,
    WallpaperEntry,
};

use crate::ctx::Ctx;
use crate::presets::{rounded_rect, thumb};
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
    if let Some(bytes) = embedded {
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
        show(Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok());
    } else {
        let path = e.path.clone();
        glib::spawn_future_local(async move {
            let cached = gio::spawn_blocking(move || make_thumb(&path))
                .await
                .ok()
                .flatten();
            show(cached.and_then(|c| Pixbuf::from_file(c).ok()));
        });
    }
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
    let res = set_wallpaper(&mut ctx.mgr.borrow_mut(), path);
    match res {
        Ok(r) => report_apply(ctx, parent, &r, Some("Wallpaper set")),
        Err(e) => ctx.toast(&format!("Wallpaper: {:#}", e)),
    }
    ctx.changed();
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

    // keep the "In Mango" mark right after a preset or a pick
    let c = ctx.clone();
    ctx.on_change(move || {
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
