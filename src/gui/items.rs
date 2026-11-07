//! Key binds, rules, monitor rules and autostart: lists of managed lines
//! with an add dialog each. Everything is staged like `mangowm-mod bind add`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use anyhow::Result;

use mangowm_mod::cli::RULE_ADD_KINDS;
use mangowm_mod::core::actions::get_actions;
use mangowm_mod::core::items::{rule_keys, Bind, Monitor, Rule, MONITOR_FORM_KEYS};
use mangowm_mod::core::manager::ConfigManager;

use crate::ctx::Ctx;
use crate::logic::{
    monitor_entries, monitor_summary, parse_optional, rule_number, scale_choice, scale_from_choice,
    scale_labels, SCALE_PRESETS, TRANSFORM_LABELS,
};

/// A group whose rows are rebuilt when the staged lines change.
struct ListGroup {
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<gtk::Widget>>,
    sig: RefCell<Option<String>>,
}

impl ListGroup {
    fn new(title: &str, description: &str) -> Rc<Self> {
        Rc::new(Self {
            group: adw::PreferencesGroup::builder()
                .title(title)
                .description(description)
                .build(),
            rows: RefCell::new(Vec::new()),
            sig: RefCell::new(None),
        })
    }

    /// Replace the rows unless `sig` (the lines shown) is unchanged.
    fn set(&self, sig: String, build: impl FnOnce() -> Vec<gtk::Widget>, empty: &str) {
        if self.sig.borrow().as_deref() == Some(sig.as_str()) {
            return;
        }
        *self.sig.borrow_mut() = Some(sig);
        for r in self.rows.borrow_mut().drain(..) {
            self.group.remove(&r);
        }
        let mut rows = build();
        if rows.is_empty() {
            rows.push(
                adw::ActionRow::builder()
                    .title(empty)
                    .css_classes(["empty-row"])
                    .build()
                    .upcast(),
            );
        }
        for r in &rows {
            self.group.add(r);
        }
        *self.rows.borrow_mut() = rows;
    }
}

fn add_button(label: &str) -> gtk::Button {
    gtk::Button::builder()
        .child(
            &adw::ButtonContent::builder()
                .icon_name("list-add-symbolic")
                .label(label)
                .build(),
        )
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build()
}

/// A managed line with a remove button; `pending` marks it as not applied yet.
fn item_row(
    title: &str,
    subtitle: &str,
    pending: bool,
    on_remove: impl Fn() + 'static,
) -> gtk::Widget {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .build();
    if pending {
        row.add_css_class("modified");
    }
    let rm = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Remove (staged until Apply)")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build();
    rm.connect_clicked(move |_| on_remove());
    row.add_suffix(&rm);
    row.upcast()
}

fn entry(title: &str, text: &str) -> adw::EntryRow {
    adw::EntryRow::builder().title(title).text(text).build()
}

/// A dialog with form groups and an Add button. `submit` stages the item and
/// returns the messages to toast; an error stays in the dialog.
fn form_dialog(
    ctx: &Ctx,
    parent: &impl IsA<gtk::Widget>,
    title: &str,
    groups: &[adw::PreferencesGroup],
    submit: impl Fn(&mut ConfigManager) -> Result<Vec<String>> + 'static,
) {
    form_dialog_with(ctx, parent, title, "Add", groups, submit);
}

