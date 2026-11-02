//! Preference rows generated from the MangoWM option schema.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib::{self, clone};

use mangowm_mod::core::schema::{OptionItem, LAYOUTS};
use mangowm_mod::core::values::{format_float, format_option_value, parse_bool};

use crate::ctx::Ctx;
use crate::logic::{color_to_rgba, default_layout_value, layout_of, rgba_to_hex};

/// One option row plus a way to re-read its value from the manager.
#[derive(Clone)]
pub struct OptionRow {
    pub key: String,
    pub row: adw::PreferencesRow,
    refresh: Rc<dyn Fn()>,
    simulate: Rc<dyn Fn(&str)>,
}

impl OptionRow {
    pub fn refresh(&self) {
        (self.refresh)()
    }

    /// Put `text` into the widget as if the user had changed it (screenshot mode).
    pub fn simulate(&self, text: &str) {
        (self.simulate)(text)
    }
}

/// Controller state every row type shares.
struct Logic {
    ctx: Ctx,
    opt: OptionItem,
    updating: Cell<bool>,
    row: adw::PreferencesRow,
    reset_rev: gtk::Revealer,
}

impl Logic {
    fn new(ctx: &Ctx, opt: &OptionItem, row: &impl IsA<adw::PreferencesRow>) -> Rc<Self> {
        let row = row.clone().upcast::<adw::PreferencesRow>();
        row.set_use_markup(false);
        row.set_title(&opt.label);
        if let Some(ar) = row.downcast_ref::<adw::ActionRow>() {
            if !opt.description.is_empty() {
                ar.set_subtitle(&opt.description);
            }
        }
        let tip = if opt.description.is_empty() || row.is::<adw::ActionRow>() {
            opt.key.clone()
        } else {
            format!("{}\n{}", opt.description, opt.key)
        };
        row.set_tooltip_text(Some(&tip));
        row.add_css_class("option-row");

        let btn = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text(
                "Reset: remove it from mango-mod.conf (back to config.conf or Mango's default)",
            )
            .css_classes(["flat", "circular", "reset-button"])
            .build();
        let reset_rev = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(150)
            .valign(gtk::Align::Center)
            .child(&btn)
            .build();
        let logic = Rc::new(Self {
            ctx: ctx.clone(),
            opt: opt.clone(),
            updating: Cell::new(false),
            row,
            reset_rev,
        });
        btn.connect_clicked(clone!(
            #[weak]
            logic,
            move |_| {
                let key = logic.opt.key.clone();
                logic.ctx.stage(|m| m.reset(&key));
            }
        ));
        logic
    }

    /// The value Mango uses now: staged/managed, else config.conf, else the default.
    fn current(&self) -> String {
        self.ctx
            .mgr
            .borrow()
            .get(&self.opt.key)
            .unwrap_or_else(|| self.opt.default.clone())
    }

    /// Stage `text` unless it is what Mango already has.
    fn commit(&self, text: &str) -> bool {
        if self.updating.get() {
            return true;
        }
        let same = format_option_value(&self.opt.r#type, &self.opt.choices, text)
            .is_ok_and(|v| self.ctx.mgr.borrow().get(&self.opt.key) == Some(v));
        if same {
            return true;
        }
        let key = self.opt.key.clone();
        self.ctx.stage(|m| m.set(&key, text)).is_some()
    }

    /// Run `load` with change signals muted, then show the managed/pending state.
    fn refresh(&self, load: &dyn Fn()) {
        self.updating.set(true);
        load();
        self.updating.set(false);
        let (managed, pending) = {
            let mgr = self.ctx.mgr.borrow();
            let staged = mgr.staged().settings.get(&self.opt.key);
            (
                staged.is_some(),
                staged != mgr.managed().settings.get(&self.opt.key),
            )
        };
        self.reset_rev.set_reveal_child(managed);
        if pending {
            self.row.add_css_class("modified");
        } else {
            self.row.remove_css_class("modified");
        }
    }
}

fn finish(
    logic: Rc<Logic>,
    load: impl Fn() + 'static,
    simulate: impl Fn(&str) + 'static,
) -> OptionRow {
    let refresh: Rc<dyn Fn()> = {
        let logic = logic.clone();
        Rc::new(move || logic.refresh(&load))
    };
    refresh();
    OptionRow {
        key: logic.opt.key.clone(),
        row: logic.row.clone(),
        refresh,
        simulate: Rc::new(simulate),
    }
}

fn switch_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let row = adw::SwitchRow::new();
    let logic = Logic::new(ctx, opt, &row);
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        row,
        #[weak]
        logic,
        move || row.set_active(parse_bool(&logic.current()).unwrap_or(false))
    );
    row.connect_active_notify(clone!(
        #[weak]
        logic,
        move |r| {
            logic.commit(if r.is_active() { "1" } else { "0" });
        }
    ));
    let sim = clone!(
        #[weak]
        row,
        move |t: &str| row.set_active(parse_bool(t).unwrap_or(false))
    );
    finish(logic, load, sim)
}

