use mangowm_mod::core::items::{Bind, Monitor, Rule};
use mangowm_mod::core::managed::{parse_managed, render, ManagedState};
use mangowm_mod::core::manager::ConfigManager;
use mangowm_mod::core::parser::strip_comment;
use mangowm_mod::core::schema::{find_option, get_schema_pages};
use mangowm_mod::core::schema_data::KNOWN_MANGO_KEYS;
use mangowm_mod::core::values::format_option_value;
use mangowm_mod::ipc::ReloadStatus;
use mangowm_mod::presets::{apply_preset, find_preset, list_presets, wallpaper_path};
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

#[test]
fn test_mangowm_lifecycle() {
    let tmp = tempdir().unwrap();
    let config_file = tmp.path().join("config.conf");
    let managed_file = tmp.path().join("mango-mod.conf");

    fs::write(
        &config_file,
        "\nborder_px=2\nborder_radius=8\ngap_inner_horizontal=4\n",
    )
    .unwrap();

    let mut mgr = manager(tmp.path());
    assert_eq!(mgr.get("border_px"), Some("2".to_string()));

    mgr.set("border_px", "3").unwrap();
    mgr.set("focus_color", "#89b4fa").unwrap();
    assert!(mgr.is_dirty());

    let res = mgr.apply().unwrap();
    assert!(res.ok);
    assert!(res.backup_id.is_some());
    assert!(!mgr.is_dirty());
    assert!(res.created_config.is_none());

    let managed_text = fs::read_to_string(&managed_file).unwrap();
    assert!(managed_text.contains("border_px=3"));
    assert!(managed_text.contains("focus_color=0x89b4faff"));

    let main_text = fs::read_to_string(&config_file).unwrap();
    assert!(main_text.starts_with("\nborder_px=2\n"));
    assert!(main_text.trim_end().ends_with("source=./mango-mod.conf"));
    assert_eq!(main_text.matches("source=").count(), 1);

    // the original config.conf was backed up before the source line was added
    let backups = mgr.backups.list();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].files.iter().any(|f| f.ends_with("config.conf")));
    let copy_dir = tmp
        .path()
        .join("data/backups")
        .join(&backups[0].id)
        .join("files");
    let copies: Vec<String> = fs::read_dir(copy_dir)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert!(copies
        .iter()
        .any(|c| c == "\nborder_px=2\nborder_radius=8\ngap_inner_horizontal=4\n"));

    // a second apply adds no second source line
    mgr.set("border_px", "4").unwrap();
    mgr.apply().unwrap();
    let main_text = fs::read_to_string(&config_file).unwrap();
    assert_eq!(main_text.matches("source=").count(), 1);
}

#[test]
fn test_backup_failure_changes_nothing() {
    let tmp = tempdir().unwrap();
    let config_file = tmp.path().join("config.conf");
    fs::write(&config_file, "border_px=2\n").unwrap();
    // data dir is a file, so the backup cannot be created
    fs::write(tmp.path().join("data"), "not a dir").unwrap();
    let mut mgr = ConfigManager::new(
        Some(&config_file),
        Some(&tmp.path().join("mango-mod.conf")),
        Some(&tmp.path().join("data")),
        true,
    )
    .unwrap();
    // set() cannot save staged.json here, but the change stays staged in memory
    let _ = mgr.set("border_px", "5");
    assert!(mgr.is_dirty());
    let err = mgr.apply();
    assert!(err.is_err());
    assert_eq!(fs::read_to_string(&config_file).unwrap(), "border_px=2\n");
    assert!(!tmp.path().join("mango-mod.conf").exists());
}

