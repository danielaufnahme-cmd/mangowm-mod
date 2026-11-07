use mangowm_mod::core::manager::ConfigManager;
use mangowm_mod::presets::{
    all_presets, apply_preset, current_preset, current_preset_id, delete_user_preset,
    find_any_preset, find_preset, list_user_presets, overwrite_user_preset, preset_keys,
    preset_modified, rename_user_preset, save_user_preset, wallpaper_path, USER_PRESET_DIR,
};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn manager(dir: &Path) -> ConfigManager {
    ConfigManager::new(
        Some(&dir.join("config.conf")),
        Some(&dir.join("mango-mod.conf")),
        Some(&dir.join("data")),
        true,
    )
    .unwrap()
}

fn modified(mgr: &ConfigManager) -> bool {
    let p = current_preset(&mgr.data_dir).unwrap();
    let w = wallpaper_path(&mgr.data_dir, &p);
    preset_modified(mgr, &p, w.as_deref())
}

#[test]
fn test_save_apply_rename_overwrite_delete_round_trip() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "border_px=4\n").unwrap();
    let pic = tmp.path().join("mine.png");
    fs::write(&pic, b"\x89PNG").unwrap();
    let mut mgr = manager(tmp.path());

    // start from Nord, make it "mine", save it with the wallpaper
    apply_preset(&mut mgr, &find_preset("nord").unwrap()).unwrap();
    mgr.set("focus_color", "#ff0000").unwrap();
    mgr.set("border_radius", "3").unwrap();
    mangowm_mod::wallpaper::stage_wallpaper(&mut mgr, &pic).unwrap();
    let saved = save_user_preset(&mgr, "My Look", true).unwrap();
    assert_eq!(saved.id, "my-look");
    assert!(saved.user);
    let file = tmp
        .path()
        .join("data")
        .join(USER_PRESET_DIR)
        .join("my-look.json");
    assert!(file.is_file());
    assert_eq!(saved.wallpaper.as_deref(), Some(pic.as_path()));
    // the same keys the built-in presets control
    let keys = preset_keys();
    assert!(saved.settings.keys().all(|k| keys.contains(k)));
    assert_eq!(saved.settings["focus_color"], "0xff0000ff");
    assert_eq!(saved.settings["border_radius"], "3");
    assert_eq!(saved.settings["tag_rule"], "id:*,layout_name:tile");
    // saving makes it current, unmodified
    assert_eq!(current_preset_id(&mgr.data_dir).as_deref(), Some("my-look"));
    assert!(!modified(&mgr));

    // listed next to the built-ins and found by name
    let users = list_user_presets(&mgr.data_dir);
    assert_eq!(users.len(), 1);
    assert_eq!(users[0], saved);
    assert_eq!(all_presets(&mgr.data_dir).len(), 13);
    assert_eq!(
        find_any_preset(&mgr.data_dir, "my look").unwrap().id,
        "my-look"
    );

    // apply something else, then the user preset through the same path
    apply_preset(&mut mgr, &find_preset("latte").unwrap()).unwrap();
    assert_eq!(current_preset_id(&mgr.data_dir).as_deref(), Some("latte"));
    let mine = find_any_preset(&mgr.data_dir, "my-look").unwrap();
    let res = apply_preset(&mut mgr, &mine).unwrap();
    assert!(res.ok, "{:?}", res.errors);
    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains("focus_color=0xff0000ff\n"), "{}", text);
    assert!(text.contains("border_radius=3\n"), "{}", text);
    assert!(text.contains(&format!(
        "exec_once=swaybg -m fill -i '{}'\n",
        pic.display()
    )));
    assert!(!text.contains("latte.png"), "{}", text);
    assert_eq!(current_preset_id(&mgr.data_dir).as_deref(), Some("my-look"));
    assert!(!modified(&mgr));

    // rename keeps the id and the file
    let renamed = rename_user_preset(&mgr.data_dir, "my-look", "  Evening ").unwrap();
    assert_eq!(renamed.name, "Evening");
    assert_eq!(renamed.id, "my-look");
    assert_eq!(list_user_presets(&mgr.data_dir)[0].name, "Evening");

    // overwrite takes the new values
    mgr.set("border_px", "5").unwrap();
    assert!(modified(&mgr));
    let over = overwrite_user_preset(&mgr, "Evening").unwrap();
    assert_eq!(over.settings["border_px"], "5");
    assert_eq!(over.name, "Evening");
    assert_eq!(over.wallpaper.as_deref(), Some(pic.as_path()));
    assert!(!modified(&mgr));
    assert_eq!(
        list_user_presets(&mgr.data_dir)[0].settings["border_px"],
        "5"
    );

    // delete removes only that file
    delete_user_preset(&mgr.data_dir, "evening").unwrap();
    assert!(!file.exists());
    assert!(list_user_presets(&mgr.data_dir).is_empty());
    assert!(current_preset(&mgr.data_dir).is_none());
    assert!(delete_user_preset(&mgr.data_dir, "evening").is_err());
}

