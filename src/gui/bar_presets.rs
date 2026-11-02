//! Bar page: preset cards, each with a drawing of the bar as the selected
//! program will show it.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::cairo;

use mangowm_mod::bar::generate::{alpha, mangobar_continuous, surface_color};
use mangowm_mod::bar::model::{module_base, Backend, BarConfig};
use mangowm_mod::bar::presets::{BarPreset, BAR_PRESETS};
use mangowm_mod::bar::process::our_bar_running;

use super::BarPage;
use crate::logic::color_to_rgba;
use crate::presets::rounded_rect;

/// Width of the screen the preview is drawn for, in bar pixels.
const SCREEN_W: f64 = 600.0;
/// Right-section modules drawn (plus the tray), so the card is not crowded.
const RIGHT_MAX: usize = 2;

pub struct Presets {
    cards: Vec<(&'static BarPreset, gtk::Button, gtk::DrawingArea, gtk::Label)>,
    /// The saved bar; the previews show each preset applied to it.
    current: Rc<RefCell<BarConfig>>,
}

fn set_color(cr: &cairo::Context, color: &str) {
    let c = color_to_rgba(color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    cr.set_source_rgba(c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64);
}

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    Active,
    Occupied,
    No,
}

/// Sample text of a module; `None` for the modules the preview leaves out.
fn sample(name: &str) -> Option<&'static str> {
    Some(match module_base(name) {
        "layout" => "[]=",
        "window" => "fish",
        "clock" => "13:02",
        "cpu" => "cpu 4%",
        "memory" => "mem 31%",
        "pulseaudio" => "vol 55%",
        "network" => "wifi",
        "battery" => "bat 82%",
        "backlight" => "bri 60%",
        _ => return None,
    })
}

