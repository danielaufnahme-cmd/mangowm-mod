//! Bar page: the module picker (left / center / right) and the settings of
//! each module in the bar.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib::{self, clone};

use mangowm_mod::bar::generate::{module_option_value, waybar_module_name};
use mangowm_mod::bar::model::{module_base, Backend, BarConfig, KNOWN_MODULES, SECTIONS};
use mangowm_mod::bar::options::{format_opt_value, module_opts, OptKind};

use super::BarPage;

/// Picker entries besides [`KNOWN_MODULES`]; the ones ending in a separator
/// take a name.
const EXTRA_CHOICES: &[&str] = &["clock#date", "clock#time", "battery#", "custom/"];

pub fn describe(name: &str) -> String {
    if let Some(c) = name.strip_prefix("custom/") {
        return format!("Custom module “{}”: runs a command", c);
    }
    match name {
        "clock#date" => return "Date".to_string(),
        "clock#time" => return "Time".to_string(),
        _ => {}
    }
    let base = module_base(name);
    let desc = KNOWN_MODULES
        .iter()
        .find(|(m, _)| *m == base)
        .map(|(_, d)| d.to_string())
        .unwrap_or_default();
    match name.split_once('#') {
        Some((_, dev)) if base == "battery" => format!("{} ({})", desc, dev),
        _ => desc,
    }
}

/// One section's group and its current rows.
pub struct Section {
    pub key: &'static str,
    pub group: adw::PreferencesGroup,
    rows: RefCell<Vec<gtk::Widget>>,
}

pub struct Modules {
    pub sections: Vec<Section>,
    pub settings: adw::PreferencesGroup,
    setting_rows: RefCell<Vec<adw::ExpanderRow>>,
    /// What the settings rows were built for (backend + module list).
    pub(super) built_for: RefCell<String>,
}

impl Modules {
    pub fn new(page: &adw::PreferencesPage) -> Self {
        let mut sections = Vec::new();
        for (key, title, desc) in [
            (
                SECTIONS[0],
                "Left Modules",
                "Order from left to right. Moving past the end of a section moves to the next one",
            ),
            (SECTIONS[1], "Center Modules", ""),
            (SECTIONS[2], "Right Modules", ""),
        ] {
            let group = adw::PreferencesGroup::builder()
                .title(title)
                .description(desc)
                .build();
            page.add(&group);
            sections.push(Section {
                key,
                group,
                rows: RefCell::new(Vec::new()),
            });
        }
        let settings = adw::PreferencesGroup::builder()
            .title("Module Settings")
            .description(
                "Empty a field to go back to the default. Click actions: a shell command, \
                 or for Mangobar @view, @toggle, @ipc:…",
            )
            .build();
        page.add(&settings);
        Self {
            sections,
            settings,
            setting_rows: RefCell::new(Vec::new()),
            built_for: RefCell::new(String::new()),
        }
    }
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build()
}

impl BarPage {
    /// Rebuild the module lists and, when the modules changed, their settings.
    pub(super) fn load_modules(self: &Rc<Self>, bar: &BarConfig) {
        let all: Vec<&String> = bar.modules.all().collect();
        let total = all.len();
        let mut n = 0;
        for sec in &self.modules.sections {
            for r in sec.rows.borrow_mut().drain(..) {
                sec.group.remove(&r);
            }
            let list = bar.modules.section(sec.key).cloned().unwrap_or_default();
            let mut rows: Vec<gtk::Widget> = Vec::new();
            for (i, name) in list.iter().enumerate() {
                rows.push(self.module_row(bar, sec.key, i, name, n, total).upcast());
                n += 1;
            }
            if list.is_empty() {
                let empty = adw::ActionRow::builder()
                    .title("No modules")
                    .css_classes(["empty-row"])
                    .build();
                rows.push(empty.upcast());
            }
            rows.push(self.add_row(bar, sec.key).upcast());
            for r in &rows {
                sec.group.add(r);
            }
            *sec.rows.borrow_mut() = rows;
        }

        let sig = format!(
            "{:?} {} {}",
            bar.backend,
            bar.waybar_mango_modules,
            all.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
        );
        if *self.modules.built_for.borrow() != sig {
            self.build_settings(bar);
            *self.modules.built_for.borrow_mut() = sig;
        }
    }

    fn module_row(
        self: &Rc<Self>,
        bar: &BarConfig,
        section: &'static str,
        index: usize,
        name: &str,
        position: usize,
        total: usize,
    ) -> adw::ActionRow {
        let mut subtitle = describe(name);
        if bar.backend == Backend::Waybar
            && waybar_module_name(name, bar.waybar_mango_modules).is_none()
        {
            subtitle.push_str(" · not shown by this Waybar");
        }
        let row = adw::ActionRow::builder()
            .title(name)
            .subtitle(subtitle)
            .use_markup(false)
            .build();
        let up = icon_button("go-up-symbolic", "Move left");
        let down = icon_button("go-down-symbolic", "Move right");
        let rm = icon_button("user-trash-symbolic", "Remove from the bar");
        up.set_sensitive(position > 0);
        down.set_sensitive(position + 1 < total);
        for (btn, op) in [(&up, 0u8), (&down, 1), (&rm, 2)] {
            btn.connect_clicked(clone!(
                #[weak(rename_to = st)]
                self,
                move |_| {
                    // after the click: the rows are rebuilt by the change
                    glib::idle_add_local_once(move || {
                        st.edit(|b| match op {
                            0 => b.modules.shift(section, index, true),
                            1 => b.modules.shift(section, index, false),
                            _ => b.modules.remove(section, index).map(|_| ()),
                        });
                    });
                }
            ));
            row.add_suffix(btn);
        }
        row
    }