fn spin_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let is_float = opt.r#type == "float";
    let step = if is_float { 0.01 } else { 1.0 };
    let adj = gtk::Adjustment::new(0.0, -1_000_000.0, 1_000_000.0, step, step * 10.0, 0.0);
    let row = adw::SpinRow::new(Some(&adj), step, if is_float { 2 } else { 0 });
    let logic = Logic::new(ctx, opt, &row);
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        row,
        #[weak]
        logic,
        move || row.set_value(logic.current().trim().parse::<f64>().unwrap_or(0.0))
    );
    row.connect_value_notify(clone!(
        #[weak]
        logic,
        move |r| {
            let v = r.value();
            let text = if logic.opt.r#type == "float" {
                format_float((v * 100.0).round() / 100.0)
            } else {
                (v.round() as i64).to_string()
            };
            logic.commit(&text);
        }
    ));
    let sim = clone!(
        #[weak]
        row,
        move |t: &str| row.set_value(t.trim().parse().unwrap_or(0.0))
    );
    finish(logic, load, sim)
}

fn combo_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let row = adw::ComboRow::new();
    let logic = Logic::new(ctx, opt, &row);
    let model = gtk::StringList::new(&opt.choices.iter().map(String::as_str).collect::<Vec<_>>());
    row.set_model(Some(&model));
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        row,
        #[weak]
        logic,
        #[weak]
        model,
        move || {
            let cur = logic.current();
            let n = model.n_items();
            let found = (0..n).find(|&i| model.string(i).is_some_and(|s| s == cur));
            // a value from config.conf that is not a known choice
            let i = found.unwrap_or_else(|| {
                model.append(&cur);
                n
            });
            row.set_selected(i);
        }
    );
    row.connect_selected_notify(clone!(
        #[weak]
        logic,
        move |r| {
            if let Some(s) = r
                .selected_item()
                .and_downcast::<gtk::StringObject>()
                .map(|o| o.string())
            {
                logic.commit(&s);
            }
        }
    ));
    let sim = clone!(
        #[weak]
        row,
        #[weak]
        model,
        move |t: &str| {
            if let Some(i) = (0..model.n_items()).find(|&i| model.string(i).is_some_and(|s| s == t))
            {
                row.set_selected(i);
            }
        }
    );
    finish(logic, load, sim)
}

pub fn color_button(title: &str) -> gtk::ColorDialogButton {
    let dialog = gtk::ColorDialog::builder()
        .with_alpha(true)
        .title(title)
        .build();
    gtk::ColorDialogButton::builder()
        .dialog(&dialog)
        .valign(gtk::Align::Center)
        .css_classes(["color-swatch"])
        .build()
}

pub fn to_gdk(c: [f32; 4]) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