/// `form_dialog` with its own button label; returns the dialog and the button
/// so the caller can retitle them.
fn form_dialog_with(
    ctx: &Ctx,
    parent: &impl IsA<gtk::Widget>,
    title: &str,
    action_label: &str,
    groups: &[adw::PreferencesGroup],
    submit: impl Fn(&mut ConfigManager) -> Result<Vec<String>> + 'static,
) -> (adw::Dialog, gtk::Button) {
    let page = adw::PreferencesPage::new();
    for g in groups {
        page.add(g);
    }
    let banner = adw::Banner::new("");
    let add = gtk::Button::builder()
        .label(action_label)
        .css_classes(["suggested-action"])
        .build();
    let header = adw::HeaderBar::new();
    header.pack_end(&add);
    let tv = adw::ToolbarView::builder().content(&page).build();
    tv.add_top_bar(&header);
    tv.add_top_bar(&banner);
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(500)
        .child(&tv)
        .build();
    let ctx = ctx.clone();
    let d = dialog.clone();
    add.connect_clicked(move |_| {
        let res = submit(&mut ctx.mgr.borrow_mut());
        match res {
            Ok(msgs) => {
                d.close();
                ctx.changed();
                for m in msgs {
                    ctx.toast(&m);
                }
            }
            Err(e) => {
                banner.set_title(&format!("{:#}", e));
                banner.set_revealed(true);
            }
        }
    });
    dialog.present(Some(parent));
    (dialog, add)
}

fn items_page(title: &str, description: &str) -> adw::PreferencesPage {
    adw::PreferencesPage::builder()
        .title(title)
        .description(description)
        .build()
}

// ------------------------------------------------------------------ binds

fn bind_title(b: &Bind) -> String {
    let mut parts: Vec<&str> = b.mods.iter().map(String::as_str).collect();
    parts.push(&b.key);
    parts.join(" + ")
}

fn bind_subtitle(b: &Bind) -> String {
    let mut s = if b.args.is_empty() {
        b.action.clone()
    } else {
        format!("{} {}", b.action, b.args)
    };
    if !b.flags.is_empty() {
        s.push_str(&format!("  ·  flags {}", b.flags));
    }
    if b.mode != "default" {
        s.push_str(&format!("  ·  mode {}", b.mode));
    }
    s
}

fn add_bind_dialog(ctx: &Ctx, parent: &impl IsA<gtk::Widget>) {
    let actions = get_actions();
    let mods = entry(
        "Modifiers (SUPER, CTRL, SHIFT, ALT, HYPER or NONE, joined with +)",
        "SUPER",
    );
    let key = entry("Key (XKB name, e.g. Return, q, F1)", "");
    let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
    let action = adw::ComboRow::builder()
        .title("Action")
        .model(&gtk::StringList::new(&ids))
        .enable_search(true)
        .build();
    let args = entry("Arguments", "");
    let describe = {
        let (action, args) = (action.clone(), args.clone());
        let actions = actions.clone();
        move || {
            if let Some(a) = actions.get(action.selected() as usize) {
                action.set_subtitle(&a.description);
                args.set_title(&match &a.arg_placeholder {
                    Some(p) => format!("Arguments ({})", p),
                    None => "Arguments (none)".to_string(),
                });
            }
        }
    };
    describe();
    action.connect_selected_notify(move |_| describe());
    let main = adw::PreferencesGroup::new();
    for w in [
        mods.upcast_ref::<gtk::Widget>(),
        key.upcast_ref(),
        action.upcast_ref(),
        args.upcast_ref(),
    ] {
        main.add(w);
    }
    let flags = entry("Flags: s, l, r, p, c (l = also when locked)", "");
    let mode = entry("Key mode", "default");
    let replace = adw::SwitchRow::builder()
        .title("Replace a managed bind with the same keys")
        .build();
    let extra = adw::PreferencesGroup::builder().title("Options").build();
    extra.add(&flags);
    extra.add(&mode);
    extra.add(&replace);

    form_dialog(ctx, parent, "Add Bind", &[main, extra], move |m| {
        let id = actions
            .get(action.selected() as usize)
            .map(|a| a.id.clone())
            .unwrap_or_default();
        let mut bind = Bind::new(&mods.text(), &key.text(), &id, &args.text());
        bind.flags = flags.text().trim().to_string();
        bind.mode = mode.text().trim().to_string();
        let line = bind.to_config_line();
        m.add_bind(bind, replace.is_active())?;
        Ok(vec![format!("Staged: {}", line)])
    });
}

