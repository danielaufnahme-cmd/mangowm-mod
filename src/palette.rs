//! Colours from a wallpaper: a small palette (background, surface, text,
//! accents, urgent, muted) and its mapping onto Mango's window borders and
//! the bar's colours.
//!
//! The picture is shrunk to a few thousand pixels, quantised in OKLab
//! (median cut, then a few k-means rounds) and each role is picked from the
//! clusters and moved to a fixed lightness range, so the result always has
//! readable contrast. Same picture in, same palette out.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::bar::generate::write_bar_files;
use crate::bar::model::BarConfig;
use crate::bar::process::{our_bar_running, start_bar, update_live, LiveUpdate};
use crate::core::fsutil::{atomic_write, read_text};
use crate::core::manager::ConfigManager;

/// The picture is scaled to fit this square before quantising.
const SAMPLE_SIZE: u32 = 128;
const CLUSTERS: usize = 8;
const KMEANS_ROUNDS: usize = 8;

/// Clusters below this OKLab chroma count as grey.
const GREY_CHROMA: f32 = 0.035;
/// Below this average chroma (and with no colourful spots) a picture is grey.
const NO_HUE_CHROMA: f32 = 0.006;
/// The most colourful share of the pixels, clustered apart as accent spots...
const VIVID_SHARE: f32 = 0.04;
const SPOT_CLUSTERS: usize = 3;
/// ...if they have at least this chroma.
const SPOT_CHROMA: f32 = 0.025;

/// Minimum WCAG contrast against the background.
pub const MIN_TEXT_CONTRAST: f64 = 4.5;
pub const MIN_ACCENT_CONTRAST: f64 = 4.5;
pub const MIN_MUTED_CONTRAST: f64 = 3.0;

/// OKLCh hue of a clean red (sRGB #e0..., about 25-29 degrees).
const URGENT_HUE: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `#rrggbbaa`.
    pub fn hex_alpha(self, alpha: u8) -> String {
        format!("{}{:02x}", self.hex(), alpha)
    }

    /// WCAG relative luminance.
    pub fn luminance(self) -> f64 {
        let ch = |c: u8| srgb_to_linear(c as f32) as f64;
        0.2126 * ch(self.r) + 0.7152 * ch(self.g) + 0.0722 * ch(self.b)
    }

    /// `self` blended towards `other` by `t` (0 = self, 1 = other).
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(m(self.r, other.r), m(self.g, other.g), m(self.b, other.b))
    }
}

/// WCAG contrast ratio (1-21).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    /// Dark base: bar background, Mango's root colour.
    pub background: Rgb,
    /// A step lighter than the background: modules, unfocused borders.
    pub surface: Rgb,
    pub text: Rgb,
    /// Main accent: focused border, active workspace.
    pub accent: Rgb,
    pub secondary: Rgb,
    pub urgent: Rgb,
    /// Inactive text.
    pub muted: Rgb,
    /// True when the picture had no real colour and a grey palette was used.
    pub neutral: bool,
}

impl Palette {
    /// (role name, colour) in display order.
    pub fn roles(&self) -> [(&'static str, Rgb); 7] {
        [
            ("Background", self.background),
            ("Surface", self.surface),
            ("Text", self.text),
            ("Accent", self.accent),
            ("Secondary", self.secondary),
            ("Urgent", self.urgent),
            ("Muted", self.muted),
        ]
    }

    /// Unfocused window border: the surface, lifted towards the muted colour.
    pub fn border(&self) -> Rgb {
        self.surface.mix(self.muted, 0.35)
    }
}

// ---- colour spaces (OKLab, https://bottosson.github.io/posts/oklab/) ----

fn srgb_to_linear(c: f32) -> f32 {
    let c = c / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    c * 255.0
}

/// OKLab [L, a, b] of an sRGB pixel.
fn to_lab(rgb: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|c| srgb_to_linear(c as f32));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Linear sRGB of an OKLab colour (may be outside 0-1).
fn lab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

#[derive(Debug, Clone, Copy)]
struct Lch {
    l: f32,
    c: f32,
    /// Degrees, 0-360.
    h: f32,
}

impl Lch {
    fn new(l: f32, c: f32, h: f32) -> Self {
        Self { l, c, h }
    }