#[test]
fn test_staging_persists_across_runs() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "border_px=2\n").unwrap();
    {
        let mut mgr = manager(tmp.path());
        assert_eq!(mgr.set("border_px", "6").unwrap(), "6");
        mgr.add_bind(Bind::new("SUPER", "Return", "spawn", "foot"), false)
            .unwrap();
        mgr.add_autostart("waybar", false).unwrap();
    }
    let mut mgr = manager(tmp.path());
    assert!(mgr.is_dirty());
    assert_eq!(mgr.get("border_px"), Some("6".into()));
    assert_eq!(
        mgr.pending_changes(),
        vec![("border_px".to_string(), Some("6".to_string()))]
    );
    assert_eq!(mgr.pending_item_changes().len(), 2);

    let res = mgr.apply().unwrap();
    assert!(res.ok);
    assert!(!res.live_applied);
    assert_eq!(res.reload, Some(ReloadStatus::DryRun));
    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains("border_px=6"));
    assert!(text.contains("bind=SUPER,Return,spawn,foot"));
    assert!(text.contains("exec_once=waybar"));
    assert!(!tmp.path().join("data/staged.json").exists());

    // discard drops staged changes
    let mut mgr = manager(tmp.path());
    mgr.set("border_px", "9").unwrap();
    mgr.discard().unwrap();
    assert!(!manager(tmp.path()).is_dirty());
}

#[test]
fn test_config_toml_blocks_config_conf() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.toml"), "[global]\n").unwrap();
    let mut mgr = manager(tmp.path());
    mgr.set("border_px", "3").unwrap();
    let res = mgr.apply().unwrap();
    assert!(!res.ok);
    assert!(!tmp.path().join("config.conf").exists());
    assert_eq!(res.errors.len(), 1);
    assert!(
        res.errors[0].contains("source = [\"./mango-mod.conf\"]"),
        "{}",
        res.errors[0]
    );
}

#[test]
fn test_value_formats() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "").unwrap();
    let mut mgr = manager(tmp.path());
    assert_eq!(mgr.set("blur", "true").unwrap(), "1");
    assert_eq!(mgr.set("shadows", "off").unwrap(), "0");
    assert_eq!(mgr.set("border_color", "#313244").unwrap(), "0x313244ff");
    assert_eq!(
        mgr.set("tag_rule", "id:*,layout_name:scroller").unwrap(),
        "id:*,layout_name:scroller"
    );
    assert!(mgr.set("blur", "maybe").is_err());
    assert!(mgr.set("tag_rule", "id:*,layout_name:nope").is_err());
    assert!(mgr.set("no_such_option", "1").is_err());
    mgr.apply().unwrap();

    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains("blur=1"));
    assert!(text.contains("shadows=0"));
    assert!(text.contains("border_color=0x313244ff"));
    assert!(text.contains("tag_rule=id:*,layout_name:scroller"));
}

#[test]
fn test_schema_matches_known_keys() {
    let schema_keys: Vec<String> = get_schema_pages()
        .into_iter()
        .flat_map(|p| p.options)
        .map(|o| o.key)
        .collect();
    for k in &schema_keys {
        assert!(
            KNOWN_MANGO_KEYS.contains(&k.as_str()),
            "schema key {} is not a Mango key",
            k
        );
    }
    for k in KNOWN_MANGO_KEYS.iter().filter(|k| **k != "tag_rule") {
        assert!(
            schema_keys.iter().any(|s| s == k),
            "Mango key {} missing from schema",
            k
        );
    }
    let mut sorted = schema_keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), schema_keys.len(), "duplicate schema keys");
}

#[test]
fn test_presets_use_schema_keys() {
    let presets = list_presets();
    let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"clean"));
    assert!(ids.contains(&"tokyo-night"));
    assert!(ids.contains(&"zen"));
    assert_eq!(presets.len(), 12);
    for p in &presets {
        for (k, v) in &p.settings {
            let opt =
                find_option(k).unwrap_or_else(|| panic!("preset {} uses unknown key {}", p.id, k));
            format_option_value(&opt.r#type, &opt.choices, v)
                .unwrap_or_else(|e| panic!("preset {} {}={}: {}", p.id, k, v, e));
        }
    }
}

