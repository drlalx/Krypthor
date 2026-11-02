pub mod cli;
#[cfg(feature = "egui-ui")]
pub mod egui_ui;
#[cfg(all(unix, feature = "fuse"))]
pub mod fs_fuse;
#[cfg(feature = "tui")]
pub mod tui_app;
pub mod vault;
#[cfg(feature = "web")]
pub mod web_server;
#[cfg(feature = "web")]
pub mod webdav_fs;

use serde::{Deserialize, Serialize};
use std::fs;

pub const CONFIG_FILE: &str = "vaults.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedVault {
    pub name: String,
    pub path: String,
}

pub fn load_saved_vaults() -> Vec<SavedVault> {
    if std::path::Path::new(CONFIG_FILE).exists()
        && let Ok(content) = fs::read_to_string(CONFIG_FILE)
        && let Ok(vaults) = serde_json::from_str::<Vec<SavedVault>>(&content)
    {
        return vaults;
    }
    Vec::new()
}

pub fn save_saved_vaults(vaults: &[SavedVault]) {
    if let Ok(content) = serde_json::to_string_pretty(vaults) {
        let _ = fs::write(CONFIG_FILE, content);
    }
}
