# MangoWM Mod & Mangobar Builder

**Fast, native settings and ricing manager for MangoWM and Mangobar status bar written in Rust.**

![MangoWM](https://img.shields.io/badge/MangoWM-Wayland-orange?style=flat-square)
![Mangobar](https://img.shields.io/badge/Mangobar-JSONC%2FCSS-teal?style=flat-square)
![Rust 2021](https://img.shields.io/badge/Rust-1.75%2B-dea584?style=flat-square&logo=rust)
![License MIT](https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square)

## Features

- **SceneFX Visual Controls:** Configure blur radius, drop shadows, window opacity, border widths, and corner rounding.
- **Dynamic Layout Management:** Switch between Scroller, Master-Stack, Monocle, Grid, Deck, and Dwindle layouts, and tune master ratios and gaps.
- **Mangobar Builder:** Complete status bar generator for `mangobar` with live preview, 9-tag DWL workspace indicators, layout status, window title, system monitors (CPU, RAM, volume, battery, brightness, tray), and CSS theming.
- **Safe by Design:** Writes only to its own `mango-mod.conf` and injects `source=./mango-mod.conf` so your handwritten `config.conf` remains intact.
- **Automatic Backups & One-Click Restore:** Backs up configs prior to applying any changes.
- **Built-in Looks & Presets:** Clean, Tokyo Night, Nord, Gruvbox, Glass, and Zen.

## Install

### Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/danielaufnahme-cmd/mangowm-mod/main/install.sh | bash
```

### Build from source (Rust)

```bash
git clone https://github.com/danielaufnahme-cmd/mangowm-mod
cd mangowm-mod
cargo build --release
cargo install --path .
```

## CLI Usage

```bash
# View status and pending changes
mangowm-mod status

# List and apply presets
mangowm-mod preset list
mangowm-mod preset apply tokyo-night

# Inspect schema options
mangowm-mod schema

# Set a setting
mangowm-mod set corner_radius 12
mangowm-mod set focused_border_color "#7aa2f7"
mangowm-mod apply

# Build and start Mangobar
mangowm-mod bar start
mangowm-mod bar generate
```

## License

[MIT](LICENSE)
