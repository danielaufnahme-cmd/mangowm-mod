# mangowm-mod

![MangoWM](https://img.shields.io/badge/MangoWM-Wayland-orange?style=flat-square)
![Mangobar](https://img.shields.io/badge/Mangobar-JSONC%2FCSS-teal?style=flat-square)
![Rust 1.80+](https://img.shields.io/badge/Rust-1.80%2B-dea584?style=flat-square&logo=rust)
![License MIT](https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square)

A command-line tool, written in Rust, that manages [MangoWM](https://github.com/mangowm/mango) settings and builds a Mangobar or Waybar status bar for it. It edits a separate file (`mango-mod.conf`) instead of your handwritten `config.conf`, backs up before every change, and tells Mango to reload. It also ships an optional GTK4/libadwaita settings window (`mangowm-mod-gui`) that uses the same files and backups. There is no live preview.

## What it covers

- All 240 plain MangoWM options from MangoWM 0.18.4 (taken from Mango's `option_defs.c` and docs), grouped into 10 pages. Browse them with `mangowm-mod schema`.
- A "Default Layout" (a `tag_rule` with `id:*`).
- Key binds with the flags `l`, `r`, `p`, `c`, `s` and key modes, using Mango's 94 real dispatch actions.
- `window_rule`, `window_rule_once`, `tag_rule`, `layer_rule` and `device_rule`.
- `monitor_rule` (resolution, position, scale, transform, VRR, disable, primary).
- `exec_once` / `exec` autostart commands.
- 12 tiling presets, each with its own wallpaper, plus backups and restore.
- Mangobar and Waybar config and style generation, start and stop, with saved choices.

## What it does not cover

- `mousebind`, `axisbind`, `gesturebind` and `switchbind` editing.
- `env` and `var` editing (they are kept if they are already in the managed file).
- TOML configs. The tool writes `.conf` only.
- A GUI or a live preview.

## Install

Requirements: Linux (it also builds on FreeBSD and OpenBSD; other systems print an "unsupported" message). Building from source needs Rust 1.80 or newer.

### Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/danielaufnahme-cmd/mangowm-mod/main/install.sh | sh
```

The script installs to `~/.local/bin` and prints how to add `~/.local/bin` to your `PATH` for bash, zsh and fish. If `cargo` is missing it downloads a prebuilt CLI binary and verifies its SHA-256.

With `cargo` and the GTK4 and libadwaita development packages present, it builds and installs both `mangowm-mod` and `mangowm-mod-gui`, plus a launcher entry (`~/.local/share/applications/mangowm-mod.desktop`, an existing file with that name is replaced) and icon, so the app shows up in your application menu. If those packages are missing, it prints the install command for your distro (for example `sudo dnf install gtk4-devel libadwaita-devel` on Fedora) and asks `Install them now with sudo? [y/N]`. The default is No; sudo runs only if you type `y`. With `--yes`, or without a terminal (as in `curl | sh`), it never runs sudo: it prints the command and installs the CLI only. If the GUI build fails (libadwaita needs a recent Rust), the CLI is still installed. The prebuilt binaries are CLI only; the GUI needs a source build.

| Option | Meaning |
| --- | --- |
| `--prebuilt` | Download the prebuilt binary even if cargo exists |
| `--source` | Build from source |
| `--version vX.Y.Z` | Install a specific release |
| `--bin-dir DIR` | Install somewhere other than `~/.local/bin` |
| `--no-gui` | Install only the CLI, skip the GUI and launcher entry |
| `--yes` | Do not ask questions (never runs sudo) |
| `--help` | Show help |

### Prebuilt binaries

Static musl binaries (CLI only, no GUI) for x86_64 and aarch64 are attached to the [GitHub releases](https://github.com/danielaufnahme-cmd/mangowm-mod/releases). CI builds them on version tags.

### From source

```bash
git clone https://github.com/danielaufnahme-cmd/mangowm-mod
cd mangowm-mod
cargo build --release
cargo install --path .
```

To include the GUI, install the GTK4 and libadwaita development packages (Fedora: `gtk4-devel libadwaita-devel`, Arch: `gtk4 libadwaita`, Debian/Ubuntu: `libgtk-4-dev libadwaita-1-dev`, openSUSE: `gtk4-devel libadwaita-devel`, Alpine: `gtk4.0-dev libadwaita-dev`, Void: `gtk4-devel libadwaita-devel`) and add the feature flag:

```bash
cargo install --path . --features gui
```

`./install.sh` does this for you and also installs the launcher entry.

### Getting MangoWM, Mangobar and Waybar

Checked on the Mango docs installation page and Repology on 2026-10-31.

| Distro | MangoWM | Mangobar | Waybar |
| --- | --- | --- | --- |
| Arch | `sudo pacman -S mangowm` (official repo, 0.18.4) | AUR `mangobar-git` | `sudo pacman -S waybar` (0.15.0: use `--waybar-mango-modules false`, or a newer Waybar build) |
| Fedora | Terra repo: `sudo dnf install mangowm` after adding Terra (see Mango docs) | Terra Rawhide only, otherwise build from source | `sudo dnf install waybar` |
| Debian / Ubuntu | Not packaged, build from source (Mango docs "Building from Source") | Build from source (the mangobar README lists the apt build deps) | `sudo apt install waybar` |
| openSUSE | Not packaged, build from source | Build from source | `sudo zypper install waybar` (unverified) |
| Alpine | edge: `apk add mangowm` (3.24 has the old 0.14.2) | edge: `apk add mangobar` | `apk add waybar` |
| Void | `sudo xbps-install mangowc` (the package is named mangowc) | Build from source | `sudo xbps-install Waybar` (unverified name) |
| Gentoo | GURU overlay: `gui-wm/mangowm` | Build from source | `gui-apps/waybar` |
| NixOS | nixpkgs unstable `mango`, or the mango flake module (Mango docs) | mangobar flake / Home Manager module | `pkgs.waybar` |
| Guix | `guix install mangowm` | `guix install mangobar` | `guix install waybar` |

CI tests the tool on Fedora, Ubuntu 24.04, Debian stable, Arch, Alpine, openSUSE Tumbleweed and on the minimum Rust version (1.80).

## How it works

- **Files written.** Only `mango-mod.conf`, next to your `config.conf`, plus one `source=./mango-mod.conf` line appended to `config.conf`. Mango reads `~/.config/mango/config.conf` (it does not use `XDG_CONFIG_HOME`).
- **Staging.** `set`, `reset`, `bind add` and similar commands stage changes. Staged changes survive between commands. `status` shows them, `discard` drops them, `apply` writes them. `preset apply` applies at once.
- **Backups.** Before every apply, both `config.conf` and `mango-mod.conf` are backed up to `~/.local/share/mangowm-mod/backups` (honors `XDG_DATA_HOME`). Use `backup list` and `backup restore`.
- **Missing config.** If `config.conf` does not exist it is created from `/etc/mango/config.conf` (Mango's documented default) or a minimal config. If only `config.toml` exists, nothing is created and the tool prints the `source = [...]` line to add under `[global]`.
- **Reload.** `apply` runs `mmsg dispatch reload_config` and reports success only when Mango confirms. Mango also reloads on file change by default (`auto_reload_config=1`). `--dry-run` (or env `MANGOWM_MOD_DRY_RUN=1`) writes files but skips the reload.
- **Value formats.** Values are written the way Mango parses them: booleans as `0`/`1`, colors as `0xRRGGBBAA`. Color input accepts `#rrggbb`, `#rrggbbaa`, `0xRRGGBBAA` and `rgba()`.

## Command reference

Global options, valid on every command: `--config PATH` (Mango `config.conf`), `--managed PATH` (`mango-mod.conf`), `--data-dir DIR` (backups, staged changes, bar settings), `--dry-run`, `-h/--help`, `-V/--version`.

### Settings

```bash
mangowm-mod status                       # paths and staged changes
mangowm-mod get border_radius            # current value of a setting
mangowm-mod set border_radius 12         # stage a setting
mangowm-mod set focus_color "#7aa2f7"    # colors accept #rrggbb and more
mangowm-mod reset border_radius          # stage removing it; Mango falls back to config.conf or its default
mangowm-mod discard                      # drop all staged changes
mangowm-mod apply                        # write mango-mod.conf and reload
```

### schema

```bash
mangowm-mod schema                       # all 10 pages
mangowm-mod schema look                  # one page (look, effects, animations, layouts,
                                         #   bars, overview, scratchpad, input, behavior, display)
mangowm-mod schema --search gap          # search keys and labels
```

### preset

```bash
mangowm-mod preset list                  # list presets
mangowm-mod preset apply tokyo-night     # by ID or name, applied at once
```

### wallpaper

```bash
mangowm-mod wallpaper set ~/Pictures/Wallpapers/forest.png   # applied at once
```

Sets the same single `exec_once=swaybg` line presets use, so a preset applied later replaces it. The GUI's Wallpaper page shows the bundled wallpapers, `~/Pictures/Wallpapers`, `~/Pictures/wallpapers` and Plasma's current wallpaper as a thumbnail grid (thumbnails cached in `~/.cache/mangowm-mod/thumbs`).

### bind

`bind add MODS KEY ACTION [ARGS...]`. Modifiers are joined with `+` (SUPER, CTRL, SHIFT, ALT, HYPER) or `NONE`. Options: `--flags` (any of `s l r p c`; `l` = also when locked), `--mode` (key mode, default `default`), `--replace` (replace a managed bind with the same keys). Put options like `--flags`, `--mode` and `--replace` before MODS; everything after ACTION is passed to the action.

```bash
mangowm-mod bind add SUPER+SHIFT Return spawn foot
mangowm-mod bind add --flags l NONE XF86AudioRaiseVolume spawn "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+"
mangowm-mod bind list                    # managed binds, numbered
mangowm-mod bind remove 1                # remove by number from `bind list`
mangowm-mod bind actions                 # the 94 Mango actions
```

### rule

`rule add KIND "key:value,key:value"` where KIND is `window_rule`, `window_rule_once`, `tag_rule`, `layer_rule` or `device_rule`.

```bash
mangowm-mod rule add window_rule "app_id:mpv,is_floating:1"
mangowm-mod rule list
mangowm-mod rule remove 1                # number from `rule list`
```

### monitor

`monitor add NAME` with options `--width`, `--height`, `--refresh` (Hz), `--x`, `--y`, `--scale`, `--transform` (0-7), `--vrr true|false`, `--disable`, `--primary`.

```bash
mangowm-mod monitor add eDP-1 --width 1920 --height 1080 --refresh 60 --scale 1
mangowm-mod monitor list
mangowm-mod monitor remove 1             # number from `monitor list`
```

### autostart

Commands are written as `exec_once` by default; `--every-reload` writes `exec` (runs on every config reload).

```bash
mangowm-mod autostart add mangowm-mod bar start
mangowm-mod autostart add --every-reload notify-send "reloaded"
mangowm-mod autostart list
mangowm-mod autostart remove 1           # number from `autostart list`
```

### backup

```bash
mangowm-mod backup list
mangowm-mod backup restore <ID>          # ID from `backup list`; reloads MangoWM
```

### bar

See the next section for the details.

```bash
mangowm-mod bar generate                 # write config.jsonc and style.css (-o DIR for another folder)
mangowm-mod bar start                    # generate and start the bar
mangowm-mod bar stop                     # stop the bar the tool started
mangowm-mod bar show                     # saved bar settings
mangowm-mod bar set height 36            # set a field, a top-level key or <module>.<option>
mangowm-mod bar unset height             # remove a top-level or <module>.<option> override
mangowm-mod bar modules                  # list the bar modules
```

## Bar builder

`bar generate` and `bar start` take the same flags: `--backend mangobar|waybar`, `--height`, `--radius`, `--bg`, `--fg`, `--accent`, `--font`, `--font-size`, `--layer top|bottom|overlay`, `--left`, `--center`, `--right` (comma-separated module lists) and `--waybar-mango-modules true|false`. Choices made with `bar set` are saved in `~/.local/share/mangowm-mod/bar.json`.

| | Mangobar | Waybar |
| --- | --- | --- |
| Files | `~/.config/mangobar/mango-mod/` | `~/.config/waybar/mango-mod/` |
| Started as | `mangobar -c … -s …` | Waybar with the generated config and style |
| Your own config | not touched | never touched |

```bash
mangowm-mod bar start --backend mangobar --bg "#1e1e2eeb" --accent "#7aa2f7" --height 32
mangowm-mod bar start --backend waybar --left workspaces,layout --center window --right cpu,memory,clock,tray
mangowm-mod bar start --backend waybar --waybar-mango-modules false   # Waybar 0.15.0 or older
```

`bar modules` lists: workspaces, layout, window, keymode, keyboardlayout, hideclients (Mangobar only), cpu, memory, backlight, pulseaudio, battery, network, clock (also `clock#date`, `clock#time`), tray, and also `battery#<device>` and `custom/<name>`.

The bar is stopped using a PID file; `pkill -f` matching only the bar the tool started is the fallback.

## Presets

`clean` (calm blue), `tokyo-night` (deep navy, blue and cyan), `nord` (arctic blues), `gruvbox` (warm amber and orange), `glass` (translucent, deep blur), `zen` (no borders, no gaps, no blur), `everforest` (soft greens), `rose-pine` (muted rose and gold), `kanagawa` (ink blue), `solarized` (deep-sea teal), `latte` (light), `graphite` (monochrome).

Every preset is a tiling setup: Mango's `tile` layout on every tag (`tag_rule=id:*,layout_name:tile`, after your own tag rules so it wins) and new windows join the stack (`new_is_master=0`). Each one also sets its own wallpaper: an original, procedurally drawn PNG (`data/wallpapers`, made by `render.py` there) that `install.sh` copies to `~/.local/share/mangowm-mod/wallpapers`. Applying a preset writes `exec_once=swaybg -m fill -i <wallpaper>` to `mango-mod.conf` (replacing any earlier managed swaybg line) and, when Mango confirms the reload, swaps the running swaybg right away. If your `config.conf` starts its own swaybg, remove that line, or at login either wallpaper may end up on top (the tool warns about it).

## Compatibility notes

- **Mango version.** The option list comes from MangoWM 0.18.4. Older versions may not know every option.
- **Waybar.** The `mango/*` modules exist only in Waybar newer than 0.15.0 (Waybar master as of 2026). On 0.15.0 or older use `--waybar-mango-modules false`: workspaces then use `ext/workspaces`, and layout, window, keymode and language are left out.
- **Monitor rules.** Mango uses the first matching rule, so a `monitor_rule` already in your own `config.conf` wins over one added by the tool. The tool warns about it.
- **TOML.** Only `.conf` is written. With a `config.toml`, add the `source = [...]` line the tool prints.

## License

[MIT](LICENSE)

Built with help from AI (Claude).
