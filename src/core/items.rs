//! MangoWM items: binds, rules, monitors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Bind {
    #[serde(default)]
    pub mods: Vec<String>,
    pub key: String,
    pub action: String,
    #[serde(default)]
    pub args: String,
}

impl Bind {
    pub fn to_config_line(&self) -> String {
        let mods_str = self.mods.join("+");
        if self.args.is_empty() {
            format!("bind={},{},{}", mods_str, self.key, self.action)
        } else {
            format!("bind={},{},{},{}", mods_str, self.key, self.action, self.args)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Rule {
    pub pattern: String, // e.g. "class:kitty" or "title:Steam"
    pub effect: String,  // e.g. "float", "tag:2", "opacity:0.9"
}

impl Rule {
    pub fn to_config_line(&self) -> String {
        format!("rule={},{}", self.pattern, self.effect)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Monitor {
    pub name: String,
    pub resolution: String,
    pub position: String,
    pub scale: String,
}

impl Monitor {
    pub fn to_config_line(&self) -> String {
        format!("monitor={},{},{},{}", self.name, self.resolution, self.position, self.scale)
    }
}
