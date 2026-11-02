//! State shared by every page: the config manager, paths and change listeners.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Instant;

use gtk::glib;
use mangowm_mod::core::manager::ConfigManager;

pub type Ctx = Rc<Context>;

/// `MANGOWM_MOD_PROFILE=1`: time the main-thread hot paths (stderr).
pub fn profiling() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MANGOWM_MOD_PROFILE").is_some())
}

/// Run `f`; with profiling on, print how long it took.
pub fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    if !profiling() {
        return f();
    }
    let t = Instant::now();
    let out = f();
    since(t, what);
    out
}

/// With profiling on, print the time from `start` to now.
pub fn since(start: Instant, what: &str) {
    if profiling() {
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("profile: {:>8.2} ms  {}", ms, what);
    }
}

pub struct Context {
    pub mgr: RefCell<ConfigManager>,
    pub data_dir: PathBuf,
    /// `--dry-run` or `MANGOWM_MOD_DRY_RUN`: files are written, Mango is not reloaded.
    pub dry_run: bool,
    pub toasts: adw::ToastOverlay,
    /// Called after anything was staged, applied, discarded or restored.
    listeners: RefCell<Vec<Rc<dyn Fn()>>>,
}

impl Context {
    pub fn new(mgr: ConfigManager, dry_run: bool, toasts: adw::ToastOverlay) -> Ctx {
        let dry_run = dry_run || std::env::var_os("MANGOWM_MOD_DRY_RUN").is_some();
        Rc::new(Self {
            data_dir: mgr.data_dir.clone(),
            mgr: RefCell::new(mgr),
            dry_run,
            toasts,
            listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn on_change(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Rc::new(f));
    }

    /// Tell every page to re-read the manager (no borrow is held while they run).
    pub fn changed(&self) {
        let listeners: Vec<Rc<dyn Fn()>> = self.listeners.borrow().clone();
        timed("change listeners", || {
            for f in listeners {
                f();
            }
        });
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(
            adw::Toast::builder()
                .title(glib::markup_escape_text(text))
                .timeout(4)
                .build(),
        );
    }

    /// Run a staging call on the manager; toast the error or notify the pages.
    pub fn stage<T>(&self, f: impl FnOnce(&mut ConfigManager) -> anyhow::Result<T>) -> Option<T> {
        let res = f(&mut self.mgr.borrow_mut());
        match res {
            Ok(v) => {
                self.changed();
                Some(v)
            }
            Err(e) => {
                self.toast(&format!("{:#}", e));
                self.changed();
                None
            }
        }
    }

    /// Number of staged changes, as `mangowm-mod status` counts them.
    pub fn pending(&self) -> usize {
        let mgr = self.mgr.borrow();
        mgr.pending_changes().len() + mgr.pending_item_changes().len()
    }
}