fn sample_state() -> ManagedState {
    let mut state = ManagedState::default();
    state.settings.insert("border_px".into(), "3".into());
    state
        .settings
        .insert("focus_color".into(), "0x89b4faff".into());
    state
        .settings
        .insert("tag_rule".into(), "id:*,layout_name:scroller".into());
    state
        .binds
        .push(Bind::new("SUPER", "Return", "spawn", "foot"));
    let mut locked = Bind::new(
        "NONE",
        "XF86AudioMute",
        "spawn",
        "wpctl set-mute @DEFAULT_SINK@ toggle",
    );
    locked.flags = "l".into();
    state.binds.push(locked);
    let mut resize = Bind::new("NONE", "h", "resizewin", "-10,0");
    resize.mode = "resize".into();
    state.binds.push(resize);
    state.binds.push(Bind::new("ALT", "q", "killclient", ""));
    state
        .rules
        .push(Rule::parse("window_rule", "app_id:mpv,is_floating:1").unwrap());
    state
        .rules
        .push(Rule::parse("tag_rule", "id:2,layout_name:monocle").unwrap());
    state.rules.push(
        Monitor {
            name: "eDP-1".into(),
            scale: Some(1.5),
            ..Default::default()
        }
        .to_rule()
        .unwrap(),
    );
    state.exec_once.push("waybar".into());
    state.exec.push("pkill -SIGUSR2 waybar".into());
    state
        .lists
        .insert("env".into(), vec!["XCURSOR_SIZE,24".into()]);
    state
}

#[test]
fn test_render_parse_round_trip() {
    let state = sample_state();
    let text = render(&state);
    assert!(
        !text.contains('{') && !text.contains('}'),
        "no JSON in the managed file:\n{}",
        text
    );
    assert!(text.contains("\nexec_once=waybar\n"));
    assert!(text.contains("\nexec=pkill -SIGUSR2 waybar\n"));
    assert!(!text.contains("exec-once"));
    assert!(text.contains("bindl=NONE,XF86AudioMute,spawn,wpctl set-mute @DEFAULT_SINK@ toggle"));
    assert!(text.contains("key_mode=resize\nbind=NONE,h,resizewin,-10,0\nkey_mode=default\n"));
    assert!(text.contains("monitor_rule=name:eDP-1,scale:1.5"));
    assert!(text.contains("env=XCURSOR_SIZE,24"));
    assert_eq!(parse_managed(&text), state);
}

#[test]
fn test_legacy_managed_file_loads() {
    let legacy = "# MangoWM Mod managed settings. Generated file: manual edits may be overwritten.\n\n\
                  border_px=2\nfocus_color=0x89b4faff\n\ntag_rule=id:*,layout_name:tile\n\n\
                  # ---- MangoWM Mod: binds, rules, monitors, autostart ----\n\
                  # mangowm-mod-extras: {\"settings\":{},\"lists\":{},\"binds\":[{\"mods\":[\"SUPER\"],\"key\":\"Return\",\"action\":\"exec\",\"args\":\"kitty\"}]}\n";
    let state = parse_managed(legacy);
    assert_eq!(
        state.settings.get("border_px").map(String::as_str),
        Some("2")
    );
    assert_eq!(
        state.settings.get("focus_color").map(String::as_str),
        Some("0x89b4faff")
    );
    assert_eq!(
        state.settings.get("tag_rule").map(String::as_str),
        Some("id:*,layout_name:tile")
    );
    assert!(state.binds.is_empty());
    assert!(state.rules.is_empty());

    // and the manager reads it as the current managed state
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "source=./mango-mod.conf\n").unwrap();
    fs::write(tmp.path().join("mango-mod.conf"), legacy).unwrap();
    let mgr = manager(tmp.path());
    assert_eq!(mgr.get("border_px"), Some("2".into()));
    assert!(!mgr.is_dirty());
}

#[test]
fn test_bind_lines_and_validation() {
    let mut bind = Bind::new("SUPER+SHIFT", "Return", "spawn", "kitty");
    assert_eq!(bind.to_config_line(), "bind=SUPER+SHIFT,Return,spawn,kitty");
    bind.validate().unwrap();
    bind.flags = "l".into();
    assert_eq!(
        bind.to_config_line(),
        "bindl=SUPER+SHIFT,Return,spawn,kitty"
    );
    bind.validate().unwrap();
    bind.flags = "x".into();
    assert!(bind.validate().is_err());

    assert!(Bind::new("META", "q", "killclient", "").validate().is_err());
    assert!(Bind::new("SUPER", "q", "exec", "kitty").validate().is_err());
    assert!(Bind::new("SUPER", "", "killclient", "").validate().is_err());
    Bind::new("ALT", "q", "killclient", "").validate().unwrap();
    Bind::new("super+ctrl", "F1", "spawn", "foot -e htop")
        .validate()
        .unwrap();
    assert_eq!(
        Bind::new("", "Print", "spawn", "grim").to_config_line(),
        "bind=NONE,Print,spawn,grim"
    );

    let parsed = Bind::parse("bindr", "SUPER,space,spawn,rofi -show drun", "default").unwrap();
    assert_eq!(parsed.flags, "r");
    assert_eq!(parsed.args, "rofi -show drun");
    assert!(Bind::parse("bindx", "SUPER,a,zoom", "default").is_none());
}

