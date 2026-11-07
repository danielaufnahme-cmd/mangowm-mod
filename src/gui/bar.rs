//! Mangobar / Waybar builder. Like `mangowm-mod bar set`, every change is
//! saved to `<data_dir>/bar.json` at once; Write Files / Start use it.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use adw::prelude::*;
use gtk::glib::{self, clone};

use mangowm_mod::bar::generate::{bar_dir, warnings, waybar_module_name, write_bar_files};
use mangowm_mod::bar::login::{
    disable_unmanaged_bars, login_state, set_login_start, unmanaged_bar_lines, LoginState,
};
use mangowm_mod::bar::model::{
    line_height, Backend, BarConfig, BAR_FIELDS, FONT_SIZE_RANGE, FONT_WEIGHTS, KNOWN_MODULES,
    LAYERS, POSITIONS, TEXT_INSET,
};
use mangowm_mod::bar::options::find_opt;
use mangowm_mod::bar::process::{
    foreign_bars, start_bar, stop_bar, stop_foreign, update_live, ForeignBar,
};

use crate::ctx::Ctx;
use crate::logic::{color_to_rgba, rgba_to_hex};
use crate::rows::{color_button, from_gdk, to_gdk};

#[path = "bar_buttons.rs"]
mod bar_buttons;
#[path = "bar_modules.rs"]
mod bar_modules;
#[path = "bar_presets.rs"]
mod bar_presets;

/// Quiet time after the last edit before the running bar is updated.
const LIVE_DELAY: Duration = Duration::from_millis(400);

type Getter = fn(&BarConfig) -> f64;
type ColorGetter = fn(&BarConfig) -> &str;

