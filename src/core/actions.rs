//! MangoWM compositor actions catalog.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub description: String,
    pub has_arg: bool,
    pub arg_placeholder: Option<String>,
}

pub fn get_actions() -> Vec<Action> {
    vec![
        Action {
            id: "exec".into(),
            label: "Execute Command".into(),
            description: "Launch an application or shell command".into(),
            has_arg: true,
            arg_placeholder: Some("e.g. kitty, rofi -show drun".into()),
        },
        Action {
            id: "killclient".into(),
            label: "Close Active Window".into(),
            description: "Closes the focused window immediately".into(),
            has_arg: false,
            arg_placeholder: None,
        },
        Action {
            id: "togglefloating".into(),
            label: "Toggle Floating".into(),
            description: "Toggle active window between tiled and floating mode".into(),
            has_arg: false,
            arg_placeholder: None,
        },
        Action {
            id: "fullscreen".into(),
            label: "Toggle Fullscreen".into(),
            description: "Toggle active window fullscreen".into(),
            has_arg: false,
            arg_placeholder: None,
        },
        Action {
            id: "view".into(),
            label: "View Tag".into(),
            description: "Switch active tag (workspace)".into(),
            has_arg: true,
            arg_placeholder: Some("tag index 1-9".into()),
        },
        Action {
            id: "tag".into(),
            label: "Move Window to Tag".into(),
            description: "Move focused window to tag".into(),
            has_arg: true,
            arg_placeholder: Some("tag index 1-9".into()),
        },
        Action {
            id: "setlayout".into(),
            label: "Set Layout".into(),
            description: "Change tiling layout (scroller, master, monocle, grid, deck, dwindle)".into(),
            has_arg: true,
            arg_placeholder: Some("layout name".into()),
        },
        Action {
            id: "focusstack".into(),
            label: "Cycle Window Focus".into(),
            description: "Move focus to next (+1) or previous (-1) window".into(),
            has_arg: true,
            arg_placeholder: Some("+1 or -1".into()),
        },
        Action {
            id: "zoom".into(),
            label: "Promote to Master".into(),
            description: "Swap active window into the master area".into(),
            has_arg: false,
            arg_placeholder: None,
        },
        Action {
            id: "reload".into(),
            label: "Reload Config".into(),
            description: "Hot reload MangoWM configuration".into(),
            has_arg: false,
            arg_placeholder: None,
        },
    ]
}