pub fn from_gdk(c: &gtk::gdk::RGBA) -> [f32; 4] {
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn color_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let row = adw::ActionRow::new();
    let logic = Logic::new(ctx, opt, &row);
    let btn = color_button(&opt.label);
    row.add_suffix(&btn);
    row.set_activatable_widget(Some(&btn));
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        btn,
        #[weak]
        logic,
        move || btn.set_rgba(&to_gdk(
            color_to_rgba(&logic.current()).unwrap_or([0.5, 0.5, 0.5, 1.0])
        ))
    );
    btn.connect_rgba_notify(clone!(
        #[weak]
        logic,
        move |b| {
            logic.commit(&rgba_to_hex(from_gdk(&b.rgba())));
        }
    ));
    let sim = clone!(
        #[weak]
        btn,
        move |t: &str| {
            if let Some(c) = color_to_rgba(t) {
                btn.set_rgba(&to_gdk(c));
            }
        }
    );
    finish(logic, load, sim)
}

fn entry_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let row = adw::EntryRow::builder().show_apply_button(true).build();
    let logic = Logic::new(ctx, opt, &row);
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        row,
        #[weak]
        logic,
        move || {
            let cur = logic.current();
            if row.text() != cur {
                row.set_text(&cur);
            }
        }
    );
    row.connect_apply(clone!(
        #[weak]
        logic,
        move |r| {
            let text = r.text().trim().to_string();
            if text.is_empty() {
                // an empty value cannot be written: remove the managed one instead
                let key = logic.opt.key.clone();
                logic.ctx.stage(|m| m.reset(&key));
            } else if !logic.commit(&text) {
                r.add_css_class("error");
            }
        }
    ));
    row.connect_changed(|r| r.remove_css_class("error"));
    let sim = clone!(
        #[weak]
        row,
        move |t: &str| {
            row.set_text(t);
            row.emit_by_name::<()>("apply", &[]);
        }
    );
    finish(logic, load, sim)
}

/// "Default Layout": the `tag_rule` for every tag, picked from Mango's layouts.
fn layout_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    let row = adw::ComboRow::new();
    let logic = Logic::new(ctx, opt, &row);
    row.set_subtitle("Layout every tag starts with (tag_rule id:*)");
    let mut names = vec!["Mango default"];
    names.extend(LAYOUTS.iter().copied());
    row.set_model(Some(&gtk::StringList::new(&names)));
    row.add_suffix(&logic.reset_rev);
    let load = clone!(
        #[weak]
        row,
        #[weak]
        logic,
        move || {
            let cur = logic.ctx.mgr.borrow().get(&logic.opt.key);
            let layout = cur
                .filter(|c| c.split(',').any(|p| p.trim() == "id:*"))
                .and_then(|c| layout_of(&c));
            let i = layout
                .and_then(|l| LAYOUTS.iter().position(|x| *x == l))
                .map_or(0, |i| i + 1);
            row.set_selected(i as u32);
        }
    );
    row.connect_selected_notify(clone!(
        #[weak]
        logic,
        move |r| {
            if logic.updating.get() {
                return;
            }
            let key = logic.opt.key.clone();
            match LAYOUTS.get((r.selected() as usize).wrapping_sub(1)) {
                Some(layout) => {
                    let managed = logic.ctx.mgr.borrow().staged().settings.get(&key).cloned();
                    logic.commit(&default_layout_value(managed.as_deref(), layout));
                }
                None => {
                    logic.ctx.stage(|m| m.reset(&key));
                }
            }
        }
    ));
    let sim = clone!(
        #[weak]
        row,
        move |t: &str| {
            if let Some(i) = LAYOUTS.iter().position(|l| *l == t) {
                row.set_selected(i as u32 + 1);
            }
        }
    );
    finish(logic, load, sim)
}

pub fn build_row(ctx: &Ctx, opt: &OptionItem) -> OptionRow {
    match opt.r#type.as_str() {
        "boolean" => switch_row(ctx, opt),
        "int" | "integer" | "float" => spin_row(ctx, opt),
        "enum" if !opt.choices.is_empty() => combo_row(ctx, opt),
        "color" => color_row(ctx, opt),
        "tag_rule" => layout_row(ctx, opt),
        _ => entry_row(ctx, opt),
    }
}
