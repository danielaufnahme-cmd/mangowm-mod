//! Bar page: preset cards, each with a drawing of the bar as the selected
//! program will show it.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{cairo, gio, glib};

use mangowm_mod::bar::generate::{alpha, mangobar_continuous, surface_color};
use mangowm_mod::bar::model::{module_base, Backend, BarConfig};
use mangowm_mod::bar::presets::{
    delete_user_preset, find_user_preset, list_user_presets, overwrite_user_preset,
    rename_user_preset, save_user_preset, BarPreset, UserPreset, BAR_PRESETS,
};

use super::BarPage;
use crate::logic::color_to_rgba;
use crate::presets::rounded_rect;

/// Width of the screen the preview is drawn for, in bar pixels.
const SCREEN_W: f64 = 600.0;
/// Right-section modules drawn (plus the tray), so the card is not crowded.
const RIGHT_MAX: usize = 2;


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

/// A card's preset: built-in (read-only) or the user's own.
#[derive(Clone)]
pub enum Choice {
    Builtin(&'static BarPreset),
    User(UserPreset),
}

impl Choice {
    fn id(&self) -> String {
        match self {
            Choice::Builtin(p) => p.id.to_string(),
            Choice::User(p) => p.id(),
        }
    }

    fn name(&self) -> String {
        match self {
            Choice::Builtin(p) => p.name.to_string(),
            Choice::User(p) => p.name.clone(),
        }
    }

    fn apply(&self, bar: &mut BarConfig) {
        match self {
            Choice::Builtin(p) => p.apply(bar),
            Choice::User(p) => p.apply(bar),
        }
    }