pub fn binds_page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = items_page(
        "Key Binds",
        "Binds written to mango-mod.conf. Binds in your own config.conf stay as they are",
    );
    let list = ListGroup::new("Managed Binds", "");
    let add = add_button("Add Bind");
    list.group.set_header_suffix(Some(&add));
    page.add(&list.group);

    let own = adw::ActionRow::builder()
        .title("Binds in your own config")
        .use_markup(false)
        .build();
    let own_count = gtk::Label::builder().css_classes(["dim-label"]).build();
    own.add_suffix(&own_count);
    let own_group = adw::PreferencesGroup::new();
    own_group.add(&own);
    page.add(&own_group);

    let refresh = {
        let ctx = ctx.clone();
        move || {
            let mgr = ctx.mgr.borrow();
            let binds = mgr.staged().binds.clone();
            let applied = mgr.managed().binds.clone();
            own.set_subtitle(&mgr.config_path.display().to_string());
            own_count.set_label(&mgr.user_binds().len().to_string());
            drop(mgr);
            let sig = binds
                .iter()
                .map(|b| format!("{}|{}", b.to_config_line(), b.mode))
                .chain(applied.iter().map(|b| b.to_config_line()))
                .collect::<Vec<_>>()
                .join("\n");
            let ctx = ctx.clone();
            list.set(
                sig,
                move || {
                    binds
                        .iter()
                        .enumerate()
                        .map(|(i, b)| {
                            let ctx = ctx.clone();
                            item_row(
                                &bind_title(b),
                                &bind_subtitle(b),
                                !applied.contains(b),
                                move || {
                                    ctx.stage(|m| m.remove_bind(i + 1).map(|_| ()));
                                },
                            )
                        })
                        .collect()
                },
                "No managed binds",
            );
        }
    };
    refresh();
    ctx.on_change(refresh);
    let (c, p) = (ctx.clone(), page.clone());
    add.connect_clicked(move |_| add_bind_dialog(&c, &p));
    page
}

// ------------------------------------------------------------------ rules

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "window_rule" => "Window rule",
        "window_rule_once" => "Window rule (once)",
        "tag_rule" => "Tag rule",
        "layer_rule" => "Layer rule",
        "device_rule" => "Device rule",
        "monitor_rule" => "Monitor rule",
        _ => "Rule",
    }
}

fn add_rule_dialog(ctx: &Ctx, parent: &impl IsA<gtk::Widget>) {
    let labels: Vec<&str> = RULE_ADD_KINDS.iter().map(|k| kind_label(k)).collect();
    let kind = adw::ComboRow::builder()
        .title("Type")
        .model(&gtk::StringList::new(&labels))
        .build();
    let fields = entry("Fields, e.g. app_id:mpv,is_floating:1", "");
    let keys = adw::ActionRow::builder()
        .title("Valid keys")
        .subtitle_selectable(true)
        .use_markup(false)
        .build();
    let show_keys = {
        let (kind, keys) = (kind.clone(), keys.clone());
        move || {
            let k = RULE_ADD_KINDS[kind.selected() as usize % RULE_ADD_KINDS.len()];
            keys.set_subtitle(&rule_keys(k).unwrap_or_default().join(", "));
        }
    };
    show_keys();
    kind.connect_selected_notify(move |_| show_keys());
    let group = adw::PreferencesGroup::new();
    group.add(&kind);
    group.add(&fields);
    group.add(&keys);

    form_dialog(ctx, parent, "Add Rule", &[group], move |m| {
        let k = RULE_ADD_KINDS[kind.selected() as usize % RULE_ADD_KINDS.len()];
        let rule = Rule::parse(k, &fields.text())?;
        let line = rule.to_config_line();
        m.add_rule(rule)?;
        Ok(vec![format!("Staged: {}", line)])
    });
}