    fn from_lab([l, a, b]: [f32; 3]) -> Self {
        let h = b.atan2(a).to_degrees();
        Self::new(l, a.hypot(b), if h < 0.0 { h + 360.0 } else { h })
    }

    fn lab(self) -> [f32; 3] {
        let h = self.h.to_radians();
        [self.l, self.c * h.cos(), self.c * h.sin()]
    }

    fn in_gamut(self) -> bool {
        lab_to_linear(self.lab())
            .iter()
            .all(|v| (-0.0005..=1.0005).contains(v))
    }

    /// Nearest sRGB colour, keeping lightness and hue and lowering chroma.
    fn to_rgb(self) -> Rgb {
        let mut fit = self;
        if !fit.in_gamut() {
            let (mut lo, mut hi) = (0.0, self.c);
            for _ in 0..14 {
                let mid = (lo + hi) / 2.0;
                if Lch::new(self.l, mid, self.h).in_gamut() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            fit.c = lo;
        }
        let [r, g, b] = lab_to_linear(fit.lab())
            .map(|v| linear_to_srgb(v.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8);
        Rgb::new(r, g, b)
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

// ---- quantising ----

#[derive(Debug, Clone, Copy)]
struct Cluster {
    lab: [f32; 3],
    /// Share of the picture, 0-1.
    weight: f32,
}

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn mean(samples: &[[f32; 3]]) -> [f32; 3] {
    let mut sum = [0.0f64; 3];
    for s in samples {
        for i in 0..3 {
            sum[i] += s[i] as f64;
        }
    }
    let n = samples.len().max(1) as f64;
    sum.map(|v| (v / n) as f32)
}

/// Widest axis of a box of samples and its range.
fn widest_axis(samples: &[[f32; 3]]) -> (usize, f32) {
    (0..3)
        .map(|i| {
            let (lo, hi) = samples
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), s| (lo.min(s[i]), hi.max(s[i])));
            // L spans 0-1, a/b only about +-0.3: weigh them up to compare
            (i, (hi - lo) * if i == 0 { 1.0 } else { 2.0 })
        })
        .fold((0, f32::MIN), |best, cur| if cur.1 > best.1 { cur } else { best })
}

/// Median cut into `k` boxes, then k-means rounds from the box means.
fn quantise(mut samples: Vec<[f32; 3]>, k: usize) -> Vec<Cluster> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut boxes: Vec<Vec<[f32; 3]>> = vec![std::mem::take(&mut samples)];
    while boxes.len() < k {
        let pick = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| (i, widest_axis(b).1 * (b.len() as f32).sqrt()))
            .filter(|(_, score)| *score > 0.0)
            .fold(None, |best: Option<(usize, f32)>, cur| match best {
                Some(b) if b.1 >= cur.1 => Some(b),
                _ => Some(cur),
            });
        let Some((i, _)) = pick else { break };
        let mut b = boxes.swap_remove(i);
        let axis = widest_axis(&b).0;
        b.sort_by(|x, y| x[axis].total_cmp(&y[axis]));
        let upper = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(upper);
    }

    let all: Vec<[f32; 3]> = boxes.concat();
    let mut centres: Vec<[f32; 3]> = boxes.iter().map(|b| mean(b)).collect();
    let mut assign = vec![0usize; all.len()];
    for _ in 0..KMEANS_ROUNDS {
        for (s, a) in all.iter().zip(assign.iter_mut()) {
            *a = (0..centres.len())
                .min_by(|&x, &y| dist2(s, &centres[x]).total_cmp(&dist2(s, &centres[y])))
                .unwrap_or(0);
        }
        let mut sums = vec![[0.0f64; 4]; centres.len()];
        for (s, &a) in all.iter().zip(&assign) {
            for i in 0..3 {
                sums[a][i] += s[i] as f64;
            }
            sums[a][3] += 1.0;
        }
        for (c, s) in centres.iter_mut().zip(&sums) {
            if s[3] > 0.0 {
                *c = [s[0], s[1], s[2]].map(|v| (v / s[3]) as f32);
            }
        }
    }
    let mut counts = vec![0usize; centres.len()];
    for &a in &assign {
        counts[a] += 1;
    }
    let total = all.len() as f32;
    centres
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|(lab, n)| Cluster {
            lab,
            weight: n as f32 / total,
        })
        .collect()
}

