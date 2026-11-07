use image::{DynamicImage, Rgb as Px, RgbImage, Rgba, RgbaImage};
use mangowm_mod::bar::model::BarConfig;
use mangowm_mod::core::manager::ConfigManager;
use mangowm_mod::palette::{
    apply_bar_colors, bar_colors, contrast, palette_from_file, palette_from_image, save_bar_colors,
    stage_window_colors, window_colors, MatchSettings, Palette, Rgb, MIN_ACCENT_CONTRAST,
    MIN_MUTED_CONTRAST, MIN_TEXT_CONTRAST,
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

/// Dark teal picture with an orange spot in the middle.
fn teal_with_orange() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(400, 240, |x, y| {
        if (170..230).contains(&x) && (100..140).contains(&y) {
            Px([235, 130, 40])
        } else {
            Px([10, (40 + y / 8) as u8, (45 + y / 8) as u8])
        }
    }))
}

fn grey_gradient() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(300, 200, |x, _| {
        let v = (x * 255 / 299) as u8;
        Px([v, v, v])
    }))
}

/// Small deterministic noise generator (no rand dependency).
fn noise(seed: u64) -> DynamicImage {
    let mut s = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = move || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as u8
    };
    let base = [next(), next(), next()];
    DynamicImage::ImageRgb8(RgbImage::from_fn(120, 80, |_, _| {
        let n = next();
        Px([
            base[0].wrapping_add(n / 3),
            base[1].wrapping_add(n / 5),
            base[2],
        ])
    }))
}

fn assert_readable(p: &Palette, what: &str) {
    let bg = p.background;
    assert!(
        contrast(p.text, bg) >= MIN_TEXT_CONTRAST,
        "{}: text {} on {} = {:.2}",
        what,
        p.text.hex(),
        bg.hex(),
        contrast(p.text, bg)
    );
    assert!(
        contrast(p.text, p.surface) >= MIN_TEXT_CONTRAST,
        "{}: text on surface",
        what
    );
    assert!(
        contrast(p.accent, bg) >= MIN_ACCENT_CONTRAST,
        "{}: accent {} on {} = {:.2}",
        what,
        p.accent.hex(),
        bg.hex(),
        contrast(p.accent, bg)
    );
    assert!(
        contrast(p.urgent, bg) >= MIN_ACCENT_CONTRAST,
        "{}: urgent",
        what
    );
    assert!(
        contrast(p.muted, bg) >= MIN_MUTED_CONTRAST,
        "{}: muted",
        what
    );
    assert!(
        contrast(p.secondary, bg) >= MIN_MUTED_CONTRAST,
        "{}: secondary",
        what
    );
    // a dark base: the bar and root colour stay calm
    assert!(
        bg.luminance() < 0.05,
        "{}: background {} too light",
        what,
        bg.hex()
    );
    assert!(p.surface.luminance() > bg.luminance(), "{}: surface", what);
}

fn is_grey(c: Rgb) -> bool {
    let (lo, hi) = (c.r.min(c.g).min(c.b), c.r.max(c.g).max(c.b));
    hi - lo <= 2
}

#[test]
fn test_palette_takes_accent_from_picture() {
    let p = palette_from_image(&teal_with_orange());
    assert!(!p.neutral);
    assert_readable(&p, "teal/orange");
    // the orange spot is the accent: red > green > blue
    let a = p.accent;
    assert!(a.r > a.g && a.g > a.b, "accent {} is not orange", a.hex());
    // the teal area tints the background: blue/green over red
    let bg = p.background;
    assert!(
        bg.g > bg.r && bg.b > bg.r,
        "background {} is not teal",
        bg.hex()
    );
    // urgent is a red
    let u = p.urgent;
    assert!(
        u.r > u.g + 40 && u.r > u.b + 40,
        "urgent {} is not red",
        u.hex()
    );
}

#[test]
fn test_grey_picture_gives_neutral_palette() {
    let p = palette_from_image(&grey_gradient());
    assert!(p.neutral);
    assert_readable(&p, "grey");
    for (name, c) in p.roles() {
        if name != "Urgent" {
            assert!(is_grey(c), "{} {} is not grey", name, c.hex());
        }
    }
}

