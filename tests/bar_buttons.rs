//! Custom bar buttons, the memory display choice, user bar presets and the
//! live update. Clicks run fake player/launcher scripts in a temp dir; no
//! sound is played, no app opened and no real bar touched.

use clap::Parser;
use mangowm_mod::bar::buttons::{
    button_id, click_argvs, click_command, import_picture, run_click, BarButton, Click,
    ClickAction, Tools,
};
use mangowm_mod::bar::generate::{generate_config, generate_style, warnings};
use mangowm_mod::bar::model::{memory_displays, Backend, BarConfig, MEMORY_DISPLAYS};
use mangowm_mod::bar::presets::{
    delete_user_preset, find_bar_preset, list_user_presets, overwrite_user_preset,
    rename_user_preset, save_user_preset, slugify,
};
use mangowm_mod::bar::process::{update_live, LiveUpdate};
use mangowm_mod::cli::{BarCommands, ButtonCommands, Cli, Commands};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

/// The generated config.jsonc as JSON (without the comment line).
fn config_json(bar: &BarConfig) -> Value {
    let text = generate_config(bar);
    let body: String = text.lines().skip(1).collect::<Vec<_>>().join("\n");
    serde_json::from_str(&body).expect("generated config is valid JSON")
}

fn emoji_button() -> BarButton {
    BarButton {
        emoji: "🔔".into(),
        label: "Ring".into(),
        left: ClickAction {
            command: "notify-send hi".into(),
            ..Default::default()
        },
        right_same: true,
        ..Default::default()
    }
}