    fn applied_to(&self, bar: &BarConfig) -> BarConfig {
        let mut out = bar.clone();
        self.apply(&mut out);
        out
    }
}

struct Card {
    choice: Choice,
    area: gtk::DrawingArea,
    chip: gtk::Label,
}

pub struct Presets {
    flow: gtk::FlowBox,
    save: gtk::Button,
    cards: RefCell<Vec<Card>>,
    /// The user presets on disk (`None`: read again).
    user: RefCell<Option<Vec<UserPreset>>>,
    /// The user presets the cards were built for.
    built_for: RefCell<Option<String>>,
    /// The saved bar; the previews show each preset applied to it.
    current: Rc<RefCell<BarConfig>>,
}

impl Presets {
    pub fn new(page: &adw::PreferencesPage) -> Self {
        let save = gtk::Button::builder()
            .label("Save as Preset…")
            .tooltip_text("Save the current bar look as your own preset")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Presets")
            .description(
                "Built-in looks set colors, sizes and module layout. Your presets keep \
                 everything except the program, login start and monitors. Previews show \
                 the selected program",
            )
            .header_suffix(&save)
            .build();
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(1)
            .max_children_per_line(3)
            .column_spacing(12)
            .row_spacing(12)
            .build();
        group.add(&flow);
        page.add(&group);
        Self {
            flow,
            save,
            cards: RefCell::new(Vec::new()),
            user: RefCell::new(None),
            built_for: RefCell::new(None),
            current: Rc::new(RefCell::new(BarConfig::default())),
        }
    }
}

impl BarPage {
    /// Build the cards (again when the user presets changed), redraw the
    /// previews and mark the preset in use ("Current" / "Modified").
    pub(super) fn load_presets(self: &Rc<Self>, bar: &BarConfig) {
        *self.presets.current.borrow_mut() = bar.clone();
        // read once; the preset actions clear it
        let user = self
            .presets
            .user
            .borrow_mut()
            .get_or_insert_with(|| list_user_presets(&self.ctx.data_dir))
            .clone();
        let sig = serde_json::to_string(
            &user.iter().map(|p| (&p.slug, p)).collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        if self.presets.built_for.borrow().as_deref() != Some(sig.as_str()) {
            while let Some(child) = self.presets.flow.first_child() {
                self.presets.flow.remove(&child);
            }
            let mut cards = Vec::new();
            let choices = BAR_PRESETS
                .iter()
                .map(Choice::Builtin)
                .chain(user.into_iter().map(Choice::User));
            for choice in choices {
                cards.push(self.preset_card(choice));
            }
            *self.presets.cards.borrow_mut() = cards;
            *self.presets.built_for.borrow_mut() = Some(sig);
        }
        let applied = bar.preset.as_deref();
        for card in self.presets.cards.borrow().iter() {
            let id = card.choice.id();
            let same = card.choice.applied_to(bar) == *bar;
            let text = match applied {
                Some(a) if a == id => Some(if same { "Current" } else { "Modified" }),
                // settings from before presets were remembered
                None if same => Some("Current"),
                _ => None,
            };
            card.chip.set_visible(text.is_some());
            card.chip.set_label(text.unwrap_or(""));
            card.area.queue_draw();
        }
    }

    fn preset_card(self: &Rc<Self>, choice: Choice) -> Card {
        let area = gtk::DrawingArea::builder()
            .content_height(44)
            .content_width(220)
            .hexpand(true)
            .css_classes(["preset-preview"])
            .build();
        let (cur, draw) = (self.presets.current.clone(), choice.clone());
        area.set_draw_func(move |_, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
            cr.clip();
            draw_bar(cr, &draw.applied_to(&cur.borrow()), w, h);
        });
        let bx = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        bx.append(&area);
        let top = gtk::Box::builder().spacing(8).build();
        top.append(
            &gtk::Label::builder()
                .label(choice.name())
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
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
        let description = match &choice {
            Choice::Builtin(p) => p.description.to_string(),
            Choice::User(_) => "Yours".to_string(),
        };
        bx.append(
            &gtk::Label::builder()
                .label(description)
                .xalign(0.0)
                .wrap(true)
                .lines(3)
                .max_width_chars(30)
                .css_classes(["dim-label", "caption"])
                .build(),
        );
        let btn = gtk::Button::builder()
            .child(&bx)
            .tooltip_text(format!("Apply {}", choice.name()))
            .css_classes(["card", "preset-card"])
            .build();
        let st = Rc::downgrade(self);
        let clicked = choice.clone();
        btn.connect_clicked(move |_| {
            if let Some(st) = st.upgrade() {
                st.confirm_preset(clicked.clone());
            }
        });
        let widget: gtk::Widget = match &choice {
            Choice::Builtin(_) => btn.upcast(),
            Choice::User(p) => {
                // rename / overwrite / delete through the page's "bar" actions
                let menu = gio::Menu::new();
                for (label, action) in [
                    ("Rename…", "bar.preset-rename"),
                    ("Overwrite with Current Settings", "bar.preset-overwrite"),
                    ("Delete…", "bar.preset-delete"),
                ] {
                    let item = gio::MenuItem::new(Some(label), None);
                    item.set_action_and_target_value(Some(action), Some(&p.slug.to_variant()));
                    menu.append_item(&item);
                }
                let more = gtk::MenuButton::builder()
                    .icon_name("view-more-symbolic")
                    .tooltip_text("Preset options")
                    .menu_model(&menu)
                    .halign(gtk::Align::End)
                    .valign(gtk::Align::End)
                    .margin_end(6)
                    .margin_bottom(6)
                    .css_classes(["flat", "circular"])
                    .build();
                let overlay = gtk::Overlay::builder().child(&btn).build();
                overlay.add_overlay(&more);
                overlay.upcast()
            }
        };
        self.presets.flow.append(&widget);
        Card { choice, area, chip }
    }

    pub(super) fn connect_presets(self: &Rc<Self>) {
        let st = Rc::downgrade(self);
        self.presets.save.connect_clicked(move |_| {
            if let Some(st) = st.upgrade() {
                st.ask_preset_name(None);
            }
        });
    }

    /// The "bar" action group's preset actions (the card menus).
    pub(super) fn add_preset_actions(self: &Rc<Self>, group: &gio::SimpleActionGroup) {
        for name in ["preset-rename", "preset-overwrite", "preset-delete"] {
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let st = Rc::downgrade(self);
            action.connect_activate(move |_, arg| {
                let (Some(st), Some(slug)) = (st.upgrade(), arg.and_then(|v| v.get::<String>()))
                else {
                    return;
                };
                match name {
                    "preset-rename" => st.ask_preset_name(Some(slug)),
                    "preset-overwrite" => st.overwrite_preset(&slug),
                    _ => st.confirm_delete_preset(&slug),
                }
            });
            group.add_action(&action);
        }
    }

    fn confirm_preset(self: &Rc<Self>, choice: Choice) {
        let replaces = match choice {
            Choice::Builtin(_) => "the bar's colors, sizes and module layout",
            Choice::User(_) => "all bar settings except the program, login start and monitors",
        };
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Use “{}”?", choice.name()))
            .body(format!(
                "This replaces {}, then writes the bar files and updates a running bar.",
                replaces
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
                st.apply_preset(&choice);
            }
        });
        dialog.present(Some(&self.page));
    }