/// List of managed rules of `kinds`; removal numbers match the CLI's lists.
fn rule_list(ctx: &Ctx, list: Rc<ListGroup>, kinds: &'static [&'static str], empty: &'static str) {
    let refresh = {
        let ctx = ctx.clone();
        move || {
            let (rules, applied) = {
                let mgr = ctx.mgr.borrow();
                (mgr.staged().rules.clone(), mgr.managed().rules.clone())
            };
            let sig = rules
                .iter()
                .chain(applied.iter())
                .map(|r| r.to_config_line())
                .collect::<Vec<_>>()
                .join("\n");
            let ctx = ctx.clone();
            list.set(
                sig,
                move || {
                    (0..rules.len())
                        .filter_map(|i| {
                            let n = rule_number(&rules, kinds, i)?;
                            let r = &rules[i];
                            let ctx = ctx.clone();
                            Some(item_row(
                                &r.body(),
                                kind_label(&r.kind),
                                !applied.contains(r),
                                move || {
                                    ctx.stage(|m| m.remove_rule(kinds, n).map(|_| ()));
                                },
                            ))
                        })
                        .collect()
                },
                empty,
            );
        }
    };
    refresh();
    ctx.on_change(refresh);
}

pub fn rules_page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = items_page(
        "Rules",
        "Window, tag, layer and device rules written to mango-mod.conf",
    );
    let list = ListGroup::new("Managed Rules", "");
    let add = add_button("Add Rule");
    list.group.set_header_suffix(Some(&add));
    page.add(&list.group);
    rule_list(ctx, list, RULE_ADD_KINDS, "No managed rules");
    let (c, p) = (ctx.clone(), page.clone());
    add.connect_clicked(move |_| add_rule_dialog(&c, &p));
    page
}

// ------------------------------------------------------------------ monitors

/// The managed rule Mango uses for output `name` (the first one), if any.
fn managed_monitor_rule(m: &ConfigManager, name: &str) -> Option<Rule> {
    m.staged()
        .rules
        .iter()
        .find(|r| r.kind == "monitor_rule" && r.get("name") == Some(name))
        .cloned()
}

