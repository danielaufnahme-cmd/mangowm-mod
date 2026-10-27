use mangowm_mod::bar::{generate_config, generate_style, Mangobar};
use mangowm_mod::core::manager::ConfigManager;
use mangowm_mod::items::{Bind, Monitor, Rule};
use mangowm_mod::presets::list_presets;
use tempfile::tempdir;

#[test]
fn test_mangowm_lifecycle() {
    let tmp = tempdir().unwrap();
    let config_file = tmp.path().join("config.conf");
    let managed_file = tmp.path().join("mango-mod.conf");
    let data_dir = tmp.path().join("data");

    std::fs::write(
        &config_file,
        r#"
border_width=2
corner_radius=8
gaps_in=4
"#,
    )
    .unwrap();

    let mut mgr =
        ConfigManager::new(Some(&config_file), Some(&managed_file), Some(&data_dir), true).unwrap();

    assert_eq!(mgr.get("border_width"), Some("2".to_string()));

    mgr.set("border_width", "3").unwrap();
    mgr.set("focused_border_color", "#89b4fa").unwrap();
    assert!(mgr.is_dirty());

    let res = mgr.apply().unwrap();
    assert!(res.ok);
    assert!(res.backup_id.is_some());
    assert!(!mgr.is_dirty());

    let managed_text = std::fs::read_to_string(&managed_file).unwrap();
    assert!(managed_text.contains("border_width=3"));
    assert!(managed_text.contains("focused_border_color=#89b4fa"));

    let main_text = std::fs::read_to_string(&config_file).unwrap();
    assert!(main_text.contains("source=./mango-mod.conf"));
}

#[test]
fn test_mangobar_generation() {
    let bar = Mangobar::default();
    let cfg = generate_config(&bar);
    assert!(cfg.contains("\"position\": \"top\""));
    assert!(cfg.contains("\"tag\""));
    assert!(cfg.contains("\"layout\""));
    assert!(cfg.contains("\"clock\""));

    let css = generate_style(&bar);
    assert!(css.contains("window#mangobar"));
    assert!(css.contains("#tag button"));
}

#[test]
fn test_mangowm_items_format() {
    let bind = Bind {
        mods: vec!["SUPER".into(), "SHIFT".into()],
        key: "Return".into(),
        action: "exec".into(),
        args: "kitty".into(),
    };
    assert_eq!(bind.to_config_line(), "bind=SUPER+SHIFT,Return,exec,kitty");

    let rule = Rule {
        pattern: "class:kitty".into(),
        effect: "float".into(),
    };
    assert_eq!(rule.to_config_line(), "rule=class:kitty,float");

    let mon = Monitor {
        name: "eDP-1".into(),
        resolution: "1920x1080@60".into(),
        position: "0x0".into(),
        scale: "1.0".into(),
    };
    assert_eq!(mon.to_config_line(), "monitor=eDP-1,1920x1080@60,0x0,1.0");
}

#[test]
fn test_presets_coverage() {
    let presets = list_presets();
    assert!(!presets.is_empty());
    let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"clean"));
    assert!(ids.contains(&"tokyo-night"));
    assert!(ids.contains(&"zen"));
}
