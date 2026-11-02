//! Start and stop the bar (Mangobar or Waybar) that MangoWM Mod generated.
//!
//! The started bar's "PID binary-name" goes to a PID file, so `stop` only
//! ever signals the bar we started (never the user's own bar).

use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::generate::{bar_dir, CONFIG_FILE, STYLE_FILE};
use super::model::{Backend, BarConfig};
use crate::core::fsutil::{atomic_write, find_in_path};

/// `$XDG_RUNTIME_DIR/mangowm-mod/bar.pid`, else `<data_dir>/bar.pid`.
pub fn pid_file(data_dir: &Path) -> PathBuf {
    match dirs::runtime_dir() {
        Some(run) => run.join("mangowm-mod").join("bar.pid"),
        None => data_dir.join("bar.pid"),
    }
}

pub fn log_file(data_dir: &Path) -> PathBuf {
    data_dir.join("bar.log")
}

fn read_pid_file(path: &Path) -> Option<(u32, String)> {
    let text = fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let pid: u32 = parts.next()?.parse().ok()?;
    let bin = parts.next()?.to_string();
    (pid > 1).then_some((pid, bin))
}

fn have_proc() -> bool {
    Path::new("/proc/self").exists()
}

/// Running and not a zombie. `None` when this cannot be checked (no /proc).
fn is_alive(pid: u32) -> Option<bool> {
    if !have_proc() {
        return None;
    }
    let stat = match fs::read_to_string(format!("/proc/{}/stat", pid)) {
        Ok(s) => s,
        Err(_) => return Some(false),
    };
    // state is the first field after the ")" that closes the command name
    let state = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .unwrap_or("");
    Some(!matches!(state, "Z" | "X" | "x"))
}

/// True when `pid` is still the bar we started: its `comm` or one of its
/// arguments is named `binary`. Without /proc the PID file is trusted.
fn is_our_process(pid: u32, binary: &str) -> bool {
    if !have_proc() {
        return true;
    }
    if is_alive(pid) != Some(true) {
        return false;
    }
    let comm = fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
    if comm.trim() == binary {
        return true;
    }
    let cmdline = fs::read(format!("/proc/{}/cmdline", pid)).unwrap_or_default();
    cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .any(|a| {
            Path::new(&*String::from_utf8_lossy(a))
                .file_name()
                .is_some_and(|n| n == binary)
        })
}

