//! Preset cards. Like `mangowm-mod preset apply`, a preset is applied at once
//! (together with anything else that is staged).

use adw::prelude::*;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::{gio, glib};

use mangowm_mod::core::schema::find_option;
use mangowm_mod::presets::{apply_preset, list_presets, Preset};

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

/// Wallpaper thumbnail with the mock windows on top.
fn preview(p: &Preset) -> gtk::Widget {
    let area = gtk::DrawingArea::builder()
        .content_height(150)
        .content_width(200)
        .css_classes(["preset-preview"])
        .build();
    let wallpaper = thumb(&p.id).and_then(|b| {
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(b));
        Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()
    });
    let pr = p.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
        cr.clip();
        if let Some(pb) = &wallpaper {
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
    let mut body = format!(
        "This writes {} tiling and look settings plus the wallpaper to mango-mod.conf and \
         reloads MangoWM now.",
        p.settings.len()
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

fn card(ctx: &Ctx, p: &Preset) -> gtk::Widget {
    let bx = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .width_request(240)
        .css_classes(["card", "preset-card"])
        .build();
    bx.append(&preview(p));
    let top = gtk::Box::builder().spacing(8).build();
    top.append(
        &gtk::Label::builder()
            .label(&p.name)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-4"])
            .build(),
    );
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
    let btn = gtk::Button::builder()
        .label("Apply")
        .tooltip_text(format!("{} settings, applied at once", p.settings.len()))
        .css_classes(["pill", "preset-apply"])
        .build();
    let (c, pr) = (ctx.clone(), p.clone());
    btn.connect_clicked(move |b| confirm(&c, b.upcast_ref(), &pr));
    bottom.append(&btn);
    bx.append(&bottom);
    bx.upcast()
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
    for p in list_presets() {
        flow.append(&card(ctx, &p));
    }
    let col = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    col.append(
        &gtk::Label::builder()
            .label("Ready-made looks. A preset is written and reloaded right away; a backup is made first.")
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
