//! Key binds, rules, monitor rules and autostart: lists of managed lines
//! with an add dialog each. Everything is staged like `mangowm-mod bind add`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use anyhow::Result;

use mangowm_mod::cli::RULE_ADD_KINDS;
use mangowm_mod::core::actions::get_actions;
use mangowm_mod::core::items::{rule_keys, Bind, Monitor, Rule};
use mangowm_mod::core::manager::ConfigManager;

use crate::ctx::Ctx;
use crate::logic::{parse_optional, rule_number};

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
    let page = adw::PreferencesPage::new();
    for g in groups {
        page.add(g);
    }
    let banner = adw::Banner::new("");
    let add = gtk::Button::builder()
        .label("Add")
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

fn add_monitor_dialog(ctx: &Ctx, parent: &impl IsA<gtk::Widget>) {
    let name = entry("Output name, e.g. eDP-1", "");
    let width = entry("Width", "");
    let height = entry("Height", "");
    let refresh = entry("Refresh rate (Hz)", "");
    let x = entry("Position X", "");
    let y = entry("Position Y", "");
    let scale = entry("Scale", "");
    let transform = entry("Transform 0-7 (rotation / flip)", "");
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
    let mode = adw::PreferencesGroup::builder()
        .title("Mode")
        .description("Leave a field empty to keep Mango's choice")
        .build();
    for w in [&width, &height, &refresh, &x, &y, &scale, &transform] {
        mode.add(w);
    }
    mode.add(&vrr);
    let flags = adw::PreferencesGroup::new();
    flags.add(&disable);
    flags.add(&primary);

    form_dialog(
        ctx,
        parent,
        "Add Monitor Rule",
        &[output, mode, flags],
        move |m| {
            let mon = Monitor {
                name: name.text().to_string(),
                width: parse_optional("width", &width.text())?,
                height: parse_optional("height", &height.text())?,
                refresh: parse_optional("refresh", &refresh.text())?,
                x: parse_optional("x", &x.text())?,
                y: parse_optional("y", &y.text())?,
                scale: parse_optional("scale", &scale.text())?,
                transform: parse_optional("transform", &transform.text())?,
                vrr: match vrr.selected() {
                    1 => Some(true),
                    2 => Some(false),
                    _ => None,
                },
                disable: disable.is_active(),
                primary: primary.is_active(),
            };
            let rule = mon.to_rule()?;
            let line = rule.to_config_line();
            let earlier = m
                .staged()
                .rules
                .iter()
                .any(|r| r.kind == "monitor_rule" && r.get("name") == rule.get("name"));
            m.add_rule(rule)?;
            let mut msgs = vec![format!("Staged: {}", line)];
            if earlier {
                msgs.push(format!(
                    "A managed monitor rule for {} already exists and Mango uses the first match; \
                 remove the old one",
                    mon.name.trim()
                ));
            }
            for own in m.user_monitor_rules_for(mon.name.trim()) {
                msgs.push(format!(
                    "Your config already has `{}`. Mango uses the first matching monitor rule, so \
                 remove that line for this one to take effect",
                    own
                ));
            }
            Ok(msgs)
        },
    );
}

pub fn monitors_page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = items_page(
        "Monitors",
        "Monitor rules (resolution, position, scale, ...) written to mango-mod.conf",
    );
    let list = ListGroup::new("Managed Monitor Rules", "");
    let add = add_button("Add Monitor");
    list.group.set_header_suffix(Some(&add));
    page.add(&list.group);
    rule_list(ctx, list, &["monitor_rule"], "No managed monitor rules");
    let (c, p) = (ctx.clone(), page.clone());
    add.connect_clicked(move |_| add_monitor_dialog(&c, &p));
    page
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
