//! Bar start/stop with a fake `mangobar` script. Own test binary because it
//! changes PATH and XDG_RUNTIME_DIR (process-wide); everything runs in one
//! test. No real bar is ever started.

use mangowm_mod::bar::generate::write_bar_files;
use mangowm_mod::bar::model::BarConfig;
use mangowm_mod::bar::process::{pid_file, start_bar, stop_bar};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

/// Running and not a zombie (the stopped child stays a zombie of this test).
fn alive(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{}/stat", pid)) {
        Ok(stat) => !stat
            .rsplit_once(')')
            .unwrap()
            .1
            .trim_start()
            .starts_with('Z'),
        Err(_) => false,
    }
}

fn wait_dead(pid: u32) -> bool {
    for _ in 0..50 {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

#[test]
fn test_start_stop_fake_bar() {
    let tmp = tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let run = tmp.path().join("run");
    let data = tmp.path().join("data");
    let out = tmp.path().join("out");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(&run).unwrap();
    std::env::set_var("XDG_RUNTIME_DIR", &run);
    std::env::set_var("MANGOBAR_MOD_DIR", tmp.path().join("mangobar-mod"));
    std::env::set_var("WAYBAR_MOD_DIR", tmp.path().join("waybar-mod"));

    let bar = BarConfig::default();
    write_bar_files(&bar, Some(&out)).unwrap();

    // missing binary: clear, distro-neutral error
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    std::env::set_var("PATH", &bin);
    let err = start_bar(&bar, Some(&out), &data).unwrap_err().to_string();
    assert!(
        err.contains("not found in PATH") && err.contains("build it from source"),
        "{}",
        err
    );

    // a fake bar: stays alive like a real one and exits on SIGTERM
    let fake = bin.join("mangobar");
    fs::write(
        &fake,
        "#!/bin/sh\ntrap 'kill $! 2>/dev/null; exit 0' TERM\nsleep 300 &\nwait\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin.clone()];
    paths.extend(std::env::split_paths(&orig_path));
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

    let pid_path = pid_file(&data);
    assert_eq!(pid_path, run.join("mangowm-mod/bar.pid"));
    let pid = start_bar(&bar, Some(&out), &data).unwrap();
    assert_eq!(
        fs::read_to_string(&pid_path).unwrap(),
        format!("{} mangobar\n", pid)
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(alive(pid));
    assert!(data.join("bar.log").is_file());

    // start again: the previous instance is stopped first
    let pid2 = start_bar(&bar, Some(&out), &data).unwrap();
    assert_ne!(pid, pid2);
    assert!(wait_dead(pid));
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(alive(pid2));

    let msg = stop_bar(&bar, Some(&out), &data).unwrap().unwrap();
    assert!(msg.contains(&pid2.to_string()), "{}", msg);
    assert!(wait_dead(pid2));
    assert!(!pid_path.exists());
    assert_eq!(stop_bar(&bar, Some(&out), &data).unwrap(), None);

    // no PID file: pkill fallback matches only "mangobar -c <our config>"
    let pid3 = start_bar(&bar, Some(&out), &data).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    fs::remove_file(&pid_path).unwrap();
    let other_out = tmp.path().join("other");
    write_bar_files(&bar, Some(&other_out)).unwrap();
    assert_eq!(
        stop_bar(&bar, Some(Path::new(&other_out)), &data).unwrap(),
        None
    );
    assert!(alive(pid3));
    assert!(stop_bar(&bar, Some(&out), &data).unwrap().is_some());
    assert!(wait_dead(pid3));

    // stale PID file pointing at an unrelated process is not signalled
    fs::write(&pid_path, format!("{} mangobar\n", std::process::id())).unwrap();
    assert_eq!(stop_bar(&bar, Some(&out), &data).unwrap(), None);
    assert!(!pid_path.exists());
}