#[test]
fn test_flat_and_extreme_pictures() {
    for (name, px) in [
        ("black", [0, 0, 0]),
        ("white", [255, 255, 255]),
        ("mid grey", [128, 128, 128]),
        ("pure red", [255, 0, 0]),
        ("pure blue", [0, 0, 255]),
        ("dark green", [5, 30, 8]),
    ] {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, Px(px)));
        let p = palette_from_image(&img);
        assert_readable(&p, name);
    }
    // fully transparent: nothing to sample, still a usable palette
    let clear = DynamicImage::ImageRgba8(RgbaImage::from_pixel(32, 32, Rgba([255, 0, 0, 0])));
    let p = palette_from_image(&clear);
    assert!(p.neutral);
    assert_readable(&p, "transparent");
}

#[test]
fn test_noise_pictures_keep_contrast() {
    for seed in 0..40 {
        let p = palette_from_image(&noise(seed));
        assert_readable(&p, &format!("noise {}", seed));
    }
}

#[test]
fn test_palette_is_deterministic() {
    let img = teal_with_orange();
    assert_eq!(palette_from_image(&img), palette_from_image(&img));
}

#[test]
fn test_palette_from_file_png_and_jpeg() {
    let tmp = tempdir().unwrap();
    let png = tmp.path().join("w.png");
    let jpg = tmp.path().join("w.jpg");
    let img = teal_with_orange();
    img.save(&png).unwrap();
    img.save(&jpg).unwrap();
    let a = palette_from_file(&png).unwrap();
    let b = palette_from_file(&jpg).unwrap();
    assert_readable(&a, "png");
    assert_readable(&b, "jpeg");
    assert!(b.accent.r > b.accent.b, "jpeg accent {}", b.accent.hex());
    assert!(palette_from_file(&tmp.path().join("missing.png")).is_err());
    fs::write(tmp.path().join("bad.png"), "not a picture").unwrap();
    assert!(palette_from_file(&tmp.path().join("bad.png")).is_err());
}

#[test]
fn test_palette_speed_4k() {
    // timing only means something in an optimised build (`cargo test --release`)
    if cfg!(debug_assertions) {
        return;
    }
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(3840, 2160, |x, y| {
        Px([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    }));
    let t = std::time::Instant::now();
    let p = palette_from_image(&img);
    let ms = t.elapsed().as_millis();
    assert_readable(&p, "4k");
    assert!(ms < 300, "4K palette took {} ms", ms);
}

#[test]
fn test_window_colors_staged_and_applied() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.conf"), "border_px=2\n").unwrap();
    let mut mgr = manager(tmp.path());
    let p = palette_from_image(&teal_with_orange());

    stage_window_colors(&mut mgr, &p).unwrap();
    let accent = format!("0x{}ff", &p.accent.hex()[1..]);
    assert_eq!(mgr.get("focus_color"), Some(accent.clone()));
    assert_eq!(
        mgr.get("border_color"),
        Some(format!("0x{}ff", &p.border().hex()[1..]))
    );
    assert_eq!(
        mgr.get("root_color"),
        Some(format!("0x{}ff", &p.background.hex()[1..]))
    );
    assert_eq!(
        mgr.get("drop_color"),
        Some(format!("0x{}80", &p.accent.hex()[1..]))
    );
    // nothing is written before Apply
    assert!(!tmp.path().join("mango-mod.conf").exists());

    let res = mgr.apply().unwrap();
    assert!(res.errors.is_empty(), "{:?}", res.errors);
    let written = fs::read_to_string(tmp.path().join("mango-mod.conf")).unwrap();
    assert!(
        written.contains(&format!("focus_color={}", accent)),
        "{}",
        written
    );
    for (key, _) in window_colors(&p) {
        assert!(written.contains(&format!("{}=0x", key)), "{} missing", key);
    }
    // the unfocused border is clearly apart from the focused one
    assert!(contrast(p.accent, p.border()) >= 2.0);
}

