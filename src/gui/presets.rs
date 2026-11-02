//! Preset cards. Like `mangowm-mod preset apply`, a preset is applied at once
//! (together with anything else that is staged). The user's own presets
//! (`mangowm-mod preset save`) show after the built-in ones.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::{gio, glib};

use mangowm_mod::core::schema::find_option;
use mangowm_mod::presets::user::staged_wallpaper;
use mangowm_mod::presets::{
    all_presets, apply_preset, current_preset_id, delete_user_preset, overwrite_user_preset,
    preset_modified, rename_user_preset, save_user_preset, wallpaper_path, Preset,
};

use crate::ctx::Ctx;
use crate::logic::{color_to_rgba, layout_of};
use crate::window::report_apply;

/// Colored dots for the preset's color settings.
fn swatches(p: &Preset) -> gtk::Box {
    let bx = gtk::Box::builder().spacing(6).build();
    for (key, value) in &p.settings {
        let is_color = find_option(key).is_some_and(|o| o.r#type == "color");
        let Some(c) = is_color.then(|| color_to_rgba(value)).flatten() else {
            continue;
        };
        let dot = gtk::DrawingArea::builder()
            .content_width(18)
            .content_height(18)
            .tooltip_text(key)
            .css_classes(["preset-dot"])
            .build();
        dot.set_draw_func(move |_, cr, w, h| {
            let r = w.min(h) as f64 / 2.0;
            cr.arc(
                w as f64 / 2.0,
                h as f64 / 2.0,
                r,
                0.0,
                std::f64::consts::TAU,
            );
            cr.set_source_rgba(c[0] as f64, c[1] as f64, c[2] as f64, 1.0);
            let _ = cr.fill();
        });
        bx.append(&dot);
    }
    bx
}

/// Wallpaper thumbnails (`data/wallpapers/thumbs`, made by `render.py`), built in
/// so the cards look right before anything is installed.
macro_rules! thumbs {
    ($($id:literal),* $(,)?) => {
        pub(crate) fn thumb(id: &str) -> Option<&'static [u8]> {
            match id {
                $($id => Some(include_bytes!(concat!(
                    "../../data/wallpapers/thumbs/", $id, ".png"
                ))),)*
                _ => None,
            }
        }
    };
}

thumbs!(
    "clean",
    "tokyo-night",
    "nord",
    "gruvbox",
    "glass",
    "zen",
    "everforest",
    "rose-pine",
    "kanagawa",
    "solarized",
    "latte",
    "graphite",
);

fn num(p: &Preset, key: &str) -> f64 {
    p.settings
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

pub(crate) fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let q = std::f64::consts::FRAC_PI_2;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -q, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, q);
    cr.arc(x + r, y + h - r, r, q, 2.0 * q);
    cr.arc(x + r, y + r, r, 2.0 * q, 3.0 * q);
    cr.close_path();
}

