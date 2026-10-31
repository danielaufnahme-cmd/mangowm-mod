//! MangoWM dispatch actions (the function names usable in `bind=` lines and
//! `mmsg dispatch`). Names come from MangoWM 0.18.4 `parse_func_name()` in
//! `src/config/binding_parse.c`; argument hints and descriptions from
//! docs/bindings/keys.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub description: String,
    pub has_arg: bool,
    pub arg_placeholder: Option<String>,
}

/// (name, argument hint or "", description)
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("focusstack", "next/prev", "Cycle focus within the stack."),
    ("overcircle", "next/prev/current_next/current_prev", "Open overview when closed; while it is open, cycle focus to the next/previous window on the current monitor. current_next/current_prev only show the current tagset's windows in the overview instead of all tags."),
    ("groupfocus", "prev/next", "Focus group member by direction."),
    ("focusdir", "left/right/up/down", "Focus window in direction."),
    ("focus_window_or_workspace", "left/right/up/down", "Focus window in direction; otherwise jump to the nearest adjacent tag that has clients, falling back to the next/previous tag if none."),
    ("groupjoin", "left/right/up/down", "Join group by direction."),
    ("groupleave", "", "Leave group."),
    ("focusid", "", "Focus window (can target any window via IPC: mmsg dispatch focusid client,<id>)"),
    ("incnmaster", "+1/-1", "Increase/Decrease number of master windows."),
    ("setmfact", "+0.05", "Increase/Decrease master area size."),
    ("zoom", "", "Swap focused window with Master."),
    ("exchange_client", "left/right/up/down", "Swap the focused window with its neighbor in direction. Both windows change place, and with exchange_cross_monitor enabled they also swap monitors."),
    ("move_client", "left/right/up/down", "Move the focused window one step in direction. On the same monitor dwindle re-inserts it next to the neighbor keeping the row/column it came from, every other layout swaps it with the neighbor like exchange_client. When the neighbor lies on another monitor the window moves onto that monitor and is inserted in front of or behind the neighbor on the side it comes from; without a neighbor in that direction it moves onto the monitor lying there. Crossing monitors needs exchange_cross_monitor."),
    ("exchange_stack_client", "next/prev", "Exchange window position in stack."),
    ("toggleglobal", "", "Pin window to all tags."),
    ("togglehdr", "", "Toggle HDR on the focused monitor"),
    ("toggleoverview", "[1]", "Toggle overview mode. Passing 1 only shows the current tagset's windows in the overview instead of all tags."),
    ("enteroverview", "", "Enter overview mode."),
    ("leaveoverview", "", "Leave overview mode."),
    ("togglejump", "", "Toggle overview with jump mode."),
    ("set_proportion", "float", "Set scroller window proportion (0.0–1.0)."),
    ("switch_proportion_preset", "", "Cycle proportion presets of scroller window."),
    ("viewtoleft", "[synctag]", "View previous tag. Optional synctag (0/1) syncs to all monitors."),
    ("viewtoright", "[synctag]", "View next tag. Optional synctag (0/1) syncs to all monitors."),
    ("view_insert", "prev/next", "View the adjacent tag if it is empty; otherwise insert an empty tag before/after the current one and switch to it."),
    ("tagsilent", "mask", "Move window to tag(s) without focusing it. Accepts a tag mask."),
    ("tagtoleft", "[synctag]", "Move window to left tag. Optional synctag (0/1)."),
    ("tagtoright", "[synctag]", "Move window to right tag. Optional synctag (0/1)."),
    ("killclient", "force", "Close the focused window. If force is specified, sends SIGKILL."),
    ("smartkillclient", "", "Remove the focused window from the current tag(s) if it is on multiple tags; otherwise close it."),
    ("centerwin", "", "Center the floating window."),
    ("focuslast", "", "Focus the previously active window."),
    ("switcher", "next/prev, all_tag_next/all_tag_prev, all_next/all_prev", "Open or cycle the thumbnail switcher. next/prev list the current tag's windows, all_tag_next/all_tag_prev list all tags on the current monitor, all_next/all_prev list all monitors and tags. Releasing any modifier key selects."),
    ("toggle_trackpad_enable", "", "Toggle trackpad enable."),
    ("setoption", "key,value", "Set config option temporarily."),
    ("setkeymode", "mode", "Set key_mode."),
    ("switch_keyboard_layout", "[index]", "Switch keyboard layout. Optional index (0, 1, 2...) to switch to specific layout."),
    ("setlayout", "name", "Switch to layout (e.g., scroller, tile)."),
    ("switch_layout", "", "Cycle through available layouts."),
    ("togglefloating", "", "Toggle floating state."),
    ("togglefullscreen", "", "Toggle fullscreen."),
    ("togglefakefullscreen", "", "Toggle \"fake\" fullscreen (remains constrained)."),
    ("toggleoverlay", "", "Toggle overlay state for the focused window."),
    ("minimized", "", "Minimize window to scratchpad."),
    ("restore_minimized", "", "Restore minimized window to the currently focused tag."),
    ("toggle_scratchpad", "", "Toggle scratchpad."),
    ("toggle_render_border", "", "Toggle border rendering."),
    ("focusmon", "left/right/up/down/next/prev/monitor_spec", "Focus monitor by direction, by cycling to the next or previous monitor (next/prev), or by monitor spec."),
    ("tagmon", "left/right/up/down/next/prev/monitor_spec,[keeptag]", "Move window to monitor by direction, by cycling to the next or previous monitor (next/prev), or by monitor spec. keeptag is 0 or 1."),
    ("incgaps", "+/-value", "Adjust gap size."),
    ("togglegaps", "", "Toggle gaps."),
    ("togglegroupbar", "", "Toggle the group bar on every tiled window."),
    ("chvt", "n", "Switch to virtual terminal n"),
    ("spawn", "cmd", "Execute a command."),
    ("spawn_shell", "cmd", "Execute shell command (supports pipes \\|)."),
    ("spawn_on_empty", "cmd, tagmask", "Open command on empty tag.Accepts a cmd string and tagmask"),
    ("quit", "", "Exit mangowm."),
    ("create_virtual_output", "", "Create a headless monitor (for VNC/Sunshine)."),
    ("destroy_all_virtual_output", "", "Destroy all virtual monitors."),
    ("moveresize", "curmove/curresize", "Move or resize the window with the pointer (mouse binds)"),
    ("togglemaximizescreen", "", "Maximize window (keep decoration/bar)."),
    ("viewprev_have_client", "", "Not described in Mango's docs."),
    ("viewnext_have_client", "", "Not described in Mango's docs."),
    ("viewtoleft_have_client", "[synctag]", "View left tag and focus client if present. Optional synctag (0/1)."),
    ("viewtoright_have_client", "[synctag]", "View right tag and focus client if present. Optional synctag (0/1)."),
    ("reload_config", "", "Hot-reload configuration. Does not support keybind conflict (c flag)."),
    ("load_config_file", "file path", "Load configuration from the specified file. Empty path resets to default config location. Does not support keybind conflict (c flag)."),
    ("tag", "mask[,synctag]", "Move window to tag(s). Accepts a tag mask. Optional synctag (0/1) syncs to all monitors."),
    ("view", "mask[,synctag]", "View tag(s). Accepts a tag mask. Additionally, 00 shows all tags, -1 shows the previous tagset. Optional synctag (0/1) syncs the action to all monitors."),
    ("viewcrossmon", "mask,monitor_spec", "View specified tag(s) on specified monitor. Accepts a tag mask and a monitor spec."),
    ("tagcrossmon", "mask,monitor_spec", "Move window to tag(s) on specified monitor. Accepts a tag mask and a monitor spec."),
    ("toggletag", "mask", "Toggle tag(s) on window. Accepts a tag mask. 00 toggles all tags."),
    ("toggleview", "mask", "Toggle view of tag(s). Accepts a tag mask."),
    ("comboview", "mask", "View multiple tags simultaneously. Accepts a tag mask (typically built by pressing keys, e.g., 1|3)."),
    ("smartmovewin", "left/right/up/down", "Move floating window by snap distance."),
    ("smartresizewin", "left/right/up/down", "Resize floating window by snap distance."),
    ("resizewin", "(width,height)", "Resize window."),
    ("movewin", "(x,y)", "Move floating window."),
    ("toggle_named_scratchpad", "app_id,title,cmd", "Toggle named scratchpad. Launches app if not running, otherwise shows/hides it."),
    ("toggle_special_tag", "", "Toggle special workspace overlay (tiling scratchpad)."),
    ("tag_special_tag", "", "Move focused window to/from the special workspace overlay."),
    ("tag_special_silent", "", "Silently move focused window to/from the special workspace overlay."),
    ("disable_monitor", "monitor_spec", "remove monitor. Accepts a monitor spec."),
    ("enable_monitor", "monitor_spec", "add monitor. Accepts a monitor spec."),
    ("toggle_monitor", "monitor_spec", "Toggle monitor add/remove. Accepts a monitor spec."),
    ("sleep_monitor", "monitor_spec", "Shutdown monitor power but not remove. Accepts a monitor spec."),
    ("wakeup_monitor", "monitor_spec", "Turn on monitor power. Accepts a monitor spec."),
    ("sleep_toggle_monitor", "monitor_spec", "Toggle monitor power but not remove."),
    ("scroller_stack", "left/right/up/down", "Move window inside/outside scroller stack by direction."),
    ("toggle_all_floating", "", "Toggle all visible clients floating state."),
    ("dwindle_toggle_split_direction", "", "Toggle the split direction in the dwindle layout"),
    ("dwindle_split_horizontal", "", "Set split window direction to horizontal in dwindle layout."),
    ("dwindle_split_vertical", "", "Set split window direction to vertical in dwindle layout."),
    ("dwindle_toggle_current_split", "", "Toggle split direction of current window in dwindle layout."),
];

pub fn is_known_action(name: &str) -> bool {
    ACTIONS.iter().any(|(id, _, _)| *id == name)
}

fn label_for(id: &str) -> String {
    id.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn get_actions() -> Vec<Action> {
    ACTIONS
        .iter()
        .map(|&(id, arg, desc)| Action {
            id: id.to_string(),
            label: label_for(id),
            description: desc.to_string(),
            has_arg: !arg.is_empty(),
            arg_placeholder: (!arg.is_empty()).then(|| arg.to_string()),
        })
        .collect()
}