/// Add (`existing` = None) or edit the monitor rule of one output. The form is
/// prefilled from the managed rule, also when an existing output name is
/// typed into the Add form, and saving replaces that rule.
fn monitor_dialog(ctx: &Ctx, parent: &impl IsA<gtk::Widget>, existing: Option<Rule>) {
    let name = entry("Output name, e.g. eDP-1 or DP-4", "");
    let width = entry("Width", "");
    let height = entry("Height", "");
    let refresh = entry("Refresh rate (Hz)", "");
    let x = entry("Position X", "");
    let y = entry("Position Y", "");
    let labels = scale_labels();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let scale = adw::ComboRow::builder()
        .title("Scale")
        .model(&gtk::StringList::new(&label_refs))
        .build();
    let custom_scale = adw::SpinRow::builder()
        .title("Custom scale")
        .digits(2)
        .adjustment(&gtk::Adjustment::new(1.0, 0.25, 10.0, 0.05, 0.25, 0.0))
        .visible(false)
        .build();
    {
        let custom_scale = custom_scale.clone();
        scale.connect_selected_notify(move |c| {
            custom_scale.set_visible(c.selected() as usize == SCALE_PRESETS.len() + 1);
        });
    }
    let transform = adw::ComboRow::builder()
        .title("Rotation")
        .model(&gtk::StringList::new(TRANSFORM_LABELS))
        .build();
    let vrr = adw::ComboRow::builder()
        .title("Variable refresh rate")
        .model(&gtk::StringList::new(&["Not set", "On", "Off"]))
        .build();
    let disable = adw::SwitchRow::builder()
        .title("Disable this output")
        .build();
    let primary = adw::SwitchRow::builder().title("Primary output").build();

    let output = adw::PreferencesGroup::new();
    output.add(&name);
    let display = adw::PreferencesGroup::builder()
        .title("Display")
        .description("Leave a field empty to keep Mango's choice")
        .build();
    display.add(&scale);
    display.add(&custom_scale);
    for w in [&width, &height, &refresh] {
        display.add(w);
    }
    let layout = adw::PreferencesGroup::builder().title("Layout").build();
    layout.add(&x);
    layout.add(&y);
    layout.add(&transform);
    layout.add(&vrr);
    let flags = adw::PreferencesGroup::new();
    flags.add(&disable);
    flags.add(&primary);

    // Output the form was filled from; saving then replaces exactly its fields.
    let filled_from: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let fill = {
        let (width, height, refresh, x, y) = (
            width.clone(),
            height.clone(),
            refresh.clone(),
            x.clone(),
            y.clone(),
        );
        let (scale, custom_scale, transform, vrr) = (
            scale.clone(),
            custom_scale.clone(),
            transform.clone(),
            vrr.clone(),
        );
        let (disable, primary) = (disable.clone(), primary.clone());
        let filled_from = filled_from.clone();
        move |rule: &Rule| {
            let mon = Monitor::from_rule(rule);
            let text = |k: &str| rule.get(k).unwrap_or_default().to_string();
            width.set_text(&text("width"));
            height.set_text(&text("height"));
            refresh.set_text(&text("refresh"));
            x.set_text(&text("x"));
            y.set_text(&text("y"));
            let choice = scale_choice(mon.scale);
            if let Some(s) = mon.scale {
                custom_scale.set_value(s);
            }
            scale.set_selected(choice as u32);
            custom_scale.set_visible(choice == SCALE_PRESETS.len() + 1);
            transform.set_selected(mon.transform.map(|t| t as u32 + 1).unwrap_or(0));
            vrr.set_selected(match mon.vrr {
                Some(true) => 1,
                Some(false) => 2,
                None => 0,
            });
            disable.set_active(mon.disable);
            primary.set_active(mon.primary);
            *filled_from.borrow_mut() = Some(mon.name);
        }
    };

    let editing = existing.is_some();
    if let Some(rule) = &existing {
        name.set_text(rule.get("name").unwrap_or_default());
        name.set_editable(false);
        fill(rule);
    }
    let title = match &existing {
        Some(r) => format!("Edit {}", r.get("name").unwrap_or("Monitor")),
        None => "Add Monitor".to_string(),
    };

    let submit = {
        let name = name.clone();
        let filled_from = filled_from.clone();
        move |m: &mut ConfigManager| -> Result<Vec<String>> {
            let mon = Monitor {
                name: name.text().trim().to_string(),
                width: parse_optional("width", &width.text())?,
                height: parse_optional("height", &height.text())?,
                refresh: parse_optional("refresh", &refresh.text())?,
                x: parse_optional("x", &x.text())?,
                y: parse_optional("y", &y.text())?,
                scale: scale_from_choice(scale.selected() as usize, custom_scale.value()),
                transform: match transform.selected() {
                    0 => None,
                    t => Some((t - 1) as u8),
                },
                vrr: match vrr.selected() {
                    1 => Some(true),
                    2 => Some(false),
                    _ => None,
                },
                disable: disable.is_active(),
                primary: primary.is_active(),
            };
            // A form filled from the rule shows all of its fields, so an emptied
            // field is removed; otherwise fields left empty keep their value.
            let clear: &[&str] = if filled_from.borrow().as_deref() == Some(mon.name.as_str()) {
                MONITOR_FORM_KEYS
            } else {
                &[]
            };
            let up = m.upsert_monitor_rule(mon.to_rule()?, clear)?;
            let mut msgs = vec![match &up.replaced {
                Some(old) if *old == up.rule => format!("{} is unchanged", mon.name),
                Some(_) => format!("Updated {}: {}", mon.name, monitor_summary(&up.rule)),
                None => format!("Staged: {}", up.rule.to_config_line()),
            }];
            if up.removed_duplicates > 0 {
                msgs.push(format!(
                    "Removed {} ignored duplicate rule(s) for {}",
                    up.removed_duplicates, mon.name
                ));
            }
            for own in m.user_monitor_rules_for(&mon.name) {
                msgs.push(format!(
                    "Your config already has `{}`. Mango uses the first matching monitor rule, so \
                     remove that line for this one to take effect",
                    own
                ));
            }
            Ok(msgs)
        }
    };

    let (dialog, button) = form_dialog_with(
        ctx,
        parent,
        &title,
        if editing { "Save" } else { "Add" },
        &[output, display, layout, flags],
        submit,
    );

    // Add form: typing the name of a managed output switches to editing it.
    if !editing {
        let ctx = ctx.clone();
        let (dialog, button) = (dialog.downgrade(), button.downgrade());
        name.connect_changed(move |e| {
            let (Some(dialog), Some(button)) = (dialog.upgrade(), button.upgrade()) else {
                return;
            };
            let typed = e.text().trim().to_string();
            if filled_from.borrow().as_deref() == Some(typed.as_str()) {
                return;
            }
            let found = managed_monitor_rule(&ctx.mgr.borrow(), &typed);
            match found {
                Some(rule) => {
                    fill(&rule);
                    dialog.set_title(&format!("Edit {}", typed));
                    button.set_label("Save");
                }
                None => {
                    if filled_from.borrow_mut().take().is_some() {
                        dialog.set_title("Add Monitor");
                        button.set_label("Add");
                    }
                }
            }
        });
    }
}