#[test]
fn test_rule_validation() {
    let r = Rule::parse("window_rule", "app_id:mpv, is_floating:1").unwrap();
    assert_eq!(r.to_config_line(), "window_rule=app_id:mpv,is_floating:1");
    assert!(Rule::parse("window_rule", "app_id:mpv,floaty:1").is_err());
    assert!(Rule::parse("window_rule", "nonsense").is_err());
    assert!(Rule::parse("window_rule", "").is_err());
    assert!(Rule::parse("bad_rule", "app_id:x").is_err());
    Rule::parse("tag_rule", "id:1,layout_name:scroller").unwrap();
    assert!(Rule::parse("tag_rule", "id:1,layout_name:spiral").is_err());
    Rule::parse("layer_rule", "layer_name:waybar,no_blur:1").unwrap();
    assert!(Rule::parse("layer_rule", "layer_name:waybar,animation_type_open:spin").is_err());
    Rule::parse("device_rule", "name:my-keyboard,kb_layout:de").unwrap();
}

#[test]
fn test_monitor_rule_line() {
    let mon = Monitor {
        name: "eDP-1".into(),
        width: Some(1920),
        height: Some(1080),
        refresh: Some(60.0),
        x: Some(0),
        y: Some(0),
        scale: Some(1.0),
        ..Default::default()
    };
    assert_eq!(
        mon.to_config_line(),
        "monitor_rule=name:eDP-1,width:1920,height:1080,refresh:60,x:0,y:0,scale:1"
    );
    let rotated = Monitor {
        name: "HDMI-A-1".into(),
        transform: Some(1),
        vrr: Some(true),
        ..Default::default()
    };
    assert_eq!(
        rotated.to_config_line(),
        "monitor_rule=name:HDMI-A-1,rr:1,vrr:1"
    );
    assert!(Monitor {
        name: "".into(),
        ..Default::default()
    }
    .to_rule()
    .is_err());
    assert!(Monitor {
        name: "X".into(),
        width: Some(1),
        ..Default::default()
    }
    .to_rule()
    .is_err());
    assert!(Monitor {
        name: "X".into(),
        transform: Some(8),
        ..Default::default()
    }
    .to_rule()
    .is_err());
}

#[test]
fn test_user_monitor_rule_warning() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.conf"),
        "monitor_rule=name:eDP-1,width:2560,height:1600\n",
    )
    .unwrap();
    let mgr = manager(tmp.path());
    assert_eq!(
        mgr.user_monitor_rules_for("eDP-1"),
        vec!["monitor_rule=name:eDP-1,width:2560,height:1600"]
    );
    assert!(mgr.user_monitor_rules_for("DP-1").is_empty());
}

#[test]
fn test_bind_conflicts() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.conf"),
        "bind=SUPER,Return,spawn,foot\n",
    )
    .unwrap();
    let mut mgr = manager(tmp.path());
    assert!(mgr
        .add_bind(Bind::new("super", "return", "spawn", "kitty"), false)
        .is_err());
    let mut allowed = Bind::new("SUPER", "Return", "spawn", "kitty");
    allowed.flags = "c".into();
    mgr.add_bind(allowed, false).unwrap();
    mgr.add_bind(Bind::new("SUPER", "e", "spawn", "nautilus"), false)
        .unwrap();
    assert!(mgr
        .add_bind(Bind::new("SUPER", "e", "spawn", "thunar"), false)
        .is_err());
    mgr.add_bind(Bind::new("SUPER", "e", "spawn", "thunar"), true)
        .unwrap();
    assert_eq!(mgr.staged().binds.len(), 2);
    assert_eq!(mgr.staged().binds[1].args, "thunar");
    assert_eq!(mgr.remove_bind(1).unwrap().args, "kitty");
    assert!(mgr.remove_bind(5).is_err());
}