    /// "Add module": a drop-down of the modules not in the bar yet, plus a
    /// name field for battery#<device> and custom/<name>.
    fn add_row(self: &Rc<Self>, bar: &BarConfig, section: &'static str) -> adw::ActionRow {
        let used: Vec<&String> = bar.modules.all().collect();
        let choices: Vec<&str> = KNOWN_MODULES
            .iter()
            .map(|(m, _)| *m)
            .chain(EXTRA_CHOICES.iter().copied())
            .filter(|m| !used.iter().any(|u| u == m))
            .collect();
        let row = adw::ActionRow::builder()
            .title("Add Module")
            .css_classes(["add-module-row"])
            .build();
        let drop = gtk::DropDown::from_strings(&choices);
        drop.set_valign(gtk::Align::Center);
        let name = gtk::Entry::builder()
            .placeholder_text("name")
            .width_chars(10)
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        let add = icon_button("list-add-symbolic", "Add to this section");
        let picked = move |d: &gtk::DropDown| {
            d.selected_item()
                .and_downcast::<gtk::StringObject>()
                .map(|o| o.string().to_string())
                .unwrap_or_default()
        };
        let needs_name = |c: &str| c.ends_with('#') || c.ends_with('/');
        name.set_visible(needs_name(&picked(&drop)));
        drop.connect_selected_notify(clone!(
            #[weak]
            name,
            move |d| name.set_visible(needs_name(&picked(d)))
        ));
        let go = clone!(
            #[weak(rename_to = st)]
            self,
            #[weak]
            drop,
            #[weak]
            name,
            move || {
                let choice = picked(&drop);
                if choice.is_empty() {
                    return;
                }
                let module = if needs_name(&choice) {
                    let n = name.text().trim().to_string();
                    if n.is_empty() {
                        st.ctx.toast("Type a name for the module first");
                        return;
                    }
                    format!("{}{}", choice, n)
                } else {
                    choice
                };
                glib::idle_add_local_once(move || {
                    st.edit(|b| b.modules.add(section, &module));
                });
            }
        );
        let go2 = go.clone();
        add.connect_clicked(move |_| go());
        name.connect_activate(move |_| go2());
        add.set_sensitive(!choices.is_empty());
        row.add_suffix(&drop);
        row.add_suffix(&name);
        row.add_suffix(&add);
        row
    }

    /// One expander per module with the options the selected program has.
    fn build_settings(self: &Rc<Self>, bar: &BarConfig) {
        let expanded: Vec<String> = self
            .modules
            .setting_rows
            .borrow()
            .iter()
            .filter(|r| r.is_expanded())
            .map(|r| r.title().to_string())
            .collect();
        for r in self.modules.setting_rows.borrow_mut().drain(..) {
            self.modules.settings.remove(&r);
        }
        let mut rows = Vec::new();
        for module in bar.modules.all() {
            if bar.backend == Backend::Waybar
                && waybar_module_name(module, bar.waybar_mango_modules).is_none()
            {
                continue;
            }
            let exp = adw::ExpanderRow::builder()
                .title(module.as_str())
                .subtitle(describe(module))
                .use_markup(false)
                .expanded(expanded.contains(module))
                .build();
            for opt in module_opts(module) {
                if !opt.supported(bar.backend) {
                    continue;
                }
                let key = format!("{}.{}", module, opt.key);
                let current = module_option_value(bar, module, opt.key);
                let tip = format!("{} → {}", key, opt.key_for(bar.backend).unwrap_or(opt.key));
                if opt.kind == OptKind::Bool {
                    let sw = adw::SwitchRow::builder()
                        .title(opt.label)
                        .tooltip_text(&tip)
                        .active(current.and_then(|v| v.as_bool()).unwrap_or(false))
                        .build();
                    sw.connect_active_notify(clone!(
                        #[weak(rename_to = st)]
                        self,
                        move |r| st.set(&key, &r.is_active().to_string())
                    ));
                    exp.add_row(&sw);
                } else {
                    let entry = adw::EntryRow::builder()
                        .title(opt.label)
                        .tooltip_text(&tip)
                        .show_apply_button(true)
                        .build();
                    entry.set_text(&current.map(|v| format_opt_value(&v)).unwrap_or_default());
                    entry.connect_apply(clone!(
                        #[weak(rename_to = st)]
                        self,
                        move |r| {
                            let text = r.text().to_string();
                            if text.trim().is_empty() {
                                st.unset(&key);
                            } else {
                                st.set(&key, &text);
                            }
                        }
                    ));
                    exp.add_row(&entry);
                }
            }
            self.modules.settings.add(&exp);
            rows.push(exp);
        }
        self.modules.settings.set_visible(!rows.is_empty());
        *self.modules.setting_rows.borrow_mut() = rows;
    }
}