/// Mock of three tiled windows (master + stack) under a thin bar, in the
/// preset's colors, gaps and radius. Sizes are drawn as on a 640 px wide
/// screen so gaps and corners stay visible on the small card.
fn draw_windows(p: &Preset, cr: &gtk::cairo::Context, w: f64, h: f64) {
    let s = w / 640.0;
    let outer = num(p, "gap_outer_horizontal") * s;
    let inner = num(p, "gap_inner_horizontal") * s;
    let radius = num(p, "border_radius") * s;
    let bpx = num(p, "border_px");
    let bw = if bpx > 0.0 { (bpx * s).max(1.0) } else { 0.0 };
    let color = |k: &str| p.settings.get(k).and_then(|v| color_to_rgba(v));
    let border = color("border_color").unwrap_or([0.12, 0.12, 0.14, 1.0]);
    let focus = color("focus_color").unwrap_or(border);
    // window body: the border color, lifted on light themes and deepened on dark ones
    let lum = 0.2126 * border[0] + 0.7152 * border[1] + 0.0722 * border[2];
    let (tint, k) = if lum > 0.5 { (1.0, 0.6) } else { (0.0, 0.3) };
    let body: Vec<f64> = border[..3]
        .iter()
        .map(|c| *c as f64 + (tint - *c as f64) * k)
        .collect();
    let ink = if lum > 0.5 { 0.0 } else { 1.0 };
    let opacity = |key: &str| match num(p, key) {
        a if a > 0.0 => a,
        _ => 1.0,
    };

    // the bar
    let bar = 16.0 * s;
    cr.rectangle(0.0, 0.0, w, bar);
    cr.set_source_rgba(body[0], body[1], body[2], 0.8);
    let _ = cr.fill();
    cr.set_source_rgba(focus[0] as f64, focus[1] as f64, focus[2] as f64, 1.0);
    rounded_rect(cr, 10.0 * s, 5.0 * s, 14.0 * s, 6.0 * s, 3.0 * s);
    let _ = cr.fill();

    let (x0, y0) = (outer, bar + outer);
    let (aw, ah) = (w - 2.0 * outer, h - y0 - outer);
    let mw = (aw - inner) * 0.55;
    let sh = (ah - inner) / 2.0;
    let (sx, sw) = (x0 + mw + inner, aw - mw - inner);
    let wins = [
        (x0, y0, mw, ah, true),
        (sx, y0, sw, sh, false),
        (sx, y0 + sh + inner, sw, ah - sh - inner, false),
    ];
    for (x, y, ww, wh, focused) in wins {
        let key = if focused { "focused_opacity" } else { "unfocused_opacity" };
        // a little see-through so the wallpaper still shows on the card
        rounded_rect(cr, x, y, ww, wh, radius);
        cr.set_source_rgba(body[0], body[1], body[2], opacity(key) * 0.6);
        let _ = cr.fill_preserve();
        if bw > 0.0 {
            let c = if focused { focus } else { border };
            cr.set_source_rgba(c[0] as f64, c[1] as f64, c[2] as f64, 1.0);
            cr.set_line_width(bw);
            let _ = cr.stroke();
        } else {
            cr.new_path();
        }
        // two faint lines of "content"
        cr.set_source_rgba(ink, ink, ink, 0.16);
        let pad = 14.0 * s;
        for (i, f) in [0.6, 0.35].iter().enumerate() {
            let ly = y + pad + i as f64 * 12.0 * s;
            rounded_rect(cr, x + pad, ly, (ww - 2.0 * pad) * f, 5.0 * s, 2.5 * s);
            let _ = cr.fill();
        }
    }
}

/// Decoded pixels a worker thread can hand back (a Pixbuf is not Send).
pub(crate) struct Pixels {
    bytes: glib::Bytes,
    alpha: bool,
    bits: i32,
    size: (i32, i32),
    stride: i32,
}

impl Pixels {
    pub(crate) fn of(pb: &Pixbuf) -> Self {
        Self {
            bytes: pb.read_pixel_bytes(),
            alpha: pb.has_alpha(),
            bits: pb.bits_per_sample(),
            size: (pb.width(), pb.height()),
            stride: pb.rowstride(),
        }
    }

    pub(crate) fn pixbuf(self) -> Pixbuf {
        Pixbuf::from_bytes(
            &self.bytes,
            gtk::gdk_pixbuf::Colorspace::Rgb,
            self.alpha,
            self.bits,
            self.size.0,
            self.size.1,
            self.stride,
        )
    }
}

