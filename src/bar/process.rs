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
        for (backend, cfg) in &targets {
            let cfg = cfg.display().to_string();
            if argv_matches(&argv, backend.binary(), cfg.as_bytes()) {
                found.push((pid, backend.binary()));
            }
        }
    }
    let mut msgs = Vec::new();
    for (pid, binary) in found {
        if let Some(msg) = stop_pid(pid, binary)? {
            msgs.push(msg);
        }
    }
    Ok((!msgs.is_empty()).then(|| msgs.join("; ")))
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

/// Start the bar with the files in `dir` (default [`bar_dir`]), stopping
/// our previous instance first. Returns the new PID.
pub fn start_bar(bar: &BarConfig, dir: Option<&Path>, data_dir: &Path) -> Result<u32> {
    let binary = bar.backend.binary();
    if find_in_path(binary).is_none() {
        bail!(
            "`{}` was not found in PATH. Install {} from your distribution's packages or build \
             it from source, then try again",
            binary,
            binary
        );
    }
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

    let mut cmd = Command::new(binary);
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
    let child = cmd
        .spawn()
        .map_err(|e| anyhow!("failed to start {}: {}", binary, e))?;
    let pid = child.id();

    let pid_path = pid_file(data_dir);
    atomic_write(&pid_path, &format!("{} {}\n", pid, binary)).with_context(|| {
        format!(
            "{} started (PID {}) but the PID file could not be written",
            binary, pid
        )
    })?;
    Ok(pid)
}