/// Warning row with a one-click cleanup of ignored duplicate monitor rules.
fn duplicates_group(ctx: &Ctx) -> (adw::PreferencesGroup, adw::ActionRow) {
    let row = adw::ActionRow::builder()
        .title("Duplicate monitor rules")
        .use_markup(false)
        .build();
    row.add_prefix(
        &gtk::Image::builder()
            .icon_name("dialog-warning-symbolic")
            .css_classes(["warning"])
            .build(),
    );
    let clean = gtk::Button::builder()
        .label("Clean Up")
        .tooltip_text("Keep the first rule per monitor (the one Mango uses); staged until Apply")
        .valign(gtk::Align::Center)
        .css_classes(["pill", "small"])
        .build();
    let c = ctx.clone();
    clean.connect_clicked(move |_| {
        if let Some(n) = c.stage(|m| m.dedupe_monitor_rules()) {
            c.toast(&format!("Removed {} duplicate monitor rule(s)", n));
        }
    });
    row.add_suffix(&clean);
    row.set_activatable_widget(Some(&clean));
    let group = adw::PreferencesGroup::builder().visible(false).build();
    group.add(&row);
    (group, row)
}

pub fn monitors_page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = items_page(
        "Monitors",
        "Resolution, refresh rate, scale and position per output, written to mango-mod.conf",
    );
    let (dup_group, dup_row) = duplicates_group(ctx);
    page.add(&dup_group);
    let list = ListGroup::new(
        "Monitors",
        "One rule per output. Click a monitor to change it",
    );
    let add = add_button("Add Monitor");
    list.group.set_header_suffix(Some(&add));
    page.add(&list.group);

    let refresh = {
        let ctx = ctx.clone();
        let page = page.clone();
        move || {
            let (rules, applied) = {
                let mgr = ctx.mgr.borrow();
                (mgr.staged().rules.clone(), mgr.managed().rules.clone())
            };
            let entries = monitor_entries(&rules);
            let dups: usize = entries.iter().map(|e| e.1).sum();
            dup_group.set_visible(dups > 0);
            dup_row.set_subtitle(&format!(
                "{} rule(s) are ignored because Mango only uses the first rule per monitor",
                dups
            ));
            let sig = rules
                .iter()
                .filter(|r| r.kind == "monitor_rule")
                .chain(applied.iter().filter(|r| r.kind == "monitor_rule"))
                .map(|r| r.to_config_line())
                .collect::<Vec<_>>()
                .join("\n");
            let ctx = ctx.clone();
            let page = page.clone();
            list.set(
                sig,
                move || {
                    entries
                        .into_iter()
                        .map(|(rule, dup)| monitor_row(&ctx, &page, rule, dup, &applied))
                        .collect()
                },
                "No managed monitors. Mango picks each output's preferred mode",
            );
        }
    };
    refresh();
    ctx.on_change(refresh);
    let (c, p) = (ctx.clone(), page.clone());
    add.connect_clicked(move |_| monitor_dialog(&c, &p, None));
    page
}

