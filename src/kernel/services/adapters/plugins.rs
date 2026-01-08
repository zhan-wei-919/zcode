//! 插件配置（plugins.json）
//!
//! 目标：
//! - 跨平台放置在系统缓存目录下的 `.zcode/plugins.json`
//! - 由用户手动维护插件列表；启动时加载一次

use crate::kernel::plugins::PluginPriority;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const SETTINGS_DIR: &str = ".zcode";
const PLUGINS_FILE: &str = "plugins.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsConfig {
    #[serde(default)]
    pub plugins: Vec<PluginConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfigEntry {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: PluginPriority,
    #[serde(default)]
    pub transport: PluginTransport,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub restart: PluginRestartPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTransport {
    #[serde(rename = "type", default = "default_stdio")]
    pub kind: String,
}

impl Default for PluginTransport {
    fn default() -> Self {
        Self {
            kind: default_stdio(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRestartPolicy {
    #[serde(default = "default_restart_policy")]
    pub policy: String,
    #[serde(default)]
    pub max_per_minute: Option<u32>,
}

impl Default for PluginRestartPolicy {
    fn default() -> Self {
        Self {
            policy: default_restart_policy(),
            max_per_minute: None,
        }
    }
}

pub fn get_plugins_path() -> Option<PathBuf> {
    super::settings::get_settings_path().and_then(|path| {
        let dir = path.parent()?;
        let dir = if dir.ends_with(SETTINGS_DIR) {
            dir.to_path_buf()
        } else {
            dir.join(SETTINGS_DIR)
        };
        Some(dir.join(PLUGINS_FILE))
    })
}

pub fn ensure_plugins_file() -> std::io::Result<PathBuf> {
    let path = get_plugins_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cannot determine plugins directory",
        )
    })?;

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if !path.exists() {
        let content = serde_json::to_string_pretty(&PluginsConfig::default())
            .unwrap_or_else(|_| "{\"plugins\":[]}".to_string());
        std::fs::write(&path, content)?;
    }

    Ok(path)
}

pub fn load_plugins_config() -> Option<PluginsConfig> {
    let path = get_plugins_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn default_true() -> bool {
    true
}

fn default_stdio() -> String {
    "stdio".to_string()
}

fn default_restart_policy() -> String {
    "manual".to_string()
}