#[test]
fn test_bar_colors_saved_and_files_written() {
    let tmp = tempdir().unwrap();
    let data = tmp.path().join("data");
    let out = tmp.path().join("bar");
    let path = BarConfig::default_path(&data);

    let mut bar = BarConfig {
        bg_color: "#11223380".into(),
        border_color: "#ffffff22".into(),
        font: "Monaspice".into(),
        height: 40,
        ..BarConfig::default()
    };
    bar.modules.left = vec!["workspaces".into()];
    fs::create_dir_all(&data).unwrap();
    bar.save(&path).unwrap();

    let p = palette_from_image(&teal_with_orange());
    let saved = apply_bar_colors(&p, &data, Some(&out)).unwrap();
    let loaded = BarConfig::load(&path).unwrap();
    assert_eq!(saved, loaded);

    // colours from the palette; the bar's own transparency is kept
    assert_eq!(loaded.bg_color, p.background.hex_alpha(0x80));
    assert_eq!(loaded.border_color, p.border().hex_alpha(0x22));
    assert_eq!(loaded.fg_color, p.text.hex_alpha(0xff));
    assert_eq!(loaded.accent_color, p.accent.hex_alpha(0xff));
    assert_eq!(loaded.urgent_color, p.urgent.hex_alpha(0xff));
    assert_eq!(loaded.inactive_color, p.muted.hex_alpha(0xff));
    assert_eq!(loaded.module_bg_color, p.surface.hex_alpha(0x80));
    // everything else untouched
    assert_eq!(loaded.font, "Monaspice");
    assert_eq!(loaded.height, 40);
    assert_eq!(loaded.modules.left, vec!["workspaces".to_string()]);
    loaded.validate().unwrap();

    let style = fs::read_to_string(out.join("style.css")).unwrap();
    let accent = p.accent.hex();
    assert!(
        style.to_lowercase().contains(&accent[1..])
            || style.contains(&format!("{}, {}, {}", p.accent.r, p.accent.g, p.accent.b)),
        "accent {} not in style.css:\n{}",
        accent,
        style
    );
    assert!(out.join("config.jsonc").is_file());
}

#[test]
fn test_bar_colors_without_saved_bar() {
    let tmp = tempdir().unwrap();
    let p = palette_from_image(&grey_gradient());
    let mut bar = BarConfig::default();
    let default_bg_alpha = &BarConfig::default().bg_color[7..];
    bar_colors(&p, &mut bar);
    assert!(bar.bg_color.ends_with(default_bg_alpha));
    bar.validate().unwrap();

    // no bar.json yet: defaults plus the palette
    let saved = apply_bar_colors(&p, tmp.path(), Some(&tmp.path().join("out"))).unwrap();
    assert_eq!(saved.accent_color, p.accent.hex_alpha(0xff));
    assert!(BarConfig::default_path(tmp.path()).is_file());
}

/// Only the settings are saved; the live path (`apply_bar_colors_live`) is
/// not tested because it writes the real bar folder and may restart the real bar.
#[test]
fn test_save_bar_colors_writes_only_bar_json() {
    let tmp = tempdir().unwrap();
    let p = palette_from_image(&teal_with_orange());
    let saved = save_bar_colors(&p, tmp.path()).unwrap();
    assert_eq!(saved.accent_color, p.accent.hex_alpha(0xff));
    let again = BarConfig::load(&BarConfig::default_path(tmp.path())).unwrap();
    assert_eq!(again.accent_color, saved.accent_color);
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn test_match_settings_round_trip() {
    let tmp = tempdir().unwrap();
    assert_eq!(MatchSettings::load(tmp.path()), MatchSettings::default());
    let s = MatchSettings {
        windows: false,
        bar: true,
        auto: true,
    };
    s.save(tmp.path()).unwrap();
    assert_eq!(MatchSettings::load(tmp.path()), s);
    fs::write(MatchSettings::path(tmp.path()), "{broken").unwrap();
    assert_eq!(MatchSettings::load(tmp.path()), MatchSettings::default());
}