/// A script that appends its name and arguments to `log`.
fn fake_tool(dir: &Path, name: &str, log: &Path) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\necho \"{} $*\" >> '{}'\n", name, log.display()),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn wait_lines(log: &Path, n: usize) -> Vec<String> {
    for _ in 0..50 {
        let lines: Vec<String> = fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        if lines.len() >= n {
            let mut lines = lines;
            lines.sort();
            return lines;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("only got {:?}", fs::read_to_string(log).unwrap_or_default());
}

#[test]
fn test_add_move_remove_buttons() {
    let mut bar = BarConfig::default();
    let a = bar.add_button(emoji_button(), "right").unwrap();
    let b = bar.add_button(emoji_button(), "left").unwrap();
    assert_eq!((a.as_str(), b.as_str()), ("1", "2"));
    assert_eq!(bar.modules.right.last().unwrap(), "custom/btn-1");
    assert_eq!(bar.modules.left.last().unwrap(), "custom/btn-2");
    assert_eq!(button_id("custom/btn-2"), Some("2"));
    assert!(bar.validate().is_ok());

    // moves like any module, and through its section setting
    let i = bar.modules.right.len() - 1;
    bar.modules.shift("right", i, true).unwrap();
    assert_eq!(bar.button_section("1"), Some("right"));
    bar.set_button_section("1", Some("center")).unwrap();
    assert_eq!(bar.button_section("1"), Some("center"));
    bar.set_button_section("1", None).unwrap();
    assert_eq!(bar.button_section("1"), None);
    assert!(bar.button("1").is_some());

    assert!(bar.remove_button("2"));
    assert!(!bar.modules.all().any(|m| m == "custom/btn-2"));
    assert!(!bar.remove_button("2"));
    // ids are reused once free
    assert_eq!(bar.add_button(emoji_button(), "right").unwrap(), "2");

    // braces would be placeholders in both bars
    let mut bad = emoji_button();
    bad.label = "a {} b".into();
    assert!(bar.add_button(bad, "right").is_err());

    // saved and loaded
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("bar.json");
    bar.save(&path).unwrap();
    assert_eq!(BarConfig::load(&path).unwrap(), bar);
}

#[test]
fn test_mangobar_button_config() {
    let mut bar = BarConfig::default();
    bar.add_button(emoji_button(), "right").unwrap();
    let cfg = config_json(&bar);
    assert!(cfg["modules-right"]
        .as_array()
        .unwrap()
        .contains(&Value::from("custom/btn-1")));
    let m = &cfg["custom/btn-1"];
    assert_eq!(m["format"], "🔔");
    let left = m["on-click"].as_str().unwrap();
    let right = m["on-click-right"].as_str().unwrap();
    assert!(left.ends_with("bar button run 1 left"), "{}", left);
    assert!(right.ends_with("bar button run 1 right"), "{}", right);
    // styled like the other modules
    let css = generate_style(&bar);
    assert!(css.contains("#custom-btn-1"), "{}", css);

    // a picture cannot be drawn: the emoji is shown and the user is told
    bar.button_mut("1").unwrap().picture = "/tmp/x.png".into();
    let cfg = config_json(&bar);
    assert_eq!(cfg["custom/btn-1"]["format"], "🔔");
    assert!(warnings(&bar).iter().any(|w| w.contains("pictures need Waybar")));
    // no emoji: the label
    bar.button_mut("1").unwrap().emoji.clear();
    assert_eq!(config_json(&bar)["custom/btn-1"]["format"], "Ring");
}

#[test]
fn test_waybar_button_config() {
    let mut bar = BarConfig::default();
    bar.backend = Backend::Waybar;
    bar.add_button(emoji_button(), "right").unwrap();
    let mut pic = emoji_button();
    pic.picture = "/data/bar-icons/2-cat.png".into();
    pic.label = "Cat & co".into();
    bar.add_button(pic, "left").unwrap();

    let cfg = config_json(&bar);
    let m = &cfg["custom/btn-1"];
    assert_eq!(m["format"], "🔔");
    assert_eq!(m["tooltip-format"], "Ring");
    assert!(m["on-click"].as_str().unwrap().ends_with("run 1 left"));
    assert!(m["on-click-right"].as_str().unwrap().ends_with("run 1 right"));

    // picture: Waybar's image module; the tooltip comes from its exec
    assert!(cfg["modules-left"]
        .as_array()
        .unwrap()
        .contains(&Value::from("image#btn-2")));
    let img = &cfg["image#btn-2"];
    assert!(img["exec"].as_str().unwrap().contains("2-cat.png"));
    assert!(img["exec"].as_str().unwrap().contains("Cat & co"));
    assert!(img["size"].as_u64().unwrap() >= 16);
    assert!(img["on-click-right"].as_str().unwrap().ends_with("run 2 right"));
    let css = generate_style(&bar);
    assert!(css.contains("#image.btn-2"), "{}", css);
    assert!(css.contains("#custom-btn-1"), "{}", css);
    assert!(warnings(&bar).iter().all(|w| !w.contains("picture")));

    // no label: the path directly, no tooltip
    bar.button_mut("2").unwrap().label.clear();
    let cfg = config_json(&bar);
    assert_eq!(cfg["image#btn-2"]["path"], "/data/bar-icons/2-cat.png");
    assert_eq!(cfg["image#btn-2"]["tooltip"], false);
}

#[test]
fn test_click_runs_sound_command_and_app_together() {
    let tmp = tempdir().unwrap();
    let log = tmp.path().join("log");
    let tools = Tools {
        player: Some(fake_tool(tmp.path(), "player", &log)),
        launcher: Some(fake_tool(tmp.path(), "launcher", &log)),
        gio: None,
        shell: PathBuf::from("/bin/sh"),
    };
    let mut b = BarButton {
        id: "1".into(),
        emoji: "🔔".into(),
        left: ClickAction {
            sound: "/sounds/ding.oga".into(),
            command: format!("echo command ran >> '{}'", log.display()),
            app: "org.example.App.desktop".into(),
        },
        right: ClickAction {
            command: format!("echo right ran >> '{}'", log.display()),
            ..Default::default()
        },
        right_same: true,
        ..Default::default()
    };

    let pids = run_click(&b, Click::Left, &tools).unwrap();
    assert_eq!(pids.len(), 3);
    assert_eq!(
        wait_lines(&log, 3),
        [
            "command ran",
            "launcher org.example.App",
            "player /sounds/ding.oga"
        ]
    );

    // "same as left": a right click runs the left action
    fs::remove_file(&log).unwrap();
    run_click(&b, Click::Right, &tools).unwrap();
    assert_eq!(wait_lines(&log, 3).len(), 3);

    // its own right action
    fs::remove_file(&log).unwrap();
    b.right_same = false;
    run_click(&b, Click::Right, &tools).unwrap();
    assert_eq!(wait_lines(&log, 1), ["right ran"]);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);

    // an empty action does nothing
    assert!(run_click(&BarButton::default(), Click::Left, &tools)
        .unwrap()
        .is_empty());
}

#[test]
fn test_click_argvs_and_missing_tools() {
    let action = ClickAction {
        sound: "/s.wav".into(),
        command: "true".into(),
        app: "foo.desktop".into(),
    };
    let tools = Tools {
        player: Some("/usr/bin/pw-play".into()),
        launcher: Some("/usr/bin/gtk-launch".into()),
        gio: None,
        shell: "/bin/sh".into(),
    };
    let (argvs, missing) = click_argvs(&action, &tools);
    assert!(missing.is_empty());
    assert_eq!(argvs[0], ["/usr/bin/pw-play", "/s.wav"]);
    assert_eq!(argvs[1], ["/bin/sh", "-c", "true"]);
    assert_eq!(argvs[2], ["/usr/bin/gtk-launch", "foo"]);

    let none = Tools {
        shell: "/bin/sh".into(),
        ..Default::default()
    };
    let (argvs, missing) = click_argvs(&action, &none);
    // the command still runs; the rest is reported
    assert_eq!(argvs.len(), 1);
    assert_eq!(missing.len(), 2);
    assert!(run_click(
        &BarButton {
            id: "1".into(),
            left: ClickAction {
                sound: "/s.wav".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        Click::Left,
        &none
    )
    .is_err());
}

#[test]
fn test_click_command_and_cli() {
    let cmd = click_command(Some(Path::new("/tmp/other data")), "3", Click::Right);
    assert!(cmd.contains("bar button run 3 right --data-dir '/tmp/other data'"), "{}", cmd);

    let cli = Cli::try_parse_from(["mangowm-mod", "bar", "button", "run", "3", "right"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Bar {
            action: BarCommands::Button {
                action: ButtonCommands::Run { ref id, click: Click::Right }
            }
        }) if id == "3"
    ));
}

#[test]
fn test_cli_add_list_run_remove() {
    let tmp = tempdir().unwrap();
    let data = tmp.path().join("data");
    let log = tmp.path().join("log");
    let pic = tmp.path().join("cat.png");
    fs::write(&pic, b"png").unwrap();
    let run = |args: &[&str]| {
        let mut argv = vec!["mangowm-mod", "--data-dir", data.to_str().unwrap(), "bar", "button"];
        argv.extend(args);
        mangowm_mod::cli::run_cli(Cli::try_parse_from(argv).unwrap())
    };
    let cmd = format!("echo clicked >> '{}'", log.display());
    run(&["add", "--emoji", "🐱", "--command", &cmd, "--picture", pic.to_str().unwrap()])
        .unwrap();
    run(&["list"]).unwrap();
    let bar = BarConfig::load(&data.join("bar.json")).unwrap();
    let b = bar.button("1").unwrap();
    // the picture was copied into the data dir
    assert!(b.picture.starts_with(data.to_str().unwrap()));
    assert!(Path::new(&b.picture).is_file());
    assert_eq!(bar.button_section("1"), Some("right"));

    run(&["run", "1", "left"]).unwrap();
    assert_eq!(wait_lines(&log, 1), ["clicked"]);
    run(&["remove", "1"]).unwrap();
    assert!(BarConfig::load(&data.join("bar.json")).unwrap().buttons.is_empty());
    assert!(run(&["run", "1", "left"]).is_err());
}

#[test]
fn test_import_picture() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("pic.svg");
    fs::write(&src, "<svg/>").unwrap();
    let copy = import_picture(&tmp.path().join("data"), "4", &src).unwrap();
    assert!(copy.ends_with("bar-icons/4-pic.svg"));
    fs::remove_file(&src).unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), "<svg/>");
    assert!(import_picture(tmp.path(), "4", &src).is_err());
}

