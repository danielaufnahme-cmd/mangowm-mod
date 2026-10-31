//! Reload status detection. Own test binary because it changes PATH and
//! MANGO_INSTANCE_SIGNATURE (process-wide); everything runs in one test.
//! Only a fake `mmsg` script is ever run.

use mangowm_mod::core::ipc::{MangoIpc, ReloadStatus};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

#[test]
fn test_reload_status() {
    std::env::remove_var("MANGOWM_MOD_DRY_RUN");
    let tmp = tempdir().unwrap();
    let empty = tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();

    assert_eq!(MangoIpc::new(true).reload(), ReloadStatus::DryRun);

    std::env::set_var("PATH", &empty);
    assert_eq!(MangoIpc::new(false).reload(), ReloadStatus::MmsgMissing);
    assert!(!MangoIpc::new(false).reload().is_live());

    // a fake mmsg that answers like Mango does
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let mmsg = bin.join("mmsg");
    fs::write(&mmsg, "#!/bin/sh\necho '{\"success\": true}'\n").unwrap();
    fs::set_permissions(&mmsg, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &bin);

    std::env::remove_var("MANGO_INSTANCE_SIGNATURE");
    assert!(matches!(
        MangoIpc::new(false).reload(),
        ReloadStatus::NotRunning(_)
    ));

    std::env::set_var("MANGO_INSTANCE_SIGNATURE", tmp.path().join("fake.sock"));
    assert_eq!(MangoIpc::new(false).reload(), ReloadStatus::Reloaded);

    fs::write(
        &mmsg,
        "#!/bin/sh\necho 'cannot connect to socket' >&2\nexit 1\n",
    )
    .unwrap();
    assert!(matches!(
        MangoIpc::new(false).reload(),
        ReloadStatus::NotRunning(_)
    ));
    std::env::remove_var("MANGO_INSTANCE_SIGNATURE");
}