// ---- roles ----

/// Raise the lightness of `c` until it has `min` contrast on `bg`.
fn lift_to_contrast(mut c: Lch, bg: Rgb, min: f64) -> Rgb {
    loop {
        let rgb = c.to_rgb();
        if contrast(rgb, bg) >= min || c.l >= 1.0 {
            return rgb;
        }
        c.l = (c.l + 0.01).min(1.0);
    }
}

/// Palette for the grey/monochrome case (and an empty picture).
fn neutral_palette(base_l: f32) -> Palette {
    // keep a very bright or very dark picture's mood a little
    let bg_l = (0.16 + (base_l - 0.5) * 0.06).clamp(0.15, 0.22);
    let grey = |l: f32| Lch::new(l, 0.0, 0.0);
    finish(
        grey(bg_l),
        grey(bg_l + 0.07),
        grey(0.94),
        grey(0.84),
        grey(0.68),
        Lch::new(0.66, 0.15, URGENT_HUE),
        grey(0.64),
        true,
    )
}

/// Turn the role colours into sRGB with the contrast guarantees.
#[allow(clippy::too_many_arguments)]
fn finish(
    bg: Lch,
    surface: Lch,
    text: Lch,
    accent: Lch,
    secondary: Lch,
    urgent: Lch,
    muted: Lch,
    neutral: bool,
) -> Palette {
    let background = bg.to_rgb();
    Palette {
        background,
        surface: surface.to_rgb(),
        text: lift_to_contrast(text, background, MIN_TEXT_CONTRAST + 2.5),
        accent: lift_to_contrast(accent, background, MIN_ACCENT_CONTRAST),
        secondary: lift_to_contrast(secondary, background, MIN_MUTED_CONTRAST),
        urgent: lift_to_contrast(urgent, background, MIN_ACCENT_CONTRAST),
        muted: lift_to_contrast(muted, background, MIN_MUTED_CONTRAST),
        neutral,
    }
}

/// How much a cluster stands out as an accent: colourful, and not a speck.
fn accent_score(c: &Lch, weight: f32) -> f32 {
    let light = if c.l < 0.25 { 0.4 } else { 1.0 };
    c.c.min(0.2) * weight.powf(0.4) * light
}

fn chroma(lab: &[f32; 3]) -> f32 {
    lab[1].hypot(lab[2])
}