pub(crate) struct BarPage {
    ctx: Ctx,
    path: PathBuf,
    updating: Cell<bool>,
    page: adw::PreferencesPage,
    backend: adw::ComboRow,
    login: adw::SwitchRow,
    layer: adw::ComboRow,
    position: adw::ComboRow,
    output: adw::EntryRow,
    mango_modules: adw::SwitchRow,
    auto_height: adw::SwitchRow,
    tag_fill: adw::SwitchRow,
    spins: Vec<(&'static str, adw::SpinRow, Getter)>,
    font: adw::EntryRow,
    weight: adw::ComboRow,
    colors: Vec<(&'static str, gtk::ColorDialogButton, ColorGetter)>,
    radius: adw::SpinRow,
    radius_pill: adw::SwitchRow,
    module_radius: adw::SpinRow,
    module_pill: adw::SwitchRow,
    /// Rows hidden while the bar program is Mangobar.
    waybar_only: Vec<gtk::Widget>,
    migrate: adw::PreferencesGroup,
    migrate_row: adw::ActionRow,
    presets: bar_presets::Presets,
    modules: bar_modules::Modules,
    buttons: bar_buttons::Buttons,
    extra: adw::PreferencesGroup,
    extra_rows: RefCell<Vec<gtk::Widget>>,
    files: adw::ActionRow,
    notes: adw::PreferencesGroup,
    note_rows: RefCell<Vec<gtk::Widget>>,
    /// The settings (bar.json is saved [`LIVE_DELAY`] after a change).
    model: RefCell<BarConfig>,
    /// A change is not saved yet.
    dirty: Cell<bool>,
    /// bar.json's time when it was last read or written.
    disk_mtime: Cell<Option<SystemTime>>,
    /// Pending live update (see [`BarPage::schedule_live`]).
    live_timer: RefCell<Option<glib::SourceId>>,
    live_busy: Cell<bool>,
}

impl BarPage {
    /// The settings in memory; re-read when bar.json was changed by someone
    /// else (the Wallpaper page, the CLI) and nothing is waiting to be saved.
    fn bar(&self) -> BarConfig {
        self.sync_from_disk();
        self.model.borrow().clone()
    }

    fn sync_from_disk(&self) {
        if self.dirty.get() {
            return;
        }
        let mtime = file_mtime(&self.path);
        if self.disk_mtime.get() == mtime && self.disk_mtime.get().is_some() {
            return;
        }
        let bar = match BarConfig::load(&self.path) {
            Ok(b) => b,
            Err(e) => {
                self.ctx.toast(&format!("{:#}", e));
                BarConfig::default()
            }
        };
        *self.model.borrow_mut() = bar;
        self.disk_mtime.set(mtime);
    }

    /// Keep `bar` as the settings; bar.json is written with the live update.
    fn commit(self: &Rc<Self>, bar: BarConfig) {
        *self.model.borrow_mut() = bar;
        self.dirty.set(true);
        self.schedule_live();
    }

    /// Write bar.json now if a change is waiting.
    fn flush_save(&self) {
        if !self.dirty.get() {
            return;
        }
        match self.model.borrow().save(&self.path) {
            Ok(()) => {
                self.dirty.set(false);
                self.disk_mtime.set(file_mtime(&self.path));
            }
            Err(e) => self.ctx.toast(&format!("Bar: {:#}", e)),
        }
    }

    /// `mangowm-mod bar set KEY VALUE`. Only the rows that depend on the
    /// change are refreshed; module and program changes rebuild the lists.
    fn set(self: &Rc<Self>, key: &str, value: &str) {
        if self.updating.get() {
            return;
        }
        let mut bar = self.bar();
        if let Err(e) = bar.set(key, value) {
            self.ctx.toast(&format!("Bar: {:#}", e));
            // show the saved value again
            self.refresh();
            return;
        }
        self.commit(bar);
        match key {
            "backend" | "waybar-mango-modules" | "memory-display" | "left" | "center" | "right" => {
                self.load()
            }
            _ if !BAR_FIELDS.contains(&key) => {
                self.refresh();
                self.load_lists(&self.bar());
            }
            _ => self.refresh(),
        }
    }

    /// `mangowm-mod bar unset KEY`.
    fn unset(self: &Rc<Self>, key: &str) {
        let mut bar = self.bar();
        if bar.unset(key) {
            self.commit(bar);
        }
        // show the default again in the module settings
        self.modules.built_for.borrow_mut().clear();
        self.load();
    }

    /// Change the settings with `f` (validated), then reload the page.
    fn edit(self: &Rc<Self>, f: impl FnOnce(&mut BarConfig) -> anyhow::Result<()>) {
        let mut bar = self.bar();
        match f(&mut bar).and_then(|_| bar.validate()) {
            Ok(()) => self.commit(bar),
            Err(e) => self.ctx.toast(&format!("Bar: {:#}", e)),
        }
        self.load();
    }

    /// Update the running bar [`LIVE_DELAY`] after the last change: write
    /// the files, then reload Waybar or restart Mangobar if our bar runs.
    fn schedule_live(self: &Rc<Self>) {
        if let Some(id) = self.live_timer.take() {
            id.remove();
        }
        let st = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(LIVE_DELAY, move || {
            if let Some(st) = st.upgrade() {
                // fired: the id must not be removed any more
                st.live_timer.take();
                st.flush_save();
                st.run_live();
            }
        });
        *self.live_timer.borrow_mut() = Some(id);
    }

    fn run_live(self: &Rc<Self>) {
        if self.live_busy.get() {
            self.schedule_live();
            return;
        }
        self.live_busy.set(true);
        let bar = self.bar();
        let data_dir = self.ctx.data_dir.clone();
        // the offscreen screenshot mode never touches the real bar
        let restart = std::env::var("GDK_BACKEND").as_deref() != Ok("broadway");
        let st = self.clone();
        glib::spawn_future_local(async move {
            // a Mangobar restart waits for the new bar: off the UI thread
            let res = gtk::gio::spawn_blocking(move || update_live(&bar, &data_dir, restart)).await;
            st.live_busy.set(false);
            match res {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => st.ctx.toast(&format!("Bar: {:#}", e)),
                Err(_) => st.ctx.toast("Bar: the live update failed"),
            }
        });
    }

    /// Every part of the page from the settings.
    fn load(self: &Rc<Self>) {
        let bar = self.bar();
        self.load_rows(&bar);
        self.load_presets(&bar);
        self.load_login(&bar);
        self.load_modules(&bar);
        self.load_buttons(&bar);
        self.load_lists(&bar);
    }

    /// After a simple change: the rows, preset marks and notes.
    fn refresh(self: &Rc<Self>) {
        let bar = self.bar();
        self.load_rows(&bar);
        self.load_presets(&bar);
        self.load_notes(&bar);
    }

    /// The setting rows (values, dependent subtitles) without rebuilding.
    fn load_rows(&self, bar: &BarConfig) {
        let waybar = bar.backend == Backend::Waybar;
        self.updating.set(true);
        self.backend.set_selected(if waybar { 1 } else { 0 });
        self.login.set_active(bar.start_at_login);
        self.layer
            .set_selected(LAYERS.iter().position(|l| *l == bar.layer).unwrap_or(0) as u32);
        self.position.set_selected(
            POSITIONS
                .iter()
                .position(|p| *p == bar.position)
                .unwrap_or(0) as u32,
        );
        let output = bar.output.join(", ");
        if self.output.text() != output {
            self.output.set_text(&output);
        }
        self.mango_modules.set_active(bar.waybar_mango_modules);
        for (key, row, get) in &self.spins {
            row.set_value(get(bar));
            match *key {
                "height" => {
                    row.set_sensitive(!bar.auto_height);
                    row.set_subtitle(&format!(
                        "At least {} px, so the text fits",
                        bar.min_height()
                    ));
                }
                "module-margin" if !waybar => {
                    row.set_subtitle("Waybar only: Mangobar modules fill the bar's height")
                }
                "module-margin" => row.set_subtitle(""),
                _ => {}
            }
        }
        self.auto_height.set_active(bar.auto_height);
        self.auto_height.set_subtitle(&format!(
            "auto: {} px (text line {} px + 2 × {} px{})",
            bar.min_height(),
            line_height(bar.font_size),
            TEXT_INSET,
            if waybar {
                " + module margins and border"
            } else {
                ""
            }
        ));
        self.tag_fill.set_active(bar.tag_fill);
        if self.font.text() != bar.font {
            self.font.set_text(&bar.font);
        }
        self.weight.set_selected(
            FONT_WEIGHTS
                .iter()
                .position(|w| *w == bar.font_weight)
                .unwrap_or(0) as u32,
        );
        for (_, btn, get) in &self.colors {
            if let Some(rgba) = color_to_rgba(get(bar)) {
                btn.set_rgba(&to_gdk(rgba));
            }
        }
        for (spin, pill, r) in [
            (&self.radius, &self.radius_pill, bar.radius),
            (&self.module_radius, &self.module_pill, bar.module_radius),
        ] {
            pill.set_active(r < 0);
            spin.set_sensitive(r >= 0);
            if r >= 0 {
                spin.set_value(r as f64);
            }
        }
        for w in &self.waybar_only {
            w.set_visible(waybar);
        }
        self.updating.set(false);
        self.files
            .set_subtitle(&bar_dir(bar.backend).display().to_string());
    }

    /// Login switch subtitle and the "config.conf starts another bar" note.
    fn load_login(&self, bar: &BarConfig) {
        let state = login_state(&self.ctx.mgr.borrow());
        let text = match (bar.start_at_login, state) {
            (true, LoginState::Applied) => "Starts from mango-mod.conf when Mango starts",
            (true, LoginState::Staged) => "Staged: press Apply to write it to mango-mod.conf",
            (true, _) => "Added to mango-mod.conf when you press Start or Write Files",
            (false, LoginState::Removing) => "Staged removal: press Apply",
            (false, LoginState::Off) => "Not started at login",
            (false, _) => "Removed from mango-mod.conf when you press Start or Write Files",
        };
        self.login.set_subtitle(text);

        let config_path = self.ctx.mgr.borrow().config_path.clone();
        let text = std::fs::read_to_string(&config_path).unwrap_or_default();
        let lines = unmanaged_bar_lines(&text);
        self.migrate.set_visible(!lines.is_empty());
        if !lines.is_empty() {
            let list = lines
                .iter()
                .map(|(n, l)| format!("line {}: {}", n, l.trim()))
                .collect::<Vec<_>>()
                .join("\n");
            self.migrate_row.set_subtitle(&format!(
                "{}\nTurn it off so only this bar starts (config.conf is backed up first)",
                list
            ));
        }
    }

    /// Comment out the other bar's line in config.conf (after a backup).
    fn migrate(self: &Rc<Self>) {
        let res = {
            let mgr = self.ctx.mgr.borrow();
            disable_unmanaged_bars(&mgr.config_path, &mgr.backups)
        };
        match res {
            Ok(Some((id, n))) => {
                self.ctx.toast(&format!(
                    "Turned off {} line(s) in config.conf (backup {}); takes effect at the next login",
                    n, id
                ));
                if let Err(e) = self.ctx.mgr.borrow_mut().reload() {
                    self.ctx.toast(&format!("{:#}", e));
                }
                self.ctx.changed();
            }
            Ok(None) => self.ctx.toast("config.conf starts no other bar"),
            Err(e) => self.ctx.toast(&format!("{:#}", e)),
        }
        self.load();
    }

    /// Top-level options and module options the page has no row for, then
    /// the notes.
    fn load_lists(&self, bar: &BarConfig) {
        for r in self.extra_rows.borrow_mut().drain(..) {
            self.extra.remove(&r);
        }
        let mut pairs: Vec<(String, String)> = bar
            .options
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();
        for (module, opts) in &bar.module_options {
            for (k, v) in opts {
                if find_opt(module, k).is_none() {
                    pairs.push((format!("{}.{}", module, k), v.to_string()));
                }
            }
        }
        let mut rows = Vec::new();
        for (key, value) in pairs {
            let row = adw::ActionRow::builder()
                .title(&key)
                .subtitle(&value)
                .use_markup(false)
                .build();
            let rm = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Remove this override")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build();
            row.add_suffix(&rm);
            // handled by the page's "bar.unset" action
            rm.set_action_name(Some("bar.unset"));
            rm.set_action_target_value(Some(&key.to_variant()));
            self.extra.add(&row);
            rows.push(row.upcast());
        }
        *self.extra_rows.borrow_mut() = rows;
        self.load_notes(bar);
    }

    /// The backend warnings.
    fn load_notes(&self, bar: &BarConfig) {
        for r in self.note_rows.borrow_mut().drain(..) {
            self.notes.remove(&r);
        }
        let mut notes = Vec::new();
        for w in warnings(bar) {
            let row = adw::ActionRow::builder()
                .title(&w)
                .use_markup(false)
                .css_classes(["warning-row"])
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
            self.notes.add(&row);
            notes.push(row.upcast());
        }
        self.notes.set_visible(!notes.is_empty());
        *self.note_rows.borrow_mut() = notes;
    }

    /// Stage the login line to match the switch; returns a note for the toast.
    fn sync_login(&self, bar: &BarConfig) -> Option<&'static str> {
        let changed = self.ctx.stage(|m| set_login_start(m, bar.start_at_login))?;
        changed.then_some(if bar.start_at_login {
            "; start at login staged: press Apply"
        } else {
            "; login start removal staged: press Apply"
        })
    }

    fn write_files(self: &Rc<Self>) {
        self.flush_save();
        let bar = self.bar();
        match write_bar_files(&bar, None::<&std::path::Path>) {
            Ok(dir) => {
                let note = self.sync_login(&bar).unwrap_or("");
                self.ctx.toast(&format!(
                    "{} files written to {}{}",
                    bar.backend.binary(),
                    dir.display(),
                    note
                ))
            }
            Err(e) => self.ctx.toast(&format!("{:#}", e)),
        }
        self.load();
    }

    /// Start, after asking to stop a Waybar/Mangobar we did not start.
    fn ask_start(self: &Rc<Self>) {
        self.flush_save();
        let bar = self.bar();
        let data_dir = self.ctx.data_dir.clone();
        let st = self.clone();
        // scans /proc: off the UI thread
        glib::spawn_future_local(async move {
            let others = gtk::gio::spawn_blocking(move || foreign_bars(&bar, &data_dir))
                .await
                .unwrap_or_default();
            if others.is_empty() {
                st.start(Vec::new());
                return;
            }
            let list = others
                .iter()
                .map(|b| format!("{} (PID {})", b.binary, b.pid))
                .collect::<Vec<_>>()
                .join(", ");
            let dialog = adw::AlertDialog::builder()
                .heading("Stop the Other Bar?")
                .body(format!(
                    "{} is running but was not started by MangoWM Mod. Stop it, so only this \
                     bar is shown?",
                    list
                ))
                .default_response("stop")
                .close_response("cancel")
                .build();
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("both", "Keep Both");
            dialog.add_response("stop", "Stop and Start");
            dialog.set_response_appearance("stop", adw::ResponseAppearance::Destructive);
            let st2 = st.clone();
            dialog.connect_response(None, move |_, resp| match resp {
                "stop" => st2.start(others.clone()),
                "both" => st2.start(Vec::new()),
                _ => {}
            });
            dialog.present(Some(&st.page));
        });
    }

    /// Write the files and (re)start the bar in a worker thread.
    fn start(self: &Rc<Self>, stop_first: Vec<ForeignBar>) {
        self.flush_save();
        let bar = self.bar();
        let data_dir = self.ctx.data_dir.clone();
        let st = self.clone();
        glib::spawn_future_local(async move {
            let job = bar.clone();
            let res = gtk::gio::spawn_blocking(move || -> anyhow::Result<u32> {
                if !stop_first.is_empty() {
                    stop_foreign(&stop_first)?;
                }
                let dir = write_bar_files(&job, None::<&std::path::Path>)?;
                start_bar(&job, Some(&dir), &data_dir)
            })
            .await;
            match res {
                Ok(Ok(pid)) => {
                    let note = st.sync_login(&bar).unwrap_or("");
                    st.ctx.toast(&format!(
                        "{} started (PID {}){}",
                        bar.backend.binary(),
                        pid,
                        note
                    ))
                }
                Ok(Err(e)) => st.ctx.toast(&format!("{:#}", e)),
                Err(_) => st.ctx.toast("Bar: starting failed"),
            }
            st.load();
        });
    }

    fn stop(self: &Rc<Self>) {
        let bar = self.bar();
        let data_dir = self.ctx.data_dir.clone();
        let st = self.clone();
        // waits up to 3 s for the bar to exit: off the UI thread
        glib::spawn_future_local(async move {
            let res = gtk::gio::spawn_blocking(move || stop_bar(&bar, None, &data_dir)).await;
            match res {
                Ok(Ok(Some(msg))) => st.ctx.toast(&msg),
                Ok(Ok(None)) => st.ctx.toast("No bar started by mangowm-mod is running"),
                Ok(Err(e)) => st.ctx.toast(&format!("{:#}", e)),
                Err(_) => st.ctx.toast("Bar: stopping failed"),
            }
        });
    }
}

fn file_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn spin(title: &str, lo: f64, hi: f64) -> adw::SpinRow {
    adw::SpinRow::builder()
        .title(title)
        .adjustment(&gtk::Adjustment::new(lo, lo, hi, 1.0, 5.0, 0.0))
        .build()
}

fn color_row(title: &str) -> (adw::ActionRow, gtk::ColorDialogButton) {
    let row = adw::ActionRow::builder().title(title).build();
    let btn = color_button(title);
    row.add_suffix(&btn);
    row.set_activatable_widget(Some(&btn));
    (row, btn)
}

fn titled_group(title: &str, desc: &str) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder()
        .title(title)
        .description(desc)
        .build()
}