/// One drawn module: text (or tray icons), tag state.
type Entry = (&'static str, Tag);

fn entries(list: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    for m in list {
        match m.as_str() {
            "workspaces" => out.extend([
                ("1", Tag::Occupied),
                ("2", Tag::Active),
                ("3", Tag::Occupied),
            ]),
            "tray" => out.push(("", Tag::No)),
            _ => {
                if let Some(t) = sample(m) {
                    out.push((t, Tag::No));
                }
            }
        }
    }
    out
}

/// The bar as Mangobar / Waybar lays it out (see `generate_style`), drawn
/// over a neutral backdrop with a window below it.
fn draw_bar(cr: &cairo::Context, bar: &BarConfig, w: f64, h: f64) {
    // backdrop: a calm dark gradient, like a dimmed wallpaper
    let grad = cairo::LinearGradient::new(0.0, 0.0, w, h);
    grad.add_color_stop_rgb(0.0, 0.29, 0.34, 0.38);
    grad.add_color_stop_rgb(1.0, 0.13, 0.15, 0.18);
    let _ = cr.set_source(&grad);
    let _ = cr.paint();

    let k = w / SCREEN_W;
    cr.save().ok();
    cr.scale(k, k);
    let height = bar.effective_height() as f64;
    let top = bar.margin_top as f64;
    let (x0, x1) = (
        bar.margin_left as f64,
        SCREEN_W - bar.margin_right as f64,
    );
    // a window under the bar
    let wy = top + height + 8.0;
    rounded_rect(cr, x0.max(8.0), wy, SCREEN_W - 2.0 * x0.max(8.0), h / k, 10.0);
    cr.set_source_rgba(0.08, 0.09, 0.11, 0.55);
    let _ = cr.fill();

    let family = bar
        .font
        .split(',')
        .map(str::trim)
        .find(|f| f.contains("Nerd"))
        .or(bar.font.split(',').map(str::trim).next())
        .unwrap_or("monospace")
        .to_string();
    let weight = if bar.font_weight == "normal" {
        cairo::FontWeight::Normal
    } else {
        cairo::FontWeight::Bold
    };
    cr.select_font_face(&family, cairo::FontSlant::Normal, weight);
    cr.set_font_size(bar.font_size as f64);
    let ext = cr.font_extents().ok();
    let (ascent, descent) = ext.map(|e| (e.ascent(), e.descent())).unwrap_or((10.0, 3.0));
    let text_w = |t: &str| cr.text_extents(t).map(|e| e.x_advance()).unwrap_or(0.0);

    let waybar = bar.backend == Backend::Waybar;
    let continuous = mangobar_continuous(bar);
    let bw = if waybar { bar.border_width as f64 } else { 0.0 };
    let pill = |r: i32, hh: f64| if r < 0 { hh / 2.0 } else { (r as f64).min(hh / 2.0) };

    // the bar body: Waybar draws a rounded window, Mangobar a plain strip
    if alpha(&bar.bg_color) > 0 {
        let r = if waybar { pill(bar.radius, height) } else { 0.0 };
        rounded_rect(cr, x0, top, x1 - x0, height, r);
        set_color(cr, &bar.bg_color);
        let _ = cr.fill_preserve();
        if bw > 0.0 {
            set_color(cr, &bar.border_color);
            cr.set_line_width(bw);
            let _ = cr.stroke();
        }
        cr.new_path();
    }

    let (mut pad, mut gap) = (bar.module_padding as f64, bar.module_gap as f64);
    let mut mr = bar.module_radius;
    if continuous {
        pad += gap;
        gap = 0.0;
        mr = 0;
    }
    let (cy, ch) = if waybar {
        let m = bar.module_margin as f64 + bw;
        (top + m, height - 2.0 * m)
    } else {
        (top, height)
    };
    let spacing = if waybar { bar.spacing as f64 } else { 0.0 };
    let width = |e: &Entry| {
        let body = if e.0.is_empty() {
            2.0 * 14.0 + 6.0 + 2.0 * pad
        } else {
            text_w(e.0) + 2.0 * pad
        };
        // Mangobar keeps tags at least as wide as the bar is tall
        let body = if e.1 != Tag::No && !waybar { body.max(height) } else { body };
        body + 2.0 * gap
    };
    let group_w = |list: &[Entry]| {
        list.iter().map(width).sum::<f64>() + spacing * list.len().saturating_sub(1) as f64
    };
    let surface = surface_color(bar);
    let draw_entry = |e: &Entry, x: f64| -> f64 {
        let ew = width(e);
        let bx = x + gap;
        let bwid = ew - 2.0 * gap;
        let (fill, ink) = match e.1 {
            Tag::Active if bar.tag_fill => (bar.accent_color.as_str(), surface.as_str()),
            Tag::Active => (bar.module_bg_color.as_str(), bar.accent_color.as_str()),
            _ => (bar.module_bg_color.as_str(), bar.fg_color.as_str()),
        };
        rounded_rect(cr, bx, cy, bwid, ch, pill(mr, ch));
        set_color(cr, fill);
        let _ = cr.fill();
        if e.1 == Tag::Active && !bar.tag_fill && waybar {
            cr.rectangle(bx, cy + ch - 2.0, bwid, 2.0);
            set_color(cr, &bar.accent_color);
            let _ = cr.fill();
        }
        if e.0.is_empty() {
            // tray: two icons
            for i in 0..2 {
                let ix = bx + pad + i as f64 * 20.0;
                rounded_rect(cr, ix, cy + (ch - 14.0) / 2.0, 14.0, 14.0, 4.0);
                set_color(cr, &bar.inactive_color);
                let _ = cr.fill();
            }
        } else {
            let tw = text_w(e.0);
            let tx = bx + (bwid - tw) / 2.0;
            let ty = cy + (ch + ascent - descent) / 2.0;
            cr.move_to(tx, ty);
            set_color(cr, ink);
            let _ = cr.show_text(e.0);
        }
        x + ew + spacing
    };

    let mut right = entries(&bar.modules.right);
    let tray = right.iter().position(|e| e.0.is_empty()).map(|i| right.remove(i));
    right.truncate(RIGHT_MAX);
    right.extend(tray);
    let (left, center) = (entries(&bar.modules.left), entries(&bar.modules.center));
    let mut x = x0;
    for e in &left {
        x = draw_entry(e, x);
    }
    let mut x = (x0 + x1 - group_w(&center)) / 2.0;
    for e in &center {
        x = draw_entry(e, x);
    }
    // as Mangobar: the right group starts after the center group at the latest
    let mut x = (x1 - group_w(&right)).max(x);
    for e in &right {
        x = draw_entry(e, x);
    }
    cr.restore().ok();
}

impl Presets {
    pub fn new(page: &adw::PreferencesPage) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Presets")
            .description(
                "Ready-made looks: colors, sizes and module layout. The program, monitors, \
                 font family and module settings stay. Previews show the selected program",
            )
            .build();
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(1)
            .max_children_per_line(3)
            .column_spacing(12)
            .row_spacing(12)
            .build();
        let current = Rc::new(RefCell::new(BarConfig::default()));
        let mut cards = Vec::new();
        for p in BAR_PRESETS {
            let area = gtk::DrawingArea::builder()
                .content_height(44)
                .content_width(220)
                .hexpand(true)
                .css_classes(["preset-preview"])
                .build();
            let cur = current.clone();
            area.set_draw_func(move |_, cr, w, h| {
                let (w, h) = (w as f64, h as f64);
                rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
                cr.clip();
                draw_bar(cr, &p.applied_to(&cur.borrow()), w, h);
            });
            let bx = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(8)
                .build();
            bx.append(&area);
            let top = gtk::Box::builder().spacing(8).build();
            top.append(
                &gtk::Label::builder()
                    .label(p.name)
                    .xalign(0.0)
                    .hexpand(true)
                    .css_classes(["heading"])
                    .build(),
            );
            let chip = gtk::Label::builder()
                .label("Current")
                .css_classes(["preset-chip"])
                .visible(false)
                .build();
            top.append(&chip);
            bx.append(&top);
            bx.append(
                &gtk::Label::builder()
                    .label(p.description)
                    .xalign(0.0)
                    .wrap(true)
                    .lines(3)
                    .max_width_chars(30)
                    .css_classes(["dim-label", "caption"])
                    .build(),
            );
            let btn = gtk::Button::builder()
                .child(&bx)
                .tooltip_text(format!("Apply {}", p.name))
                .css_classes(["card", "preset-card"])
                .build();
            flow.append(&btn);
            cards.push((p, btn, area, chip));
        }
        group.add(&flow);
        page.add(&group);
        Self {
            cards,
            current,
        }
    }

    /// Redraw the previews for the saved bar and mark the preset in use.
    pub fn update(&self, bar: &BarConfig) {
        *self.current.borrow_mut() = bar.clone();
        for (p, _, area, chip) in &self.cards {
            chip.set_visible(p.applied_to(bar) == *bar);
            area.queue_draw();
        }
    }
}