/// `clusters` cover the whole picture; `spots` are its most colourful pixels
/// clustered on their own (weights are shares of the whole picture).
fn palette_from_clusters(clusters: &[Cluster], spots: &[Cluster]) -> Palette {
    let lch: Vec<(Lch, f32)> = clusters
        .iter()
        .map(|c| (Lch::from_lab(c.lab), c.weight))
        .collect();
    let avg_l = lch.iter().map(|(c, w)| c.l * w).sum::<f32>();
    let avg_c = lch.iter().map(|(c, w)| c.c * w).sum::<f32>();
    if lch.is_empty() || (avg_c < NO_HUE_CHROMA && spots.is_empty()) {
        return neutral_palette(avg_l);
    }
    // the picture's overall hue (weighted mean of a/b)
    let overall = Lch::from_lab([
        avg_l,
        clusters.iter().map(|c| c.lab[1] * c.weight).sum(),
        clusters.iter().map(|c| c.lab[2] * c.weight).sum(),
    ]);

    let mut candidates: Vec<(Lch, f32)> = lch
        .iter()
        .copied()
        .filter(|(c, w)| c.c >= GREY_CHROMA && *w >= 0.01 && c.l > 0.12)
        .chain(spots.iter().map(|s| (Lch::from_lab(s.lab), s.weight)))
        .collect();
    if candidates.is_empty() {
        // only a faint cast (misty or dark photos): its hue is the accent
        candidates.push((overall, 1.0));
    }

    // background hue: the biggest dark area, else the overall cast
    let (base, _) = lch
        .iter()
        .copied()
        .max_by(|a, b| (a.1 * (1.2 - a.0.l)).total_cmp(&(b.1 * (1.2 - b.0.l))))
        .unwrap_or(candidates[0]);
    let (tint_h, tint_c) = if base.c >= NO_HUE_CHROMA {
        (base.h, (base.c * 1.5).min(0.03))
    } else {
        (overall.h, overall.c.min(0.015))
    };

    candidates.sort_by(|a, b| accent_score(&b.0, b.1).total_cmp(&accent_score(&a.0, a.1)));
    let primary = candidates[0].0;
    let second = candidates
        .iter()
        .skip(1)
        .map(|v| v.0)
        .find(|c| hue_distance(c.h, primary.h) >= 30.0)
        // else the picture's main tint, if it is a different colour
        .or_else(|| {
            (tint_c >= NO_HUE_CHROMA && hue_distance(tint_h, primary.h) >= 30.0)
                .then(|| Lch::new(0.7, 0.06, tint_h))
        });

    let accent = Lch::new(primary.l.clamp(0.72, 0.82), primary.c.clamp(0.09, 0.15), primary.h);
    let secondary = match second {
        Some(s) => Lch::new(s.l.clamp(0.64, 0.76), s.c.clamp(0.06, 0.13), s.h),
        // one-colour picture: a calmer, darker shade of the accent
        None => Lch::new(0.64, accent.c * 0.7, accent.h),
    };
    // a red that sits next to the accent; darker when the accent is red too
    let urgent_l = if hue_distance(accent.h, URGENT_HUE) < 35.0 {
        0.6
    } else {
        accent.l.clamp(0.62, 0.68)
    };
    let urgent = Lch::new(urgent_l, accent.c.clamp(0.13, 0.17), URGENT_HUE);

    finish(
        Lch::new(0.2, tint_c, tint_h),
        Lch::new(0.27, tint_c * 1.15, tint_h),
        Lch::new(0.93, tint_c.min(0.015), tint_h),
        accent,
        secondary,
        urgent,
        Lch::new(0.63, tint_c.min(0.03), tint_h),
        false,
    )
}

/// Palette of a decoded picture.
pub fn palette_from_image(img: &image::DynamicImage) -> Palette {
    let small = img.thumbnail(SAMPLE_SIZE, SAMPLE_SIZE).to_rgba8();
    let samples: Vec<[f32; 3]> = small
        .pixels()
        .filter(|p| p.0[3] >= 128)
        .map(|p| to_lab([p.0[0], p.0[1], p.0[2]]))
        .collect();
    let total = samples.len().max(1) as f32;

    // the most colourful few percent on their own, so a small bright spot
    // (a lit window, a flower) can still become the accent
    let mut vivid: Vec<[f32; 3]> = samples.iter().filter(|s| s[0] > 0.2).copied().collect();
    vivid.sort_by(|a, b| chroma(b).total_cmp(&chroma(a)));
    vivid.truncate((total * VIVID_SHARE).ceil() as usize);
    vivid.retain(|s| chroma(s) >= SPOT_CHROMA);
    let share = vivid.len() as f32 / total;
    let spots: Vec<Cluster> = quantise(vivid, SPOT_CLUSTERS)
        .into_iter()
        .map(|c| Cluster {
            weight: c.weight * share,
            ..c
        })
        .collect();

    palette_from_clusters(&quantise(samples, CLUSTERS), &spots)
}

/// Palette of the picture file at `path` (PNG, JPEG or WebP).
pub fn palette_from_file(path: &Path) -> Result<Palette> {
    let img = image::ImageReader::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("failed to read the picture {}", path.display()))?;
    Ok(palette_from_image(&img))
}

// ---- window borders ----

/// Mango border settings for a palette, as `#rrggbbaa`.
pub fn window_colors(p: &Palette) -> Vec<(&'static str, String)> {
    vec![
        ("focus_color", p.accent.hex_alpha(0xff)),
        ("border_color", p.border().hex_alpha(0xff)),
        ("urgent_color", p.urgent.hex_alpha(0xff)),
        ("root_color", p.background.hex_alpha(0xff)),
        ("scratchpad_color", p.secondary.hex_alpha(0xff)),
        ("maximized_screen_color", p.accent.mix(p.text, 0.35).hex_alpha(0xff)),
        ("global_color", p.secondary.mix(p.text, 0.3).hex_alpha(0xff)),
        ("overlay_color", p.accent.mix(p.secondary, 0.5).hex_alpha(0xff)),
        ("drop_color", p.accent.hex_alpha(0x80)),
        ("split_color", p.secondary.hex_alpha(0xff)),
    ]
}