#[test]
fn test_builtin_presets_are_protected() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "").unwrap();
    let mgr = manager(tmp.path());
    let dir = &mgr.data_dir;
    for name in ["nord", "Tokyo Night"] {
        assert!(delete_user_preset(dir, name).is_err());
        assert!(rename_user_preset(dir, name, "Other").is_err());
        assert!(overwrite_user_preset(&mgr, name).is_err());
    }
    // a built-in's name or id can't be taken, nor an empty one
    assert!(save_user_preset(&mgr, "Nord", false).is_err());
    assert!(save_user_preset(&mgr, "tokyo-night", false).is_err());
    assert!(save_user_preset(&mgr, "   ", false).is_err());
    assert!(find_preset("nord").is_some());

    // two user presets can't share a name either; ids never clash
    let a = save_user_preset(&mgr, "Nord!", false).unwrap();
    assert_eq!(a.id, "nord-2");
    assert!(save_user_preset(&mgr, "nord!", false).is_err());
    let b = save_user_preset(&mgr, "B", false).unwrap();
    assert!(rename_user_preset(dir, &b.id, "Nord!").is_err());
    assert!(rename_user_preset(dir, &b.id, "b").is_ok());
    assert_eq!(find_any_preset(dir, "nord").unwrap().user, false);
}

#[test]
fn test_modified_after_edit_and_wallpaper_change() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "").unwrap();
    let mut mgr = manager(tmp.path());
    assert!(current_preset(&mgr.data_dir).is_none());

    apply_preset(&mut mgr, &find_preset("glass").unwrap()).unwrap();
    assert_eq!(current_preset_id(&mgr.data_dir).as_deref(), Some("glass"));
    assert!(!modified(&mgr));

    // an equivalent spelling is not a change
    mgr.set("focus_color", "#9FD8EF").unwrap();
    assert!(!modified(&mgr));
    mgr.set("border_px", "2").unwrap();
    assert!(modified(&mgr));
    mgr.set("border_px", "1").unwrap();
    assert!(!modified(&mgr));
    mgr.reset("focused_opacity").unwrap();
    assert!(modified(&mgr));
    mgr.set("focused_opacity", "0.92").unwrap();
    assert!(!modified(&mgr));

    // a different wallpaper counts too
    let pic = tmp.path().join("other.png");
    fs::write(&pic, b"\x89PNG").unwrap();
    mangowm_mod::wallpaper::stage_wallpaper(&mut mgr, &pic).unwrap();
    assert!(modified(&mgr));
}

#[test]
fn test_user_preset_without_wallpaper_keeps_the_wallpaper() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "").unwrap();
    let mut mgr = manager(tmp.path());
    apply_preset(&mut mgr, &find_preset("nord").unwrap()).unwrap();
    mgr.set("border_px", "6").unwrap();
    let p = save_user_preset(&mgr, "Thick", false).unwrap();
    assert!(p.wallpaper.is_none());
    let json = fs::read_to_string(
        tmp.path()
            .join("data")
            .join(USER_PRESET_DIR)
            .join("thick.json"),
    )
    .unwrap();
    assert!(!json.contains("wallpaper"), "{}", json);

    apply_preset(&mut mgr, &find_preset("latte").unwrap()).unwrap();
    let thick = find_any_preset(&mgr.data_dir, "thick").unwrap();
    let res = apply_preset(&mut mgr, &thick).unwrap();
    assert!(res.ok);
    assert!(
        !res.warnings.iter().any(|w| w.contains("wallpaper")),
        "{:?}",
        res.warnings
    );
    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains("border_px=6\n"), "{}", text);
    assert!(text.contains("latte.png'"), "{}", text);
    assert_eq!(text.matches("exec_once=swaybg").count(), 1);
    assert!(!modified(&mgr));
}

#[test]
fn test_broken_user_preset_file_is_skipped() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join(USER_PRESET_DIR);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.json"), "{ not json").unwrap();
    fs::write(dir.join("notes.txt"), "x").unwrap();
    assert!(list_user_presets(tmp.path()).is_empty());
}
