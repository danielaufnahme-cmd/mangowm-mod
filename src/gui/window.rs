//! Main window: sidebar, page stack, search and the Apply bar.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::gio;
use gtk::glib::{self, clone};

use mangowm_mod::core::ipc::ReloadStatus;
use mangowm_mod::core::manager::{ApplyResult, ConfigManager};
use mangowm_mod::core::schema::{schema_pages, search_options, OptionItem};

use crate::apply_bar::ApplyBar;
use crate::ctx::{timed, Context, Ctx};
use crate::rows::{build_row, OptionRow};
use crate::{backups, bar, items, presets, wallpaper, GuiArgs, APP_ID};

/// Sidebar icons of the schema pages (page id, icon).
const PAGE_ICONS: &[(&str, &str)] = &[
    ("look", "preferences-desktop-appearance-symbolic"),
    ("effects", "display-brightness-symbolic"),
    ("animations", "media-playback-start-symbolic"),
    ("layouts", "view-grid-symbolic"),
    ("bars", "view-paged-symbolic"),
    ("overview", "view-app-grid-symbolic"),
    ("scratchpad", "window-new-symbolic"),
    ("input", "input-keyboard-symbolic"),
    ("behavior", "preferences-system-symbolic"),
    ("display", "video-display-symbolic"),
];

/// Pages that are not generated from the schema: (id, title, icon).
const FIXED_PAGES: &[(&str, &str, &str)] = &[
    (
        "binds",
        "Key Binds",
        "preferences-desktop-keyboard-shortcuts-symbolic",
    ),
    ("rules", "Rules", "view-list-symbolic"),
    ("monitors", "Monitors", "video-joined-displays-symbolic"),
    ("autostart", "Autostart", "system-run-symbolic"),
    ("presets", "Presets", "starred-symbolic"),
    (
        "wallpaper",
        "Wallpaper",
        "preferences-desktop-wallpaper-symbolic",
    ),
    ("bar", "Bar", "view-continuous-symbolic"),
    ("backups", "Backups", "document-open-recent-symbolic"),
];

/// Max rows the search page shows.
const SEARCH_LIMIT: usize = 80;
/// Search rows built before the results show; the rest follow in batches this size.
const SEARCH_FIRST: usize = 16;

fn icon_or_fallback(name: &str) -> &str {
    let has = gtk::gdk::Display::default()
        .map(|d| gtk::IconTheme::for_display(&d).has_icon(name))
        .unwrap_or(false);
    if has {
        name
    } else {
        "preferences-other-symbolic"
    }
}

fn status_content(title: &str, description: &str) -> adw::ToolbarView {
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(
        &adw::StatusPage::builder()
            .title(title)
            .description(glib::markup_escape_text(description).as_str())
            .icon_name("dialog-error-symbolic")
            .build(),
    ));
    tv
}

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
    pub state: Option<Rc<WindowState>>,
}

impl MainWindow {
    pub fn new(app: &adw::Application, args: &GuiArgs) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("MangoWM Mod")
            .icon_name(APP_ID)
            .default_width(1100)
            .default_height(760)
            .build();
        window.set_size_request(360, 480);

        let mgr = ConfigManager::new(
            args.config.as_deref(),
            args.managed.as_deref(),
            args.data_dir.as_deref(),
            args.dry_run,
        );
        let state = match mgr {
            Ok(mgr) => Some(WindowState::new(&window, mgr, args.dry_run)),
            Err(e) => {
                window.set_content(Some(&status_content(
                    "MangoWM Mod Couldn't Start",
                    &format!("{:#}", e),
                )));
                None
            }
        };
        Self { window, state }
    }
}

pub struct WindowState {
    pub ctx: Ctx,
    window: adw::ApplicationWindow,
    stack: gtk::Stack,
    list: gtk::ListBox,
    ids: Vec<String>,
    titles: Vec<(String, String)>,
    pub search: gtk::SearchEntry,
    search_group: RefCell<Option<adw::PreferencesGroup>>,
    search_page: adw::PreferencesPage,
    search_rows: RefCell<Vec<OptionRow>>,
    /// Every search row built so far, by key (reused while typing).
    search_cache: RefCell<HashMap<String, OptionRow>>,
    /// Bumped by every search; stale batches of rows stop.
    search_gen: std::cell::Cell<u64>,
    me: RefCell<Weak<Self>>,
    /// Rows of the schema pages built so far.
    rows: RefCell<Vec<OptionRow>>,
    /// Pages already in the stack.
    built: RefCell<HashSet<String>>,
    pub apply_bar: ApplyBar,
    title: adw::WindowTitle,
    split: adw::NavigationSplitView,
    current: RefCell<String>,
    silent: std::cell::Cell<bool>,
}