/// Wallpaper thumbnail with the mock windows on top. `wallpaper` is a user
/// preset's own picture (built-in presets use their bundled thumbnail).
fn preview(p: &Preset, wallpaper_file: Option<PathBuf>) -> gtk::Widget {
    let area = gtk::DrawingArea::builder()
        .content_height(150)
        .content_width(200)
        .css_classes(["preset-preview"])
        .build();
    let wallpaper: Rc<RefCell<Option<Pixbuf>>> = Rc::default();
    let source = match (p.user, thumb(&p.id), wallpaper_file) {
        (false, Some(bytes), _) => Some(Ok(bytes)),
        (true, _, Some(path)) => Some(Err(path)),
        _ => None,
    };
    if let Some(source) = source {
        // decoded on a worker thread; the mock windows show at once
        let (area, wallpaper) = (area.clone(), wallpaper.clone());
        glib::spawn_future_local(async move {
            let pixels = gio::spawn_blocking(move || {
                let pb = match source {
                    Ok(bytes) => {
                        let stream =
                            gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
                        Pixbuf::from_stream(&stream, gio::Cancellable::NONE)
                    }
                    Err(path) => Pixbuf::from_file_at_scale(&path, 480, 360, true),
                };
                pb.ok().map(|pb| Pixels::of(&pb))
            })
            .await
            .ok()
            .flatten();
            if let Some(px) = pixels {
                *wallpaper.borrow_mut() = Some(px.pixbuf());
                area.queue_draw();
            }
        });
    }
    let pr = p.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
        cr.clip();
        if let Some(pb) = &*wallpaper.borrow() {
            // cover: fill the card, crop the overflow evenly
            let (pw, ph) = (pb.width() as f64, pb.height() as f64);
            let k = (w / pw).max(h / ph);
            cr.save().ok();
            cr.translate((w - pw * k) / 2.0, (h - ph * k) / 2.0);
            cr.scale(k, k);
            cr.set_source_pixbuf(pb, 0.0, 0.0);
            let _ = cr.paint();
            cr.restore().ok();
        }
        draw_windows(&pr, cr, w, h);
    });
    area.upcast()
}

fn apply(ctx: &Ctx, parent: &gtk::Widget, p: &Preset) {
    let res = apply_preset(&mut ctx.mgr.borrow_mut(), p);
    match res {
        Ok(r) => report_apply(ctx, parent, &r, Some(&format!("Preset {} applied", p.name))),
        Err(e) => ctx.toast(&format!("Preset {}: {:#}", p.name, e)),
    }
    ctx.changed();
}

fn confirm(ctx: &Ctx, parent: &gtk::Widget, p: &Preset) {
    let what = match (p.user, &p.wallpaper) {
        (true, None) => "",
        _ => " plus the wallpaper",
    };
    let mut body = format!(
        "This writes {} tiling and look settings{} to mango-mod.conf and reloads MangoWM now.",
        p.settings.len(),
        what
    );
    if ctx.mgr.borrow().is_dirty() {
        body.push_str(" Your other staged changes are applied too.");
    }
    let dialog = adw::AlertDialog::builder()
        .heading(format!("Apply “{}”?", p.name))
        .body(body)
        .default_response("apply")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("apply", "Apply");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    let (ctx, p, par) = (ctx.clone(), p.clone(), parent.clone());
    dialog.connect_response(Some("apply"), move |_, _| apply(&ctx, &par, &p));
    dialog.present(Some(parent));
}

/// Ask for a preset name; `wallpaper` adds an "Include wallpaper" switch.
/// `done` gets the name and the switch.
fn name_dialog(
    parent: &gtk::Widget,
    heading: &str,
    body: &str,
    action: &str,
    name: &str,
    wallpaper: Option<Option<PathBuf>>,
    done: impl Fn(&str, bool) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .default_response("ok")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("ok", action);
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let entry = adw::EntryRow::builder().title("Name").text(name).build();
    list.append(&entry);
    let switch = wallpaper.map(|current| {
        let file = current
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned());
        let row = adw::SwitchRow::builder()
            .title("Include wallpaper")
            .subtitle(file.as_deref().unwrap_or("No wallpaper set"))
            .sensitive(file.is_some())
            .build();
        list.append(&row);
        row
    });
    dialog.set_extra_child(Some(&list));
    let empty = |e: &adw::EntryRow| e.text().trim().is_empty();
    dialog.set_response_enabled("ok", !empty(&entry));
    let d = dialog.clone();
    entry.connect_changed(move |e| d.set_response_enabled("ok", !empty(e)));
    dialog.connect_response(Some("ok"), move |_, _| {
        done(&entry.text(), switch.as_ref().is_some_and(|s| s.is_active()));
    });
    dialog.present(Some(parent));
}

/// Yes/No for something that can't be undone.
fn confirm_destructive(
    parent: &gtk::Widget,
    heading: &str,
    body: &str,
    action: &str,
    done: impl Fn() + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .default_response("cancel")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("ok", action);
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("ok"), move |_, _| done());
    dialog.present(Some(parent));
}