/// One output: click to edit, trash to remove all of its managed rules.
fn monitor_row(
    ctx: &Ctx,
    page: &adw::PreferencesPage,
    rule: Rule,
    duplicates: usize,
    applied: &[Rule],
) -> gtk::Widget {
    let name = rule.get("name").unwrap_or_default().to_string();
    let row = adw::ActionRow::builder()
        .title(&name)
        .subtitle(monitor_summary(&rule))
        .use_markup(false)
        .activatable(true)
        .build();
    row.add_prefix(&gtk::Image::from_icon_name("video-display-symbolic"));
    if !applied.contains(&rule) {
        row.add_css_class("modified");
    }
    if duplicates > 0 {
        row.add_suffix(
            &gtk::Label::builder()
                .label(format!("+{} ignored", duplicates))
                .tooltip_text("Later rules for this monitor that Mango ignores")
                .css_classes(["dim-label", "caption"])
                .valign(gtk::Align::Center)
                .build(),
        );
    }
    let rm = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Remove (staged until Apply)")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build();
    {
        let (ctx, name) = (ctx.clone(), name.clone());
        rm.connect_clicked(move |_| {
            ctx.stage(|m| m.remove_monitor_rules(&name).map(|_| ()));
        });
    }
    row.add_suffix(&rm);
    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    let (ctx, page) = (ctx.clone(), page.clone());
    row.connect_activated(move |_| monitor_dialog(&ctx, &page, Some(rule.clone())));
    row.upcast()
}

// ------------------------------------------------------------------ autostart

fn add_autostart_dialog(ctx: &Ctx, parent: &impl IsA<gtk::Widget>) {
    let command = entry("Command", "");
    let every = adw::SwitchRow::builder()
        .title("Run on every config reload")
        .subtitle("exec instead of exec_once")
        .build();
    let group = adw::PreferencesGroup::new();
    group.add(&command);
    group.add(&every);
    form_dialog(ctx, parent, "Add Autostart Command", &[group], move |m| {
        let cmd = command.text().to_string();
        let every_reload = every.is_active();
        m.add_autostart(&cmd, every_reload)?;
        let kw = if every_reload { "exec" } else { "exec_once" };
        Ok(vec![format!("Staged: {}={}", kw, cmd.trim())])
    });
}

pub fn autostart_page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = items_page(
        "Autostart",
        "Programs MangoWM starts (exec_once) or runs on every reload (exec)",
    );
    let list = ListGroup::new("Managed Commands", "");
    let add = add_button("Add Command");
    list.group.set_header_suffix(Some(&add));
    page.add(&list.group);
    let refresh = {
        let ctx = ctx.clone();
        move || {
            let (all, applied) = {
                let mgr = ctx.mgr.borrow();
                let (st, ap) = (mgr.staged(), mgr.managed());
                let tag = |once: &[String], every: &[String]| {
                    once.iter()
                        .map(|c| (false, c.clone()))
                        .chain(every.iter().map(|c| (true, c.clone())))
                        .collect::<Vec<_>>()
                };
                (tag(&st.exec_once, &st.exec), tag(&ap.exec_once, &ap.exec))
            };
            let sig = format!("{:?}{:?}", all, applied);
            let ctx = ctx.clone();
            list.set(
                sig,
                move || {
                    all.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let ctx = ctx.clone();
                            let when = if item.0 {
                                "Every reload (exec)"
                            } else {
                                "Once at start (exec_once)"
                            };
                            item_row(&item.1, when, !applied.contains(item), move || {
                                ctx.stage(|m| m.remove_autostart(i + 1).map(|_| ()));
                            })
                        })
                        .collect()
                },
                "No managed autostart commands",
            );
        }
    };
    refresh();
    ctx.on_change(refresh);
    let (c, p) = (ctx.clone(), page.clone());
    add.connect_clicked(move |_| add_autostart_dialog(&c, &p));
    page
}
