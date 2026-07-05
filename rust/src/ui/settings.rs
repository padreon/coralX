//! Small persisted-settings store (stand-in for Qt's `QSettings`).
//!
//! Stored as JSON under the platform config dir, e.g.
//! `~/.config/coralX/settings.json` on Linux.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub model_path: String,
    #[serde(default = "default_conf")]
    pub conf_threshold: f64,
    #[serde(default = "default_crop")]
    pub crop_size: i64,
    #[serde(default = "default_scope")]
    pub scope: String,
    /// "Label only unlabeled points" checkbox state (true = don't overwrite).
    #[serde(default = "default_true")]
    pub only_unlabeled: bool,
}

fn default_conf() -> f64 {
    0.5
}
fn default_crop() -> i64 {
    64
}
fn default_scope() -> String {
    "image".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            model_path: String::new(),
            conf_threshold: default_conf(),
            crop_size: default_crop(),
            scope: default_scope(),
            only_unlabeled: true,
        }
    }
}

fn settings_path() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home_dir)
    } else if cfg!(target_os = "macos") {
        home_dir().join("Library").join("Preferences")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home_dir().join(".config"))
    };
    base.join("coralX").join("settings.json")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

impl AiSettings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