/// Rename / overwrite / delete for a user preset.
fn user_menu(page: &Rc<Page>, p: &Preset) -> gtk::MenuButton {
    let items = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let popover = gtk::Popover::builder().child(&items).build();
    let menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("More")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .popover(&popover)
        .build();
    let item = |label: &str, f: Box<dyn Fn(&gtk::Widget)>| {
        let b = gtk::Button::builder()
            .label(label)
            .css_classes(["flat"])
            .build();
        if let Some(l) = b.child().and_downcast::<gtk::Label>() {
            l.set_xalign(0.0);
        }
        let (pop, anchor) = (popover.clone(), menu.clone());
        b.connect_clicked(move |_| {
            pop.popdown();
            f(anchor.upcast_ref());
        });
        items.append(&b);
    };

    let (pg, pr) = (page.clone(), p.clone());
    item(
        "Rename…",
        Box::new(move |w| {
            let pg = pg.clone();
            let id = pr.id.clone();
            name_dialog(w, "Rename Preset", "", "Rename", &pr.name, None, move |name, _| {
                let res = rename_user_preset(&pg.ctx.data_dir, &id, name);
                pg.done(res.map(|p| format!("Renamed to {}", p.name)));
            });
        }),
    );
    let (pg, pr) = (page.clone(), p.clone());
    item(
        "Overwrite with current settings",
        Box::new(move |w| {
            let pg = pg.clone();
            let id = pr.id.clone();
            confirm_destructive(
                w,
                &format!("Overwrite “{}”?", pr.name),
                "Its saved values are replaced with your current settings.",
                "Overwrite",
                move || {
                    let res = overwrite_user_preset(&pg.ctx.mgr.borrow(), &id);
                    pg.done(res.map(|p| format!("Preset {} updated", p.name)));
                },
            );
        }),
    );
    let (pg, pr) = (page.clone(), p.clone());
    item(
        "Delete…",
        Box::new(move |w| {
            let pg = pg.clone();
            let id = pr.id.clone();
            confirm_destructive(
                w,
                &format!("Delete “{}”?", pr.name),
                "The preset is removed. Your current settings stay as they are.",
                "Delete",
                move || {
                    let res = delete_user_preset(&pg.ctx.data_dir, &id);
                    pg.done(res.map(|p| format!("Preset {} deleted", p.name)));
                },
            );
        }),
    );
    menu
}

/// A card and its "Current" / "Modified" mark.
struct Card {
    preset: Preset,
    wallpaper: Option<PathBuf>,
    status: gtk::Label,
}

fn card(page: &Rc<Page>, p: &Preset) -> (gtk::Widget, Card) {
    let wallpaper = wallpaper_path(&page.ctx.data_dir, p);
    let bx = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .width_request(240)
        .css_classes(["card", "preset-card"])
        .build();
    bx.append(&preview(p, wallpaper.clone()));
    let top = gtk::Box::builder().spacing(8).build();
    top.append(
        &gtk::Label::builder()
            .label(&p.name)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["title-4"])
            .build(),
    );
    if p.user {
        top.append(
            &gtk::Label::builder()
                .label("yours")
                .valign(gtk::Align::Center)
                .tooltip_text("Saved by you")
                .css_classes(["caption", "dim-label"])
                .build(),
        );
    }
    top.append(&gtk::Box::builder().hexpand(true).build());
    let status = gtk::Label::builder()
        .visible(false)
        .css_classes(["preset-chip"])
        .build();
    top.append(&status);
    if let Some(layout) = p.settings.get("tag_rule").and_then(|v| layout_of(v)) {
        top.append(
            &gtk::Label::builder()
                .label(&layout)
                .css_classes(["preset-chip"])
                .build(),
        );
    }
    bx.append(&top);
    bx.append(
        &gtk::Label::builder()
            .label(&p.description)
            .xalign(0.0)
            .wrap(true)
            .lines(3)
            .max_width_chars(28)
            .css_classes(["dim-label"])
            .build(),
    );
    let bottom = gtk::Box::builder()
        .spacing(8)
        .valign(gtk::Align::End)
        .vexpand(true)
        .build();
    let dots = swatches(p);
    dots.set_hexpand(true);
    dots.set_valign(gtk::Align::Center);
    bottom.append(&dots);
    if p.user {
        bottom.append(&user_menu(page, p));
    }
    let btn = gtk::Button::builder()
        .label("Apply")
        .tooltip_text(format!("{} settings, applied at once", p.settings.len()))
        .css_classes(["pill", "preset-apply"])
        .build();
    let (pg, pr) = (page.clone(), p.clone());
    btn.connect_clicked(move |b| confirm(&pg.ctx, b.upcast_ref(), &pr));
    bottom.append(&btn);
    bx.append(&bottom);
    let card = Card {
        preset: p.clone(),
        wallpaper,
        status,
    };
    (bx.upcast(), card)
}