impl BarPage {
    pub(super) fn connect_presets(self: &Rc<Self>) {
        for (p, btn, _, _) in &self.presets.cards {
            let (st, p) = (Rc::downgrade(self), *p);
            btn.connect_clicked(move |_| {
                if let Some(st) = st.upgrade() {
                    st.confirm_preset(p);
                }
            });
        }
    }

    fn confirm_preset(self: &Rc<Self>, p: &'static BarPreset) {
        let running = our_bar_running(&self.ctx.data_dir);
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Use “{}”?", p.name))
            .body(format!(
                "This replaces the bar's colors, sizes and module layout, then writes the bar \
                 files{}.",
                if running { " and restarts the bar" } else { "" }
            ))
            .default_response("apply")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("apply", "Use Preset");
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
        let st = Rc::downgrade(self);
        dialog.connect_response(Some("apply"), move |_, _| {
            if let Some(st) = st.upgrade() {
                st.apply_preset(p);
            }
        });
        dialog.present(Some(&self.page));
    }

    /// Save the preset's look, write the files and restart a running bar
    /// (the Start / Write Files flow).
    fn apply_preset(self: &Rc<Self>, p: &BarPreset) {
        self.edit(|bar| {
            p.apply(bar);
            Ok(())
        });
        self.ctx.toast(&format!("Bar preset “{}” saved", p.name));
        if our_bar_running(&self.ctx.data_dir) {
            self.start(&[]);
        } else {
            self.write_files();
        }
    }
}