/// Escape a literal for the extended regex `pkill -f` uses.
fn regex_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn wait_gone(pid: u32) -> bool {
    for _ in 0..30 {
        if is_alive(pid) != Some(true) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    is_alive(pid) != Some(true)
}

/// Send SIGTERM. Ok(false) when the process no longer exists.
#[cfg(unix)]
fn send_term(pid: u32) -> Result<bool> {
    let pid_t = libc::pid_t::try_from(pid).map_err(|_| anyhow!("invalid PID {}", pid))?;
    // SAFETY: kill(2) only takes plain integers.
    if unsafe { libc::kill(pid_t, libc::SIGTERM) } == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    bail!("could not stop PID {}: {}", pid, err)
}

#[cfg(not(unix))]
fn send_term(pid: u32) -> Result<bool> {
    bail!("stopping PID {} is only supported on Unix systems", pid)
}

const NO_PKILL: &str = "this system has no /proc and `pkill` was not found in PATH, so the bar \
                        could not be found; stop it manually (close it or end the process)";

/// Signal `pid` and wait for it to exit; returns the message for the user.
fn stop_pid(pid: u32, binary: &str) -> Result<Option<String>> {
    if !send_term(pid)? {
        return Ok(None);
    }
    if !wait_gone(pid) {
        return Ok(Some(format!(
            "sent the stop signal to {} (PID {}), but it is still running",
            binary, pid
        )));
    }
    Ok(Some(format!("stopped {} (PID {})", binary, pid)))
}

/// Stop the bar MangoWM Mod started. Returns a message about what was done,
/// or `None` when no bar of ours was running.
pub fn stop_bar(bar: &BarConfig, dir: Option<&Path>, data_dir: &Path) -> Result<Option<String>> {
    let pid_path = pid_file(data_dir);
    if let Some((pid, binary)) = read_pid_file(&pid_path) {
        let ours = is_our_process(pid, &binary);
        // Stale PID files (process ended or PID reused) are removed as well.
        let _ = fs::remove_file(&pid_path);
        if ours {
            if let Some(msg) = stop_pid(pid, &binary)? {
                return Ok(Some(msg));
            }
        }
    }
    stop_by_command_line(bar, dir)
}

/// The bars we may have started: (backend, config path).
fn our_configs(bar: &BarConfig, dir: Option<&Path>) -> Vec<(Backend, PathBuf)> {
    match dir {
        Some(d) => vec![(bar.backend, d.join(CONFIG_FILE))],
        None => [Backend::Mangobar, Backend::Waybar]
            .into_iter()
            .map(|b| (b, bar_dir(b).join(CONFIG_FILE)))
            .collect(),
    }
}

/// True when `argv` runs `<binary> -c <cfg>`. The binary may be argv[0] or,
/// for a script started through its interpreter, argv[1].
fn argv_matches(argv: &[&[u8]], binary: &str, cfg: &[u8]) -> bool {
    (0..2).any(|i| {
        argv.len() > i + 2
            && Path::new(&*String::from_utf8_lossy(argv[i]))
                .file_name()
                .is_some_and(|n| n == binary)
            && argv[i + 1] == b"-c"
            && argv[i + 2] == cfg
    })
}

/// Fallback without a usable PID file: find `<binary> -c <our config>`
/// in /proc (or with `pkill -f` where there is no /proc).
fn stop_by_command_line(bar: &BarConfig, dir: Option<&Path>) -> Result<Option<String>> {
    let targets = our_configs(bar, dir);
    if !have_proc() {
        return pkill_ours(&targets);
    }
    let mut msgs = Vec::new();
    for (pid, binary) in find_ours(&targets)? {
        if let Some(msg) = stop_pid(pid, binary)? {
            msgs.push(msg);
        }
    }
    Ok((!msgs.is_empty()).then(|| msgs.join("; ")))
}

/// Running `<binary> -c <cfg>` processes for the targets, from /proc.
fn find_ours(targets: &[(Backend, PathBuf)]) -> Result<Vec<(u32, &'static str)>> {
    let me = std::process::id();
    let mut found: Vec<(u32, &'static str)> = Vec::new();
    for entry in fs::read_dir("/proc")?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me || is_alive(pid) != Some(true) {
            continue;
        }
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let argv: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
        for (backend, cfg) in targets {
            let cfg = cfg.display().to_string();
            if argv_matches(&argv, backend.binary(), cfg.as_bytes()) {
                found.push((pid, backend.binary()));
            }
        }
    }
    Ok(found)
}

/// True when a bar MangoWM Mod started (PID file, or `-c <our config>`,
/// e.g. from mango-mod.conf at login) is running. False without /proc.
pub fn our_bar_running(data_dir: &Path) -> bool {
    if !have_proc() {
        return false;
    }
    if let Some((pid, binary)) = read_pid_file(&pid_file(data_dir)) {
        if is_our_process(pid, &binary) {
            return true;
        }
    }
    let targets = our_configs(&BarConfig::default(), None);
    find_ours(&targets).is_ok_and(|f| !f.is_empty())
}

/// `pkill -f "<binary> -c <our config>"` for systems without /proc.
fn pkill_ours(targets: &[(Backend, PathBuf)]) -> Result<Option<String>> {
    let Some(pkill) = find_in_path("pkill") else {
        bail!(NO_PKILL);
    };
    let mut stopped = Vec::new();
    for (backend, cfg) in targets {
        let pattern = format!(
            "{} -c {}",
            regex_escape(backend.binary()),
            regex_escape(&cfg.display().to_string())
        );
        let status = Command::new(&pkill)
            .arg("-f")
            .arg(&pattern)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("failed to run pkill")?;
        // pkill: 0 = matched, 1 = nothing matched
        if status.success() {
            stopped.push(backend.binary());
        }
    }
    Ok((!stopped.is_empty()).then(|| format!("stopped {}", stopped.join(", "))))
}

/// A Mangobar or Waybar that MangoWM Mod did not start (e.g. `exec_once=waybar`
/// in config.conf, or one started from a terminal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignBar {
    pub pid: u32,
    pub binary: &'static str,
    pub command: String,
}

/// Running bars that are not ours: their program is mangobar or waybar, they
/// do not run `-c <our config>` and are not in our PID file. Empty without /proc.
pub fn foreign_bars(bar: &BarConfig, data_dir: &Path) -> Vec<ForeignBar> {
    let mut out = Vec::new();
    if !have_proc() {
        return out;
    }
    let started = read_pid_file(&pid_file(data_dir)).map(|(pid, _)| pid);
    let ours: Vec<String> = our_configs(bar, None)
        .into_iter()
        .map(|(_, c)| c.display().to_string())
        .collect();
    let me = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me || Some(pid) == started || is_alive(pid) != Some(true) {
            continue;
        }
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        let binary = match comm.trim() {
            "waybar" => Backend::Waybar.binary(),
            "mangobar" => Backend::Mangobar.binary(),
            _ => continue,
        };
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let argv: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
        if ours
            .iter()
            .any(|cfg| argv_matches(&argv, binary, cfg.as_bytes()))
        {
            continue;
        }
        let command = argv
            .iter()
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        out.push(ForeignBar {
            pid,
            binary,
            command,
        });
    }
    out.sort_by_key(|b| b.pid);
    out
}