pub fn page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Bar")
        .description("Mangobar or Waybar config built by MangoWM Mod. Changes are saved at once")
        .build();
    let mut waybar_only: Vec<gtk::Widget> = Vec::new();

    // config.conf starts its own bar
    let migrate_row = adw::ActionRow::builder()
        .title("config.conf starts another bar")
        .use_markup(false)
        .css_classes(["warning-row"])
        .build();
    migrate_row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    let migrate_btn = gtk::Button::builder()
        .label("Turn Off")
        .tooltip_text("Comment out the line in config.conf (backed up first)")
        .valign(gtk::Align::Center)
        .css_classes(["pill"])
        .build();
    migrate_row.add_suffix(&migrate_btn);
    let migrate = adw::PreferencesGroup::new();
    migrate.add(&migrate_row);
    migrate.set_visible(false);
    page.add(&migrate);

    let presets = bar_presets::Presets::new(&page);

    // general
    let backend = adw::ComboRow::builder()
        .title("Bar program")
        .model(&gtk::StringList::new(&["Mangobar", "Waybar"]))
        .build();
    let login = adw::SwitchRow::builder()
        .title("Start bar at login")
        .build();
    let layer = adw::ComboRow::builder()
        .title("Layer")
        .model(&gtk::StringList::new(&["Top", "Bottom", "Overlay"]))
        .build();
    let position = adw::ComboRow::builder()
        .title("Position")
        .model(&gtk::StringList::new(&["Top", "Bottom"]))
        .build();
    let output = adw::EntryRow::builder()
        .title("Monitors (comma-separated, empty = all)")
        .show_apply_button(true)
        .build();
    let mango_modules = adw::SwitchRow::builder()
        .title("Use Waybar's mango/* modules")
        .subtitle(
            "Needs a Waybar newer than 0.15.0. Off: workspaces use ext/workspaces and the \
             window title reads mmsg",
        )
        .build();
    let general = adw::PreferencesGroup::new();
    for w in [
        backend.upcast_ref::<gtk::Widget>(),
        login.upcast_ref(),
        layer.upcast_ref(),
        position.upcast_ref(),
        output.upcast_ref(),
        mango_modules.upcast_ref(),
    ] {
        general.add(w);
    }
    waybar_only.push(position.clone().upcast());
    waybar_only.push(mango_modules.clone().upcast());
    page.add(&general);

    // size and spacing
    let size = titled_group(
        "Size and Spacing",
        "In pixels. Margins are the gap between the bar and the screen edges",
    );
    let auto_height = adw::SwitchRow::builder().title("Automatic height").build();
    size.add(&auto_height);
    let mut spins: Vec<(&'static str, adw::SpinRow, Getter)> = Vec::new();
    let size_rows: [(&'static str, &str, f64, f64, Getter, bool); 10] = [
        (
            "height",
            "Height",
            16.0,
            160.0,
            |b| b.effective_height() as f64,
            false,
        ),
        (
            "margin-top",
            "Top margin",
            0.0,
            100.0,
            |b| b.margin_top as f64,
            false,
        ),
        (
            "margin-bottom",
            "Bottom margin",
            0.0,
            100.0,
            |b| b.margin_bottom as f64,
            true,
        ),
        (
            "margin-left",
            "Left margin",
            0.0,
            1000.0,
            |b| b.margin_left as f64,
            false,
        ),
        (
            "margin-right",
            "Right margin",
            0.0,
            1000.0,
            |b| b.margin_right as f64,
            false,
        ),
        (
            "spacing",
            "Space between modules",
            0.0,
            50.0,
            |b| b.spacing as f64,
            true,
        ),
        (
            "module-padding",
            "Module padding",
            0.0,
            60.0,
            |b| b.module_padding as f64,
            false,
        ),
        (
            "module-margin",
            "Module top/bottom margin",
            0.0,
            30.0,
            |b| b.module_margin as f64,
            false,
        ),
        (
            "module-gap",
            "Module side margin",
            0.0,
            40.0,
            |b| b.module_gap as f64,
            false,
        ),
        (
            "title-length",
            "Window title max length (characters, 0 = no limit)",
            0.0,
            300.0,
            |b| b.title_length as f64,
            false,
        ),
    ];
    for (key, title, lo, hi, get, wb) in size_rows {
        let row = spin(title, lo, hi);
        size.add(&row);
        if wb {
            waybar_only.push(row.clone().upcast());
        }
        spins.push((key, row, get));
    }
    page.add(&size);

    // font
    let font = adw::EntryRow::builder()
        .title("Font families (Mangobar uses one: a Nerd Font in the list, else the first)")
        .show_apply_button(true)
        .build();
    let font_size = spin(
        "Font size",
        FONT_SIZE_RANGE.0 as f64,
        FONT_SIZE_RANGE.1 as f64,
    );
    let weight = adw::ComboRow::builder()
        .title("Font weight")
        .model(&gtk::StringList::new(&[
            "Normal", "Medium", "Semibold", "Bold",
        ]))
        .build();
    let fonts = titled_group("Font", "");
    fonts.add(&font);
    fonts.add(&font_size);
    fonts.add(&weight);
    spins.push(("font-size", font_size, |b| b.font_size as f64));
    page.add(&fonts);

    // look
    let look = titled_group("Look", "");
    let mut colors: Vec<(&'static str, gtk::ColorDialogButton, ColorGetter)> = Vec::new();
    let color_rows: [(&'static str, &str, &str, ColorGetter, bool); 8] = [
        ("bg", "Background", "", |b| &b.bg_color, false),
        ("fg", "Text", "", |b| &b.fg_color, false),
        ("accent", "Accent", "Active tag", |b| &b.accent_color, false),
        (
            "module-bg",
            "Module background",
            "",
            |b| &b.module_bg_color,
            false,
        ),
        (
            "hover",
            "Hover",
            "Mangobar: tags only",
            |b| &b.hover_color,
            false,
        ),
        ("urgent", "Urgent", "Urgent tag", |b| &b.urgent_color, false),
        (
            "inactive",
            "Inactive",
            "Empty and inactive tags",
            |b| &b.inactive_color,
            false,
        ),
        ("border-color", "Border", "", |b| &b.border_color, true),
    ];
    for (key, title, sub, get, wb) in color_rows {
        let (row, btn) = color_row(title);
        if !sub.is_empty() {
            row.set_subtitle(sub);
        }
        look.add(&row);
        if wb {
            waybar_only.push(row.clone().upcast());
        }
        colors.push((key, btn, get));
    }
    let border_width = spin("Border width", 0.0, 10.0);
    look.add(&border_width);
    waybar_only.push(border_width.clone().upcast());
    spins.push(("border-width", border_width, |b| b.border_width as f64));
    let radius = spin("Bar corner radius", 0.0, 100.0);
    let radius_pill = adw::SwitchRow::builder().title("Pill-shaped bar").build();
    let module_radius = spin("Module corner radius", 0.0, 100.0);
    let module_pill = adw::SwitchRow::builder()
        .title("Pill-shaped modules")
        .build();
    look.add(&radius);
    look.add(&radius_pill);
    look.add(&module_radius);
    look.add(&module_pill);
    let tag_fill = adw::SwitchRow::builder()
        .title("Filled active tag")
        .subtitle("Off: the active tag is accent text")
        .build();
    look.add(&tag_fill);
    page.add(&look);

    let modules = bar_modules::Modules::new(&page);
    let custom_buttons = bar_buttons::Buttons::new(&page);

    let extra = adw::PreferencesGroup::builder()
        .title("Extra Options")
        .description(
            "Advanced: a top-level key (Mangobar: sys-interval, scroll-interval, buffer-scale, \
             smooth-scrolling-threshold) or &lt;module&gt;.&lt;option&gt;, e.g. cpu.tooltip=false",
        )
        .build();
    let add_opt = adw::EntryRow::builder()
        .title("Add option (key=value)")
        .show_apply_button(true)
        .build();
    extra.add(&add_opt);
    let known = adw::ExpanderRow::builder()
        .title("Available modules")
        .subtitle("With their Waybar name")
        .build();
    for (name, desc) in KNOWN_MODULES {
        let wb = waybar_module_name(name, true).unwrap_or_else(|| "-".into());
        known.add_row(
            &adw::ActionRow::builder()
                .title(*name)
                .subtitle(format!("{} · Waybar: {}", desc, wb))
                .use_markup(false)
                .build(),
        );
    }
    extra.add(&known);
    page.add(&extra);

    let notes = adw::PreferencesGroup::new();
    page.add(&notes);

    let files = adw::ActionRow::builder()
        .title("Files")
        .subtitle_selectable(true)
        .use_markup(false)
        .build();
    let run = adw::PreferencesGroup::builder().title("Run").build();
    run.add(&files);
    let buttons = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::End)
        .margin_top(12)
        .build();
    let write_btn = gtk::Button::builder()
        .label("Write Files")
        .tooltip_text("Write config.jsonc and style.css")
        .css_classes(["pill"])
        .build();
    let stop_btn = gtk::Button::builder()
        .label("Stop")
        .tooltip_text("Stop the bar started by mangowm-mod")
        .css_classes(["pill"])
        .build();
    let start_btn = gtk::Button::builder()
        .label("Start")
        .tooltip_text("Write the files and start the bar")
        .css_classes(["pill", "suggested-action"])
        .build();
    buttons.append(&write_btn);
    buttons.append(&stop_btn);
    buttons.append(&start_btn);
    run.add(&buttons);
    page.add(&run);

    let st = Rc::new(BarPage {
        ctx: ctx.clone(),
        path: BarConfig::default_path(&ctx.data_dir),
        updating: Cell::new(false),
        page: page.clone(),
        backend,
        login,
        layer,
        position,
        output,
        mango_modules,
        auto_height,
        tag_fill,
        spins,
        font,
        weight,
        colors,
        radius,
        radius_pill,
        module_radius,
        module_pill,
        waybar_only,
        migrate,
        migrate_row,
        presets,
        modules,
        buttons: custom_buttons,
        extra,
        extra_rows: RefCell::new(Vec::new()),
        files,
        notes,
        note_rows: RefCell::new(Vec::new()),
        model: RefCell::new(BarConfig::default()),
        dirty: Cell::new(false),
        disk_mtime: Cell::new(None),
        live_timer: RefCell::new(None),
        live_busy: Cell::new(false),
    });

    st.backend.connect_selected_notify(clone!(
        #[weak]
        st,
        move |r| st.set(
            "backend",
            if r.selected() == 1 {
                "waybar"
            } else {
                "mangobar"
            }
        )
    ));
    st.login.connect_active_notify(clone!(
        #[weak]
        st,
        move |r| {
            if st.updating.get() {
                return;
            }
            st.set("start-at-login", &r.is_active().to_string());
            let on = r.is_active();
            if st.ctx.stage(|m| set_login_start(m, on)) == Some(true) {
                st.ctx.toast("Staged: press Apply to write mango-mod.conf");
            }
            st.load_login(&st.bar());
        }
    ));
    st.layer.connect_selected_notify(clone!(
        #[weak]
        st,
        move |r| st.set("layer", LAYERS[r.selected() as usize % LAYERS.len()])
    ));
    st.position.connect_selected_notify(clone!(
        #[weak]
        st,
        move |r| st.set(
            "position",
            POSITIONS[r.selected() as usize % POSITIONS.len()]
        )
    ));
    st.weight.connect_selected_notify(clone!(
        #[weak]
        st,
        move |r| st.set(
            "font-weight",
            FONT_WEIGHTS[r.selected() as usize % FONT_WEIGHTS.len()]
        )
    ));
    for (key, row, _) in &st.spins {
        let key = *key;
        row.connect_value_notify(clone!(
            #[weak]
            st,
            move |r| st.set(key, &(r.value().round() as i64).to_string())
        ));
    }
    for (spin, pill, key) in [
        (&st.radius, &st.radius_pill, "radius"),
        (&st.module_radius, &st.module_pill, "module-radius"),
    ] {
        spin.connect_value_notify(clone!(
            #[weak]
            st,
            move |r| st.set(key, &(r.value().round() as i64).to_string())
        ));
        pill.connect_active_notify(clone!(
            #[weak]
            st,
            #[weak]
            spin,
            move |r| {
                let v = if r.is_active() {
                    "-1".to_string()
                } else {
                    (spin.value().round() as i64).to_string()
                };
                st.set(key, &v)
            }
        ));
    }
    st.font.connect_apply(clone!(
        #[weak]
        st,
        move |r| st.set("font", &r.text())
    ));
    st.output.connect_apply(clone!(
        #[weak]
        st,
        move |r| st.set("output", &r.text())
    ));
    st.auto_height.connect_active_notify(clone!(
        #[weak]
        st,
        move |r| st.set("auto-height", &r.is_active().to_string())
    ));
    st.tag_fill.connect_active_notify(clone!(
        #[weak]
        st,
        move |r| st.set("tag-fill", &r.is_active().to_string())
    ));
    st.connect_presets();
    st.connect_buttons();
    st.mango_modules.connect_active_notify(clone!(
        #[weak]
        st,
        move |r| st.set("waybar-mango-modules", &r.is_active().to_string())
    ));
    for (key, btn, _) in &st.colors {
        let key = *key;
        btn.connect_rgba_notify(clone!(
            #[weak]
            st,
            move |b| st.set(key, &rgba_to_hex(from_gdk(&b.rgba())))
        ));
    }
    add_opt.connect_apply(clone!(
        #[weak]
        st,
        move |r| {
            let text = r.text().to_string();
            match text.split_once('=') {
                Some((k, v)) if !k.trim().is_empty() => {
                    st.set(k.trim(), v.trim());
                    r.set_text("");
                }
                _ => st.ctx.toast("Write the option as key=value"),
            }
        }
    ));
    migrate_btn.connect_clicked(clone!(
        #[weak]
        st,
        move |_| st.migrate()
    ));
    write_btn.connect_clicked(clone!(
        #[weak]
        st,
        move |_| st.write_files()
    ));
    start_btn.connect_clicked(clone!(
        #[weak]
        st,
        move |_| st.ask_start()
    ));
    stop_btn.connect_clicked(clone!(
        #[weak]
        st,
        move |_| st.stop()
    ));
    // a change still waiting for its save when the window goes
    page.connect_destroy(clone!(
        #[weak]
        st,
        move |_| st.flush_save()
    ));
    // Apply / Discard / restore change the login state
    ctx.on_change(clone!(
        #[weak]
        st,
        move || {
            let bar = st.bar();
            st.load_login(&bar);
        }
    ));

    // the remove buttons of the extra options; the action also keeps the controller alive
    let group = gtk::gio::SimpleActionGroup::new();
    let unset = gtk::gio::SimpleAction::new("unset", Some(gtk::glib::VariantTy::STRING));
    let keep = st.clone();
    unset.connect_activate(move |_, arg| {
        if let Some(key) = arg.and_then(|v| v.get::<String>()) {
            keep.unset(&key);
        }
    });
    group.add_action(&unset);
    st.add_preset_actions(&group);
    page.insert_action_group("bar", Some(&group));

    crate::ctx::timed("bar page load", || st.load());
    page
}