#[test]
fn test_strip_comment_mango_semantics() {
    assert_eq!(
        strip_comment("bind=SUPER,a,spawn,foo#bar"),
        "bind=SUPER,a,spawn,foo#bar"
    );
    assert_eq!(strip_comment("x=1 # c"), "x=1");
    assert_eq!(strip_comment("  # whole line"), "");
    assert_eq!(
        strip_comment("exec_once=\"notify-send '# x'\""),
        "exec_once=\"notify-send '# x'\""
    );
    assert_eq!(strip_comment("title='a # b' # c"), "title='a # b'");
}

#[test]
fn test_every_preset_tiles_and_sets_wallpaper() {
    for p in list_presets() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("config.conf"), "border_px=4\n").unwrap();
        let mut mgr = manager(tmp.path());
        let res = apply_preset(&mut mgr, &p).unwrap();
        assert!(res.ok, "preset {}: {:?}", p.id, res.errors);

        let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
        assert!(
            text.contains("tag_rule=id:*,layout_name:tile\n"),
            "preset {} does not tile:\n{}",
            p.id,
            text
        );
        assert!(text.contains("new_is_master=0\n"), "preset {}", p.id);
        let swaybg: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("exec_once=swaybg"))
            .collect();
        assert_eq!(swaybg.len(), 1, "preset {}:\n{}", p.id, text);
        let file = format!("/{}.png'", p.id);
        assert!(swaybg[0].ends_with(&file), "{}", swaybg[0]);
        let path = wallpaper_path(&mgr.data_dir, &p).unwrap();
        assert!(path.is_file(), "missing wallpaper {}", path.display());
        assert!(
            fs::read(&path).unwrap().starts_with(b"\x89PNG"),
            "{} is not a PNG",
            path.display()
        );
    }
}

#[test]
fn test_preset_wallpaper_replaces_previous_one() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.conf"),
        "exec_once=swaybg -m fill -i /old.png\n",
    )
    .unwrap();
    // an installed copy in the data dir wins over the source checkout
    let installed = tmp.path().join("data").join("wallpapers");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("nord.png"), b"\x89PNG").unwrap();

    let mut mgr = manager(tmp.path());
    mgr.add_autostart("waybar", false).unwrap();
    mgr.set("shadows", "1").unwrap();
    apply_preset(&mut mgr, &find_preset("glass").unwrap()).unwrap();
    let res = apply_preset(&mut mgr, &find_preset("Nord").unwrap()).unwrap();
    assert!(res.ok);

    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains("exec_once=waybar\n"), "{}", text);
    assert!(!text.contains("glass.png"), "{}", text);
    // Glass's opacity does not leak into Nord; the user's own setting stays
    assert!(!text.contains("focused_opacity"), "{}", text);
    assert!(text.contains("shadows=1\n"), "{}", text);
    let line = format!(
        "exec_once=swaybg -m fill -i '{}'",
        installed.join("nord.png").display()
    );
    assert!(text.contains(&line), "{}", text);
    assert_eq!(text.matches("exec_once=swaybg").count(), 1);
    // config.conf is never edited for this, but its own swaybg is pointed out
    // (only when swaybg is installed; the check is skipped otherwise)
    let warned = res.warnings.iter().any(|w| w.contains("/old.png"));
    let has_swaybg = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("swaybg").is_file()))
        .unwrap_or(false);
    assert_eq!(warned, has_swaybg, "{:?}", res.warnings);
}

#[test]
fn test_swaybg_line_round_trip() {
    use mangowm_mod::wallpaper::{swaybg_command, swaybg_image};
    for p in ["/w/plain.png", "/w/with space.jpg", "/w/it's.png"] {
        let cmd = swaybg_command(Path::new(p));
        assert_eq!(swaybg_image(&cmd).as_deref(), Some(Path::new(p)), "{}", cmd);
    }
    assert_eq!(
        swaybg_image("swaybg -m fill -i /home/u/a.png").as_deref(),
        Some(Path::new("/home/u/a.png"))
    );
    assert_eq!(swaybg_image("waybar -i /x.png"), None);
}