#[test]
fn test_memory_display_choices() {
    let expect = [
        ("percent", "{percentage}%"),
        ("used", "{used:0.1f} GB"),
        ("used-total", "{used:0.1f}/{total:0.0f} GB"),
        ("both", "{used:0.1f} GB · {percentage}%"),
    ];
    assert_eq!(memory_displays(Backend::Waybar).len(), MEMORY_DISPLAYS.len());
    assert_eq!(memory_displays(Backend::Mangobar), &[("percent", "Percent")]);
    for (display, fmt) in expect {
        let mut bar = BarConfig::default();
        bar.set("memory-display", display).unwrap();
        bar.backend = Backend::Waybar;
        let cfg = config_json(&bar);
        let got = cfg["memory"]["format"].as_str().unwrap();
        assert!(got.ends_with(fmt), "{}: {}", display, got);
        assert!(warnings(&bar).iter().all(|w| !w.contains("memory")));

        // Mangobar formats only the percentage ({})
        bar.backend = Backend::Mangobar;
        let cfg = config_json(&bar);
        assert_eq!(cfg["memory"]["format"], "󰘚 {}%");
        assert_eq!(
            warnings(&bar).iter().any(|w| w.contains("memory")),
            display != "percent"
        );
    }
    // the free-text format still wins
    let mut bar = BarConfig::default();
    bar.backend = Backend::Waybar;
    bar.set("memory-display", "both").unwrap();
    bar.set("memory.format", "RAM {percentage}").unwrap();
    assert_eq!(config_json(&bar)["memory"]["format"], "RAM {percentage}");
    assert!(BarConfig::default().set("memory-display", "gigs").is_err());
}