struct Page {
    ctx: Ctx,
    flow: gtk::FlowBox,
    cards: RefCell<Vec<Card>>,
}

impl Page {
    /// (Re)build the cards: built-in presets, then the user's.
    fn fill(self: &Rc<Self>) {
        while let Some(c) = self.flow.first_child() {
            self.flow.remove(&c);
        }
        let mut cards = Vec::new();
        for p in all_presets(&self.ctx.data_dir) {
            let (w, c) = card(self, &p);
            self.flow.append(&w);
            cards.push(c);
        }
        *self.cards.borrow_mut() = cards;
        self.refresh();
    }

    /// Mark the current preset, and whether the settings moved away from it.
    fn refresh(&self) {
        let current = current_preset_id(&self.ctx.data_dir);
        let mgr = self.ctx.mgr.borrow();
        for c in self.cards.borrow().iter() {
            let is_current = current.as_deref() == Some(c.preset.id.as_str());
            c.status.set_visible(is_current);
            if !is_current {
                continue;
            }
            let modified = preset_modified(&mgr, &c.preset, c.wallpaper.as_deref());
            c.status.set_label(if modified { "Modified" } else { "Current" });
            c.status.set_tooltip_text(Some(if modified {
                "Your settings differ from this preset"
            } else {
                "Your settings match this preset"
            }));
            if modified {
                c.status.add_css_class("dim-label");
            } else {
                c.status.remove_css_class("dim-label");
            }
        }
    }

    /// After a save, rename, overwrite or delete: toast and rebuild the cards.
    fn done(self: &Rc<Self>, res: anyhow::Result<String>) {
        match res {
            Ok(msg) => self.ctx.toast(&msg),
            Err(e) => self.ctx.toast(&format!("{:#}", e)),
        }
        self.fill();
    }

    fn save_dialog(self: &Rc<Self>, parent: &gtk::Widget) {
        let current = staged_wallpaper(&self.ctx.mgr.borrow()).filter(|p| p.is_file());
        let pg = self.clone();
        name_dialog(
            parent,
            "Save as Preset",
            "Saves your current tiling, gap, border and color settings as your own preset.",
            "Save",
            "",
            Some(current),
            move |name, wallpaper| {
                let res = save_user_preset(&pg.ctx.mgr.borrow(), name, wallpaper);
                pg.done(res.map(|p| format!("Saved preset {}", p.name)));
            },
        );
    }
}

pub fn page(ctx: &Ctx) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(3)
        .column_spacing(12)
        .row_spacing(12)
        .valign(gtk::Align::Start)
        .build();
    let page = Rc::new(Page {
        ctx: ctx.clone(),
        flow: flow.clone(),
        cards: RefCell::default(),
    });
    page.fill();
    let pg = page.clone();
    ctx.on_change(move || pg.refresh());

    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    let head = gtk::Box::builder().spacing(12).build();
    head.append(
        &gtk::Label::builder()
            .label("Ready-made looks. A preset is written and reloaded right away; a backup is made first.")
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let save = gtk::Button::builder()
        .label("Save current settings as preset…")
        .valign(gtk::Align::Center)
        .build();
    let pg = page.clone();
    save.connect_clicked(move |b| pg.save_dialog(b.upcast_ref()));
    head.append(&save);
    col.append(&head);
    col.append(&flow);
    let clamp = adw::Clamp::builder()
        .maximum_size(980)
        .margin_top(24)
        // room for the floating apply bar under the last row
        .margin_bottom(88)
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