#[test]
fn test_plasma_wallpaper_parsing() {
    use mangowm_mod::wallpaper::plasma_wallpaper;
    let rc = "\
[Containments][1][General]
Image=file:///wrong.png

[Containments][2][Applets][5][Configuration]
Image=file:///also-wrong.png

[Containments][2][Wallpaper][org.kde.image][General]
PreviewImage=file:///preview.png
Image=file:///home/u/Pictures/My%20Walls/forest.png
";
    assert_eq!(
        plasma_wallpaper(rc).as_deref(),
        Some(Path::new("/home/u/Pictures/My Walls/forest.png"))
    );
    assert_eq!(plasma_wallpaper("[Containments][1][General]\n"), None);
}

#[test]
fn test_wallpaper_and_presets_share_one_line() {
    use mangowm_mod::wallpaper::set_wallpaper;
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "border_px=4\n").unwrap();
    let pic = tmp.path().join("mine.jpg");
    fs::write(&pic, b"jpg").unwrap();
    let mut mgr = manager(tmp.path());
    mgr.add_autostart("waybar", false).unwrap();

    apply_preset(&mut mgr, &find_preset("nord").unwrap()).unwrap();
    // a picked wallpaper replaces only the preset's wallpaper
    assert!(set_wallpaper(&mut mgr, &pic).unwrap().ok);
    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(text.contains(&format!("exec_once=swaybg -m fill -i '{}'\n", pic.display())));
    assert_eq!(text.matches("exec_once=swaybg").count(), 1, "{}", text);
    assert!(text.contains("focus_color=0x88c0d0ff\n"), "{}", text);
    assert!(text.contains("tag_rule=id:*,layout_name:tile\n"), "{}", text);
    assert!(text.contains("exec_once=waybar\n"), "{}", text);

    // and a preset afterwards brings its own wallpaper back
    apply_preset(&mut mgr, &find_preset("latte").unwrap()).unwrap();
    let text = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert_eq!(text.matches("exec_once=swaybg").count(), 1, "{}", text);
    assert!(text.contains("latte.png'"), "{}", text);
    assert!(!text.contains("mine.jpg"), "{}", text);

    assert!(set_wallpaper(&mut mgr, &tmp.path().join("missing.png")).is_err());
}

#[test]
fn test_collect_wallpapers_order_and_marks() {
    use mangowm_mod::wallpaper::{collect_wallpapers, mango_wallpaper};
    let tmp = tempdir().unwrap();
    let home = tmp.path().join("home");
    let bundled = tmp.path().join("bundled");
    let mine = home.join("Pictures/Wallpapers");
    let other = home.join("Pictures/wallpapers");
    for d in [&bundled, &mine, &other] {
        fs::create_dir_all(d).unwrap();
    }
    for f in [
        bundled.join("nord.png"),
        mine.join("forest.png"),
        other.join("b.jpg"),
        other.join("b.png"),
        other.join("a.JPEG"),
        other.join("notes.txt"),
    ] {
        fs::write(f, b"x").unwrap();
    }
    let plasma = mine.join("forest.png");
    let mango = other.join("b.png");
    let list = collect_wallpapers(&bundled, &home, Some(&plasma), Some(&mango));
    let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
    // Plasma's first, shown once; .jpg and .png of the same name both kept
    assert_eq!(names, ["forest", "nord", "a", "b", "b"]);
    assert!(list[0].in_plasma && !list[0].bundled);
    assert!(list[1].bundled);
    assert_eq!(list.iter().filter(|e| e.in_mango).count(), 1);
    assert_eq!(list[4].path, mango);
    assert!(list[4].in_mango);

    // Mango's current wallpaper comes from config.conf until one is managed
    fs::write(
        tmp.path().join("config.conf"),
        format!("exec_once=swaybg -m fill -i {}\n", mango.display()),
    )
    .unwrap();
    let mut mgr = manager(tmp.path());
    assert_eq!(mango_wallpaper(&mgr), Some(mango.clone()));
    mangowm_mod::wallpaper::set_wallpaper(&mut mgr, &plasma).unwrap();
    assert_eq!(mango_wallpaper(&mgr), Some(plasma));
}
