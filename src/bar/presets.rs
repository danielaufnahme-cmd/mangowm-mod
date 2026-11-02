//! Ready-made bar looks. A preset sets the layout, sizes, radii, module set
//! and colors; the program, monitors, layer, font family, login start and
//! module options stay as they are.
//!
//! Colors: neutral dark greys with one restrained accent (teal #76ccb1).

use super::model::{module_base, BarConfig};

#[derive(Debug, Clone, Copy)]
pub struct BarPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    bg: &'static str,
    fg: &'static str,
    accent: &'static str,
    module_bg: &'static str,
    hover: &'static str,
    urgent: &'static str,
    inactive: &'static str,
    border_color: &'static str,
    border_width: u32,
    /// -1 = pill.
    radius: i32,
    module_radius: i32,
    margin_top: u32,
    margin_side: u32,
    module_padding: u32,
    module_margin: u32,
    module_gap: u32,
    font_size: u32,
    font_weight: &'static str,
    title_length: u32,
    tag_fill: bool,
    left: &'static [&'static str],
    center: &'static [&'static str],
    right: &'static [&'static str],
}

const FULL_LEFT: &[&str] = &["workspaces", "layout", "window"];
const FULL_RIGHT: &[&str] = &["cpu", "memory", "pulseaudio", "network", "battery", "tray"];

/// Modules a preset keeps when the bar already has them (hardware or the
/// user's own); they go to the right section, before the tray.
fn kept_module(name: &str) -> bool {
    name.starts_with("custom/")
        || name.contains('#') && module_base(name) == "battery"
        || matches!(
            name,
            "backlight" | "keyboardlayout" | "keymode" | "hideclients"
        )
}

pub const BAR_PRESETS: &[BarPreset] = &[
    BarPreset {
        id: "islands",
        name: "Islands",
        description: "Each module a separate rounded block on a clear bar. Looks the same in \
                      Mangobar and Waybar",
        bg: "#00000000",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#16181ce6",
        hover: "#2a2e35ff",
        urgent: "#e5737aff",
        inactive: "#7c828cff",
        border_color: "#ffffff14",
        border_width: 0,
        radius: 14,
        module_radius: 10,
        margin_top: 6,
        margin_side: 8,
        module_padding: 12,
        module_margin: 0,
        module_gap: 3,
        font_size: 13,
        font_weight: "medium",
        title_length: 48,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "floating-pill",
        name: "Floating Pill",
        description: "One rounded, floating bar with flat modules. Mangobar draws it with \
                      square ends",
        bg: "#16181cf0",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff14",
        urgent: "#e5737aff",
        inactive: "#7c828cff",
        border_color: "#ffffff12",
        border_width: 1,
        radius: -1,
        module_radius: -1,
        margin_top: 8,
        margin_side: 12,
        module_padding: 10,
        module_margin: 4,
        module_gap: 2,
        font_size: 13,
        font_weight: "medium",
        title_length: 56,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "solid-dock",
        name: "Solid Dock",
        description: "Opaque, full width, edge to edge, with a solid accent block on the \
                      active tag",
        bg: "#111316ff",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff12",
        urgent: "#e5737aff",
        inactive: "#6f757eff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 12,
        module_margin: 0,
        module_gap: 0,
        font_size: 13,
        font_weight: "medium",
        title_length: 64,
        tag_fill: true,
        left: FULL_LEFT,
        center: &["clock"],
        right: FULL_RIGHT,
    },
    BarPreset {
        id: "minimal",
        name: "Minimal",
        description: "Flat and see-through, text only: the active tag is accent text",
        bg: "#0d0e11a6",
        fg: "#dfe2e6ff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff10",
        urgent: "#e5737aff",
        inactive: "#6b7079ff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 10,
        module_margin: 0,
        module_gap: 0,
        font_size: 13,
        font_weight: "normal",
        title_length: 48,
        tag_fill: false,
        left: &["workspaces", "window"],
        center: &["clock"],
        right: &["pulseaudio", "network", "battery", "tray"],
    },
    BarPreset {
        id: "compact",
        name: "Compact",
        description: "Thin full-width bar with a smaller font; the clock sits on the right",
        bg: "#16181cf2",
        fg: "#e6e8ebff",
        accent: "#76ccb1ff",
        module_bg: "#00000000",
        hover: "#ffffff12",
        urgent: "#e5737aff",
        inactive: "#6f757eff",
        border_color: "#ffffff0f",
        border_width: 0,
        radius: 0,
        module_radius: 0,
        margin_top: 0,
        margin_side: 0,
        module_padding: 7,
        module_margin: 0,
        module_gap: 0,
        font_size: 12,
        font_weight: "medium",
        title_length: 40,
        tag_fill: true,
        left: FULL_LEFT,
        center: &[],
        right: &["cpu", "memory", "pulseaudio", "network", "battery", "clock", "tray"],
    },
];

/// The look of a fresh bar.json.
pub const DEFAULT_BAR_PRESET: &str = "islands";

pub fn find_bar_preset(id: &str) -> Option<&'static BarPreset> {
    BAR_PRESETS.iter().find(|p| p.id == id)
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl BarPreset {
    /// Set the preset's look on `bar`. The height becomes automatic.
    pub fn apply(&self, bar: &mut BarConfig) {
        bar.bg_color = self.bg.to_string();
        bar.fg_color = self.fg.to_string();
        bar.accent_color = self.accent.to_string();
        bar.module_bg_color = self.module_bg.to_string();
        bar.hover_color = self.hover.to_string();
        bar.urgent_color = self.urgent.to_string();
        bar.inactive_color = self.inactive.to_string();
        bar.border_color = self.border_color.to_string();
        bar.border_width = self.border_width;
        bar.radius = self.radius;
        bar.module_radius = self.module_radius;
        // a bottom Waybar floats the same distance above the screen edge
        let (top, bottom) = if bar.position == "bottom" {
            (0, self.margin_top)
        } else {
            (self.margin_top, 0)
        };
        bar.margin_top = top;
        bar.margin_bottom = bottom;
        bar.margin_left = self.margin_side;
        bar.margin_right = self.margin_side;
        // the gap between modules is their side margin (Mangobar has no spacing)
        bar.spacing = 0;
        bar.module_padding = self.module_padding;
        bar.module_margin = self.module_margin;
        bar.module_gap = self.module_gap;
        bar.font_size = self.font_size;
        bar.font_weight = self.font_weight.to_string();
        bar.title_length = self.title_length;
        bar.tag_fill = self.tag_fill;
        bar.auto_height = true;

        let kept: Vec<String> = bar
            .modules
            .all()
            .filter(|m| kept_module(m))
            .cloned()
            .collect();
        bar.modules.left = strings(self.left);
        bar.modules.center = strings(self.center);
        bar.modules.right = strings(self.right);
        let at = bar
            .modules
            .right
            .iter()
            .position(|m| m == "tray")
            .unwrap_or(bar.modules.right.len());
        for (i, m) in kept.into_iter().enumerate() {
            bar.modules.right.insert(at + i, m);
        }
    }

    /// `bar` with this preset applied.
    pub fn applied_to(&self, bar: &BarConfig) -> BarConfig {
        let mut out = bar.clone();
        self.apply(&mut out);
        out
    }
}