impl WindowState {
    fn new(window: &adw::ApplicationWindow, mgr: ConfigManager, dry_run: bool) -> Rc<Self> {
        let toasts = adw::ToastOverlay::new();
        let ctx = Context::new(mgr, dry_run, toasts.clone());
        let pages = schema_pages();

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(120)
            .hexpand(true)
            .vexpand(true)
            .build();

        // pages are built on their first visit (open_page), not here
        let mut ids = Vec::new();
        let mut titles = Vec::new();
        for p in pages {
            ids.push(p.id.clone());
            titles.push((p.id.clone(), p.title.clone()));
        }
        for (id, title, _) in FIXED_PAGES {
            ids.push(id.to_string());
            titles.push((id.to_string(), title.to_string()));
        }

        // search results
        let search_page = adw::PreferencesPage::new();
        let search_empty = adw::StatusPage::builder()
            .icon_name("edit-find-symbolic")
            .title("No Matching Settings")
            .description("Search looks at setting names and their Mango keys")
            .build();
        stack.add_named(&search_page, Some("search"));
        stack.add_named(&search_empty, Some("search-empty"));

        let apply_bar = ApplyBar::new();
        let overlay = gtk::Overlay::builder().child(&stack).build();
        overlay.add_overlay(&apply_bar.widget);

        let title = adw::WindowTitle::new("", "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        let content_tv = adw::ToolbarView::builder().content(&overlay).build();
        content_tv.add_top_bar(&header);
        if ctx.dry_run {
            content_tv.add_top_bar(
                &adw::Banner::builder()
                    .title("Dry run: files are written, but MangoWM is not asked to reload")
                    .revealed(true)
                    .build(),
            );
        }
        let content_page = adw::NavigationPage::builder()
            .title("Settings")
            .tag("content")
            .child(&content_tv)
            .build();

        // sidebar
        let menu = gio::Menu::new();
        menu.append(Some("Files & Status"), Some("win.status"));
        menu.append(Some("About MangoWM Mod"), Some("win.about"));
        let side_header = adw::HeaderBar::builder()
            .title_widget(&adw::WindowTitle::new("MangoWM Mod", ""))
            .build();
        side_header.pack_end(
            &gtk::MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&menu)
                .tooltip_text("Main Menu")
                .primary(true)
                .build(),
        );
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search settings")
            .hexpand(true)
            .build();
        let search_box = gtk::Box::builder()
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(6)
            .build();
        search_box.append(&search);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        let icon_of = |id: &str| -> &'static str {
            PAGE_ICONS
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, ic)| *ic)
                .or_else(|| {
                    FIXED_PAGES
                        .iter()
                        .find(|(i, _, _)| *i == id)
                        .map(|(_, _, ic)| *ic)
                })
                .unwrap_or("preferences-other-symbolic")
        };
        for (id, t) in &titles {
            let bx = gtk::Box::builder().spacing(12).build();
            bx.append(&gtk::Image::from_icon_name(icon_or_fallback(icon_of(id))));
            bx.append(
                &gtk::Label::builder()
                    .label(t)
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
            );
            list.append(
                &gtk::ListBoxRow::builder()
                    .child(&bx)
                    .css_classes(["sidebar-row"])
                    .build(),
            );
        }
        let first_fixed = pages.len() as i32;
        list.set_header_func(move |row, _| {
            if row.index() == first_fixed {
                row.set_header(Some(
                    &gtk::Separator::builder()
                        .css_classes(["sidebar-separator"])
                        .build(),
                ));
            } else {
                row.set_header(gtk::Widget::NONE);
            }
        });
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        let side_tv = adw::ToolbarView::new();
        side_tv.add_top_bar(&side_header);
        side_tv.add_top_bar(&search_box);
        side_tv.set_content(Some(&scroller));
        let sidebar_page = adw::NavigationPage::builder()
            .title("MangoWM Mod")
            .tag("sidebar")
            .child(&side_tv)
            .build();

        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar_page)
            .content(&content_page)
            .min_sidebar_width(230.0)
            .max_sidebar_width(290.0)
            .sidebar_width_fraction(0.26)
            .build();
        toasts.set_child(Some(&split));
        window.set_content(Some(&toasts));
        let bp = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 700sp").expect("breakpoint"),
        );
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);

        let first = ids.first().cloned().unwrap_or_else(|| "binds".into());
        let state = Rc::new(Self {
            ctx: ctx.clone(),
            window: window.clone(),
            stack,
            list,
            ids,
            titles,
            search,
            search_group: RefCell::new(None),
            search_page,
            search_rows: RefCell::new(Vec::new()),
            search_cache: RefCell::new(HashMap::new()),
            search_gen: std::cell::Cell::new(0),
            me: RefCell::new(Weak::new()),
            rows: RefCell::new(Vec::new()),
            built: RefCell::new(HashSet::new()),
            apply_bar,
            title,
            split,
            current: RefCell::new(first.clone()),
            silent: std::cell::Cell::new(false),
        });
        state.connect(Rc::downgrade(&state));
        state.open_page(&first);
        state.sync();
        state
    }

    fn connect(&self, weak: Weak<Self>) {
        *self.me.borrow_mut() = weak.clone();
        let w = weak.clone();
        self.ctx.on_change(move || {
            if let Some(s) = w.upgrade() {
                s.sync();
            }
        });
        let w = weak.clone();
        self.list.connect_row_selected(move |_, row| {
            let (Some(s), Some(row)) = (w.upgrade(), row) else {
                return;
            };
            if s.silent.get() {
                return;
            }
            if let Some(id) = s.ids.get(row.index() as usize).cloned() {
                s.silent.set(true);
                s.search.set_text("");
                s.silent.set(false);
                s.open_page(&id);
                s.split.set_show_content(true);
            }
        });
        let w = weak.clone();
        self.search.connect_search_changed(move |e| {
            if let Some(s) = w.upgrade() {
                if !s.silent.get() {
                    s.on_search(&e.text());
                }
            }
        });
        self.search.connect_stop_search(|e| e.set_text(""));
        let w = weak.clone();
        self.apply_bar.apply.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.apply();
            }
        });
        let w = weak.clone();
        self.apply_bar.discard.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.discard();
            }
        });

        let status = gio::SimpleAction::new("status", None);
        let w = weak.clone();
        status.connect_activate(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.show_status();
            }
        });
        self.window.add_action(&status);
        let about = gio::SimpleAction::new("about", None);
        about.connect_activate(clone!(
            #[weak(rename_to = window)]
            self.window,
            move |_, _| {
                adw::AboutDialog::builder()
                    .application_name("MangoWM Mod")
                    .application_icon(APP_ID)
                    .developer_name("Daniel")
                    .version(env!("CARGO_PKG_VERSION"))
                    .comments("MangoWM settings manager and Mangobar/Waybar bar builder")
                    .website("https://github.com/danielaufnahme-cmd/mangowm-mod")
                    .license_type(gtk::License::MitX11)
                    .build()
                    .present(Some(&window));
            }
        ));
        self.window.add_action(&about);
    }

    /// Re-read every row and update the Apply bar.
    fn sync(&self) {
        for r in self.rows.borrow().iter() {
            r.refresh();
        }
        for r in self.search_rows.borrow().iter() {
            r.refresh();
        }
        let n = self.ctx.pending();
        let dirty = self.ctx.mgr.borrow().is_dirty();
        self.apply_bar.set_count(if dirty { n.max(1) } else { 0 });
    }

    pub fn open_page(&self, id: &str) {
        timed(&format!("open_page {}", id), || self.open_page_now(id))
    }

    /// Build page `id` and add it to the stack, once.
    fn ensure_page(&self, id: &str) {
        if !self.built.borrow_mut().insert(id.to_string()) {
            return;
        }
        let ctx = &self.ctx;
        let widget: gtk::Widget = match id {
            "binds" => items::binds_page(ctx).upcast(),
            "rules" => items::rules_page(ctx).upcast(),
            "monitors" => items::monitors_page(ctx).upcast(),
            "autostart" => items::autostart_page(ctx).upcast(),
            "presets" => presets::page(ctx),
            "wallpaper" => wallpaper::page(ctx),
            "bar" => bar::page(ctx).upcast(),
            "backups" => backups::page(ctx).upcast(),
            _ => {
                let Some(p) = schema_pages().iter().find(|p| p.id == id) else {
                    return;
                };
                let page = adw::PreferencesPage::builder()
                    .title(&p.title)
                    .description(glib::markup_escape_text(&p.description).as_str())
                    .build();
                let group = adw::PreferencesGroup::new();
                let mut rows = self.rows.borrow_mut();
                for opt in &p.options {
                    let r = build_row(ctx, opt);
                    group.add(&r.row);
                    rows.push(r);
                }
                page.add(&group);
                page.upcast()
            }
        };
        self.stack.add_named(&widget, Some(id));
    }

    /// The row of setting `key`, building its page if needed (screenshot mode).
    pub fn row(&self, key: &str) -> Option<OptionRow> {
        let page = schema_pages()
            .iter()
            .find(|p| p.options.iter().any(|o| o.key == key))?;
        self.ensure_page(&page.id);
        self.rows.borrow().iter().find(|r| r.key == key).cloned()
    }

    fn open_page_now(&self, id: &str) {
        self.ensure_page(id);
        *self.current.borrow_mut() = id.to_string();
        self.stack.set_visible_child_name(id);
        let title = self
            .titles
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, t)| t.as_str())
            .unwrap_or("");
        self.title.set_title(title);
        self.silent.set(true);
        if let Some(i) = self.ids.iter().position(|i| i == id) {
            if let Some(row) = self.list.row_at_index(i as i32) {
                self.list.select_row(Some(&row));
            }
        }
        self.silent.set(false);
    }

    pub fn on_search(&self, text: &str) {
        timed(&format!("search {:?}", text), || self.search_now(text))
    }

    fn search_now(&self, text: &str) {
        let q = text.trim();
        if q.is_empty() {
            let cur = self.current.borrow().clone();
            self.open_page(&cur);
            return;
        }
        let gen = self.search_gen.get() + 1;
        self.search_gen.set(gen);
        if let Some(old) = self.search_group.borrow_mut().take() {
            // the rows are kept for the next search: take them out of the old group
            for r in self.search_rows.borrow_mut().drain(..) {
                old.remove(&r.row);
            }
            self.search_page.remove(&old);
        }
        let found = search_options(q);
        let group = adw::PreferencesGroup::builder()
            .title(glib::markup_escape_text(&format!("{} results", found.len())).as_str())
            .build();
        let shown: Vec<OptionItem> = found.iter().take(SEARCH_LIMIT).cloned().collect();
        let (now, later) = shown.split_at(SEARCH_FIRST.min(shown.len()));
        for opt in now {
            self.add_search_row(&group, opt);
        }
        if !later.is_empty() {
            // the rest when idle, so typing never waits for a long result list
            let (w, group, later) = (self.me.borrow().clone(), group.clone(), later.to_vec());
            let mut done = 0;
            glib::idle_add_local(move || {
                let Some(s) = w.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if s.search_gen.get() != gen {
                    return glib::ControlFlow::Break;
                }
                for opt in later.iter().skip(done).take(SEARCH_FIRST) {
                    s.add_search_row(&group, opt);
                }
                done += SEARCH_FIRST;
                if done < later.len() {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
        }
        if found.len() > SEARCH_LIMIT {
            group.set_description(Some(&format!(
                "Showing the first {}; type more to narrow it down",
                SEARCH_LIMIT
            )));
        }
        self.search_page.add(&group);
        *self.search_group.borrow_mut() = Some(group);
        self.title.set_title("Search");
        self.silent.set(true);
        self.list.unselect_all();
        self.silent.set(false);
        self.stack.set_visible_child_name(if found.is_empty() {
            "search-empty"
        } else {
            "search"
        });
        self.split.set_show_content(true);
    }

    /// Add the search row of `opt`, reusing the one built by an earlier search.
    fn add_search_row(&self, group: &adw::PreferencesGroup, opt: &OptionItem) {
        let cached = self.search_cache.borrow().get(&opt.key).cloned();
        let r = match cached {
            Some(r) => {
                r.refresh();
                r
            }
            None => {
                let r = build_row(&self.ctx, opt);
                if let Some(ar) = r.row.downcast_ref::<adw::ActionRow>() {
                    let page = schema_pages()
                        .iter()
                        .find(|p| p.options.iter().any(|o| o.key == opt.key))
                        .map(|p| p.title.as_str())
                        .unwrap_or_default();
                    ar.set_subtitle(&format!("{} · {}", page, opt.key));
                }
                self.search_cache
                    .borrow_mut()
                    .insert(opt.key.clone(), r.clone());
                r
            }
        };
        group.add(&r.row);
        self.search_rows.borrow_mut().push(r);
    }

    pub fn apply(&self) {
        let res = self.ctx.mgr.borrow_mut().apply();
        match res {
            Ok(r) => report_apply(&self.ctx, &self.window, &r, None),
            Err(e) => self.ctx.toast(&format!("Apply failed: {:#}", e)),
        }
        self.ctx.changed();
    }

    fn discard(&self) {
        let res = self.ctx.mgr.borrow_mut().discard();
        if let Err(e) = res {
            self.ctx.toast(&format!("{:#}", e));
        } else {
            self.ctx.toast("Staged changes discarded");
        }
        self.ctx.changed();
    }

    /// What `mangowm-mod status` prints.
    fn show_status(&self) {
        let body = {
            let mgr = self.ctx.mgr.borrow();
            let line = |name: &str, p: &Path| {
                let missing = if p.exists() { "" } else { " (missing)" };
                format!("{}: {}{}", name, p.display(), missing)
            };
            let mut lines = vec![
                line("Config", &mgr.config_path),
                line("Managed", &mgr.managed_path),
                format!("Data dir: {}", mgr.data_dir.display()),
            ];
            let changes = mgr.pending_changes();
            let items = mgr.pending_item_changes();
            if changes.is_empty() && items.is_empty() {
                lines.push("Staged: none".into());
            } else {
                lines.push(format!("Staged changes ({}):", changes.len() + items.len()));
                for (k, v) in changes {
                    lines.push(match v {
                        Some(val) => format!("  {} = {}", k, val),
                        None => format!("  {} (reset)", k),
                    });
                }
                lines.extend(items.into_iter().map(|l| format!("  {}", l)));
            }
            lines.push(format!("Backups: {}", mgr.backups.list().len()));
            if self.ctx.dry_run {
                lines.push("Dry run: MangoWM is not asked to reload".into());
            }
            lines.join("\n")
        };
        let dialog = adw::AlertDialog::builder()
            .heading("Files & Status")
            .body(&body)
            .build();
        dialog.add_response("close", "Close");
        dialog.present(Some(&self.window));
    }
}

/// Toast an apply result, or show a dialog when it had errors.
pub fn report_apply(
    ctx: &Ctx,
    parent: &impl IsA<gtk::Widget>,
    res: &ApplyResult,
    what: Option<&str>,
) {
    if !res.errors.is_empty() {
        let mut body = res.errors.join("\n\n");
        if let Some(id) = &res.backup_id {
            body.push_str(&format!("\n\nBackup created: {}", id));
        }
        let dialog = adw::AlertDialog::builder()
            .heading("Apply Finished With Errors")
            .body(&body)
            .build();
        dialog.add_response("close", "Close");
        dialog.present(Some(parent));
        return;
    }
    let mut parts = vec![what.unwrap_or("Changes applied").to_string()];
    match &res.reload {
        Some(ReloadStatus::Reloaded) => parts.push("MangoWM reloaded".into()),
        Some(ReloadStatus::DryRun) => parts.push("dry run, not reloaded".into()),
        _ => {}
    }
    if let Some(origin) = &res.created_config {
        parts.push(format!("created config.conf from {}", origin));
    }
    ctx.toast(&parts.join(" · "));
    for w in &res.warnings {
        ctx.toast(w);
    }
}