/// Stop bars found by [`foreign_bars`] (only after the user agreed).
/// A PID that is no longer that bar is skipped.
pub fn stop_foreign(bars: &[ForeignBar]) -> Result<Vec<String>> {
    let mut msgs = Vec::new();
    for b in bars {
        let comm = fs::read_to_string(format!("/proc/{}/comm", b.pid)).unwrap_or_default();
        if comm.trim() != b.binary {
            continue;
        }
        if let Some(msg) = stop_pid(b.pid, b.binary)? {
            msgs.push(msg);
        }
    }
    Ok(msgs)
}

/// The bar program: `$PATH` first, then the usual install places, since
/// Mango's login environment may not have ~/.local/bin in `$PATH`.
pub fn find_bar_binary(backend: Backend) -> Option<PathBuf> {
    let name = backend.binary();
    if let Some(p) = find_in_path(name) {
        return Some(p);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
    }
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/usr/bin"));
    dirs.into_iter()
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// Where to get the bar when its binary is missing (mangobar README, 2026-10).
fn not_installed(backend: Backend) -> String {
    match backend {
        Backend::Mangobar => "`mangobar` was not found in PATH. Mangobar is not in Fedora's or \
                              Debian's repositories (Arch: AUR mangobar-git); build it from \
                              source: https://github.com/mangowm/mangobar (meson + ninja, see its \
                              README), or switch the bar to Waybar"
            .to_string(),
        Backend::Waybar => "`waybar` was not found in PATH. Install waybar from your \
                            distribution's packages or build it from source, then try again"
            .to_string(),
    }
}

/// How long a bar that fails on startup (bad config, no Wayland, missing
/// font) is given to exit before Start reports success.
const STARTUP_CHECK: std::time::Duration = std::time::Duration::from_millis(400);

/// Last non-empty line of the bar log, for the error message.
fn last_log_line(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Start the bar with the files in `dir` (default [`bar_dir`]), stopping
/// our previous instance first. Returns the new PID.
pub fn start_bar(bar: &BarConfig, dir: Option<&Path>, data_dir: &Path) -> Result<u32> {
    let binary = bar.backend.binary();
    let Some(program) = find_bar_binary(bar.backend) else {
        bail!(not_installed(bar.backend));
    };
    let out_dir = dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| bar_dir(bar.backend));
    let cfg = out_dir.join(CONFIG_FILE);
    let css = out_dir.join(STYLE_FILE);
    if !cfg.is_file() || !css.is_file() {
        bail!(
            "bar files are missing in {} (run `mangowm-mod bar generate`)",
            out_dir.display()
        );
    }

    let had_pid_file = read_pid_file(&pid_file(data_dir)).is_some();
    if let Err(e) = stop_bar(bar, Some(&out_dir), data_dir) {
        // Without a PID file there is no earlier bar of ours we know of.
        if had_pid_file {
            return Err(e);
        }
    }

    fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    let log_path = log_file(data_dir);
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open {}", log_path.display()))?;
    let log_err = log.try_clone()?;

    let mut cmd = Command::new(&program);
    cmd.arg("-c")
        .arg(&cfg)
        .arg("-s")
        .arg(&css)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    #[cfg(unix)]
    {
        // own process group: Ctrl+C in this terminal does not reach the bar
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let log_len = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("failed to start {}: {}", binary, e))?;
    let pid = child.id();

    // mangobar exits right away on a config error or a missing font; say so
    // instead of reporting a bar that is already gone.
    let started = std::time::Instant::now();
    while started.elapsed() < STARTUP_CHECK {
        if let Some(status) = child.try_wait()? {
            let new_log = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0) > log_len;
            let reason = new_log.then(|| last_log_line(&log_path)).flatten();
            bail!(
                "{} exited right after starting ({}){}; log: {}",
                binary,
                status,
                reason.map(|r| format!(": {}", r)).unwrap_or_default(),
                log_path.display()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let pid_path = pid_file(data_dir);
    atomic_write(&pid_path, &format!("{} {}\n", pid, binary)).with_context(|| {
        format!(
            "{} started (PID {}) but the PID file could not be written",
            binary, pid
        )
    })?;
    Ok(pid)
}
