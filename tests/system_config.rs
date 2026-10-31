//! Creating a missing config.conf. Own test binary because it sets
//! MANGOWM_MOD_SYSTEM_CONFIG (process-wide); everything runs in one test.

use mangowm_mod::core::manager::{ConfigManager, MINIMAL_CONFIG};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn apply_border(dir: &Path) -> mangowm_mod::core::manager::ApplyResult {
    let mut mgr = ConfigManager::new(
        Some(&dir.join("config.conf")),
        Some(&dir.join("mango-mod.conf")),
        Some(&dir.join("data")),
        true,
    )
    .unwrap();
    mgr.set("border_px", "3").unwrap();
    mgr.apply().unwrap()
}

#[test]
fn test_missing_config_conf_is_created() {
    // copied from the system default config
    let tmp = tempdir().unwrap();
    let system = tmp.path().join("system-config.conf");
    fs::write(&system, "# system default\nbind=SUPER,r,reload_config\n").unwrap();
    std::env::set_var("MANGOWM_MOD_SYSTEM_CONFIG", &system);
    let user = tmp.path().join("user");
    let res = apply_border(&user);
    assert!(res.ok, "{:?}", res.errors);
    assert_eq!(
        res.created_config.as_deref(),
        Some(system.display().to_string().as_str())
    );
    let text = fs::read_to_string(user.join("config.conf")).unwrap();
    assert!(text.starts_with("# system default\nbind=SUPER,r,reload_config\n"));
    assert!(text.trim_end().ends_with("source=./mango-mod.conf"));
    assert_eq!(
        fs::read_to_string(&system).unwrap(),
        "# system default\nbind=SUPER,r,reload_config\n"
    );

    // minimal built-in config when there is no system default
    std::env::set_var(
        "MANGOWM_MOD_SYSTEM_CONFIG",
        tmp.path().join("does-not-exist"),
    );
    let user2 = tmp.path().join("user2");
    let res = apply_border(&user2);
    assert!(res.ok, "{:?}", res.errors);
    assert_eq!(
        res.created_config.as_deref(),
        Some("a minimal built-in config")
    );
    let text = fs::read_to_string(user2.join("config.conf")).unwrap();
    assert!(text.starts_with(MINIMAL_CONFIG));
    assert!(text.trim_end().ends_with("source=./mango-mod.conf"));

    // an existing config.conf is kept (only the source line is added)
    let user3 = tmp.path().join("user3");
    fs::create_dir_all(&user3).unwrap();
    fs::write(user3.join("config.conf"), "border_px=1\n").unwrap();
    let res = apply_border(&user3);
    assert!(res.created_config.is_none());
    let text = fs::read_to_string(user3.join("config.conf")).unwrap();
    assert!(text.starts_with("border_px=1\n"));
    std::env::remove_var("MANGOWM_MOD_SYSTEM_CONFIG");
}
