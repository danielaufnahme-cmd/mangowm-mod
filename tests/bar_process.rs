//! Bar start/stop with a fake `mangobar` script. Own test binary because it
//! changes PATH and XDG_RUNTIME_DIR (process-wide); everything runs in one
//! test. No real bar is ever started, and no kill/pkill/ps binaries are
//! needed (only /bin/sh and sleep for the fake bar).

use mangowm_mod::bar::generate::write_bar_files;
use mangowm_mod::bar::model::BarConfig;
use mangowm_mod::bar::process::{foreign_bars, pid_file, start_bar, stop_bar, stop_foreign};
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
        err.contains("not found in PATH")
            && err.contains("build it from source")
            && err.contains("https://github.com/mangowm/mangobar"),
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

    // a bar that dies on startup (like mangobar on a config error) is an
    // error with its last log line, not a "started" message
    let crash = bin.join("crashbar");
    fs::write(
        &crash,
        "#!/bin/sh\necho 'mangobar: config error: bad json' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&crash, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(&fake, bin.join("mangobar.keep")).unwrap();
    fs::rename(&crash, &fake).unwrap();
    let err = start_bar(&bar, Some(&out), &data).unwrap_err().to_string();
    assert!(
        err.contains("exited right after starting") && err.contains("config error: bad json"),
        "{}",
        err
    );
    assert!(!pid_path.exists());
    fs::rename(bin.join("mangobar.keep"), &fake).unwrap();
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

    // no PID file: the /proc fallback matches only "mangobar -c <our config>"
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

    // a bar started outside mangowm-mod (like exec_once=waybar) is found,
    // ours is not; only the fake is ever stopped (a real Waybar may be running)
    let pid4 = start_bar(&bar, Some(&out), &data).unwrap();
    let user_bar = bin.join("waybar");
    fs::copy(&fake, &user_bar).unwrap();
    let mut child = std::process::Command::new(&user_bar)
        .arg("-c")
        .arg(tmp.path().join("user-waybar.jsonc"))
        .spawn()
        .unwrap();
    let fake_pid = child.id();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let found = foreign_bars(&bar, &data);
    assert!(!found.iter().any(|b| b.pid == pid4), "{:?}", found);
    let mine: Vec<_> = found.into_iter().filter(|b| b.pid == fake_pid).collect();
    assert_eq!(mine.len(), 1, "fake waybar not found");
    assert_eq!(mine[0].binary, "waybar");
    assert!(mine[0].command.contains("user-waybar.jsonc"));
    let msgs = stop_foreign(&mine).unwrap();
    assert_eq!(msgs.len(), 1, "{:?}", msgs);
    child.wait().unwrap();
    assert!(stop_bar(&bar, Some(&out), &data).unwrap().is_some());
    assert!(wait_dead(pid4));
}