/// Stage the window border colours (written by Apply).
pub fn stage_window_colors(mgr: &mut ConfigManager, p: &Palette) -> Result<()> {
    for (key, value) in window_colors(p) {
        mgr.set(key, &value)?;
    }
    Ok(())
}

// ---- bar ----

/// Alpha byte of a `#rrggbbaa` colour; opaque for anything else.
fn alpha_of(color: &str) -> u8 {
    color
        .strip_prefix('#')
        .filter(|h| h.len() == 8)
        .and_then(|h| u8::from_str_radix(&h[6..], 16).ok())
        .unwrap_or(0xff)
}

/// Set the bar's colour fields from `p`. The bar's own transparency (of the
/// background and the border) is kept; everything else is untouched.
pub fn bar_colors(p: &Palette, bar: &mut BarConfig) {
    let bg_alpha = alpha_of(&bar.bg_color);
    let border_alpha = alpha_of(&bar.border_color);
    bar.bg_color = p.background.hex_alpha(bg_alpha);
    bar.fg_color = p.text.hex_alpha(0xff);
    bar.accent_color = p.accent.hex_alpha(0xff);
    bar.module_bg_color = p.surface.hex_alpha(bg_alpha);
    bar.hover_color = p.surface.mix(p.accent, 0.25).hex_alpha(bg_alpha);
    bar.urgent_color = p.urgent.hex_alpha(0xff);
    bar.inactive_color = p.muted.hex_alpha(0xff);
    bar.border_color = p.border().hex_alpha(border_alpha);
}

/// Put `p` into the saved bar settings (`<data_dir>/bar.json`) only; no bar
/// files are written.
pub fn save_bar_colors(p: &Palette, data_dir: &Path) -> Result<BarConfig> {
    let path = BarConfig::default_path(data_dir);
    let mut bar = BarConfig::load(&path)?;
    bar_colors(p, &mut bar);
    bar.save(&path)?;
    Ok(bar)
}

/// Put `p` into the saved bar settings (`<data_dir>/bar.json`) and write the
/// bar files into `dir` (default: the bar's config folder). Does not restart it.
pub fn apply_bar_colors(p: &Palette, data_dir: &Path, dir: Option<&Path>) -> Result<BarConfig> {
    let bar = save_bar_colors(p, data_dir)?;
    write_bar_files(&bar, dir)?;
    Ok(bar)
}

/// Save the colours, write the bar files and show them in the running bar,
/// like an edit on the Bar page ([`update_live`]). Writes the real bar
/// folder; may block while Mangobar restarts, so call it off the UI thread.
pub fn apply_bar_colors_live(p: &Palette, data_dir: &Path) -> Result<LiveUpdate> {
    let bar = save_bar_colors(p, data_dir)?;
    update_live(&bar, data_dir, true)
}

/// Restart the bar with new files, but only when ours is running already.
/// Returns the new PID.
pub fn restart_bar_if_running(
    bar: &BarConfig,
    dir: Option<&Path>,
    data_dir: &Path,
) -> Result<Option<u32>> {
    if !our_bar_running(data_dir) {
        return Ok(None);
    }
    start_bar(bar, dir, data_dir).map(Some)
}

// ---- settings ----

/// The Wallpaper page's "Match colours" choices (`<data_dir>/wallpaper-colors.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSettings {
    #[serde(default = "yes")]
    pub windows: bool,
    #[serde(default = "yes")]
    pub bar: bool,
    /// Apply the matched colours whenever a wallpaper is set on the page.
    #[serde(default)]
    pub auto: bool,
}

fn yes() -> bool {
    true
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            windows: true,
            bar: true,
            auto: false,
        }
    }
}

impl MatchSettings {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("wallpaper-colors.json")
    }

    /// Saved choices; defaults when there are none (or the file is broken).
    pub fn load(data_dir: &Path) -> Self {
        read_text(Self::path(data_dir))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        atomic_write(Self::path(data_dir), &(serde_json::to_string_pretty(self)? + "\n"))
    }
}