#[test]
fn test_font_size_bounds() {
    let mut bar = BarConfig::default();
    assert!(bar.set("font-size", "130").is_err());
    assert!(bar.set("font-size", "5").is_err());
    assert_eq!(bar.font_size, 13);
    bar.set("font-size", "72").unwrap();
    bar.set("font-size", "6").unwrap();
}

#[test]
fn test_user_presets_round_trip() {
    let tmp = tempdir().unwrap();
    let data = tmp.path();
    let mut mine = BarConfig::default();
    mine.font_size = 17;
    mine.fg_color = "#ff0000ff".into();
    mine.font = "Inter".into();
    mine.backend = Backend::Waybar;
    mine.output = vec!["DP-1".into()];
    mine.add_button(emoji_button(), "left").unwrap();

    let p = save_user_preset(data, "My Bar!", &mine).unwrap();
    assert_eq!(p.slug, "my-bar");
    assert!(data.join("bar-presets/my-bar.json").is_file());
    // names are unique, built-in names included
    assert!(save_user_preset(data, "my bar!", &mine).is_err());
    assert!(save_user_preset(data, "Islands", &mine).is_err());
    assert!(save_user_preset(data, "  ", &mine).is_err());

    let listed = list_user_presets(data);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "My Bar!");

    // applying keeps the per-machine settings
    let mut other = BarConfig::default();
    other.output = vec!["HDMI-A-1".into()];
    other.start_at_login = false;
    listed[0].apply(&mut other);
    assert_eq!(other.font_size, 17);
    assert_eq!(other.font, "Inter");
    assert_eq!(other.buttons.len(), 1);
    assert_eq!(other.backend, Backend::Mangobar);
    assert_eq!(other.output, vec!["HDMI-A-1".to_string()]);
    assert!(!other.start_at_login);
    assert_eq!(listed[0].applied_to(&other), other);

    // edited after applying: no longer the same
    other.font_size = 20;
    assert_ne!(listed[0].applied_to(&other), other);
    let p = overwrite_user_preset(data, "my-bar", &other).unwrap();
    assert_eq!(p.bar.font_size, 20);
    assert_eq!(list_user_presets(data)[0].applied_to(&other), other);

    rename_user_preset(data, "my-bar", "Evening").unwrap();
    assert_eq!(list_user_presets(data)[0].name, "Evening");

    // built-ins cannot be deleted; user presets can
    assert!(find_bar_preset("islands").is_some());
    assert!(delete_user_preset(data, "islands").is_err());
    delete_user_preset(data, "user:my-bar").unwrap();
    assert!(list_user_presets(data).is_empty());
    assert!(delete_user_preset(data, "user:my-bar").is_err());

    assert_eq!(slugify("  Dark / Neon 2 "), "dark-neon-2");
    assert_eq!(slugify("***"), "preset");
}

#[test]
fn test_update_live_writes_without_a_bar() {
    let tmp = tempdir().unwrap();
    // own output dirs: never the user's config
    std::env::set_var("MANGOBAR_MOD_DIR", tmp.path().join("mangobar"));
    let bar = BarConfig::default();
    assert_eq!(
        update_live(&bar, &tmp.path().join("data"), false).unwrap(),
        LiveUpdate::Written
    );
    assert!(tmp.path().join("mangobar/config.jsonc").is_file());
    // same files again: nothing to do
    assert_eq!(
        update_live(&bar, &tmp.path().join("data"), false).unwrap(),
        LiveUpdate::Unchanged
    );
}