    /// Copy the preset into the saved settings; the live update writes the
    /// files and updates a running bar, as for any other change.
    fn apply_preset(self: &Rc<Self>, choice: &Choice) {
        self.edit(|bar| {
            choice.apply(bar);
            bar.preset = Some(choice.id());
            Ok(())
        });
        let note = self.sync_login(&self.bar()).unwrap_or("");
        self.ctx
            .toast(&format!("Bar preset “{}” applied{}", choice.name(), note));
    }

    /// Name dialog: save a new preset (`slug` None) or rename one.
    fn ask_preset_name(self: &Rc<Self>, slug: Option<String>) {
        let old = slug
            .as_deref()
            .and_then(|s| find_user_preset(&self.ctx.data_dir, s));
        let entry = gtk::Entry::builder()
            .placeholder_text("Name")
            .activates_default(true)
            .text(old.as_ref().map(|p| p.name.as_str()).unwrap_or(""))
            .build();
        let (heading, ok) = match &old {
            Some(_) => ("Rename Preset", "Rename"),
            None => ("Save as Preset", "Save"),
        };
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(if old.is_some() {
                ""
            } else {
                "Saves everything except the program, login start and monitors."
            })
            .extra_child(&entry)
            .default_response("ok")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("ok", ok);
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        let st = Rc::downgrade(self);
        dialog.connect_response(Some("ok"), move |_, _| {
            let Some(st) = st.upgrade() else {
                return;
            };
            let name = entry.text().to_string();
            let data_dir = st.ctx.data_dir.clone();
            st.presets.user.take();
            let res = match &slug {
                Some(s) => rename_user_preset(&data_dir, s, &name).map(|_| None),
                None => save_user_preset(&data_dir, &name, &st.bar()).map(Some),
            };
            match res {
                Ok(Some(p)) => {
                    // the bar now is this preset
                    let id = p.id();
                    st.edit(|bar| {
                        bar.preset = Some(id);
                        Ok(())
                    });
                    st.ctx.toast(&format!("Preset “{}” saved", p.name));
                }
                Ok(None) => st.load(),
                Err(e) => st.ctx.toast(&format!("{:#}", e)),
            }
        });
        dialog.present(Some(&self.page));
    }

    fn overwrite_preset(self: &Rc<Self>, slug: &str) {
        self.presets.user.take();
        match overwrite_user_preset(&self.ctx.data_dir, slug, &self.bar()) {
            Ok(p) => {
                let id = p.id();
                self.edit(|bar| {
                    bar.preset = Some(id);
                    Ok(())
                });
                self.ctx
                    .toast(&format!("Preset “{}” now has the current settings", p.name));
            }
            Err(e) => self.ctx.toast(&format!("{:#}", e)),
        }
    }

    fn confirm_delete_preset(self: &Rc<Self>, slug: &str) {
        let Some(p) = find_user_preset(&self.ctx.data_dir, slug) else {
            return;
        };
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Delete “{}”?", p.name))
            .body("The preset is deleted. The current bar settings stay as they are.")
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let st = Rc::downgrade(self);
        dialog.connect_response(Some("delete"), move |_, _| {
            let Some(st) = st.upgrade() else {
                return;
            };
            let id = p.id();
            st.presets.user.take();
            match delete_user_preset(&st.ctx.data_dir, &id) {
                Ok(()) => {
                    if st.bar().preset.as_deref() == Some(id.as_str()) {
                        st.edit(|bar| {
                            bar.preset = None;
                            Ok(())
                        });
                    } else {
                        st.load();
                    }
                    st.ctx.toast(&format!("Preset “{}” deleted", p.name));
                }
                Err(e) => st.ctx.toast(&format!("{:#}", e)),
            }
        });
        dialog.present(Some(&self.page));
    }
}
