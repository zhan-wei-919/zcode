use serde::{Deserialize, Serialize};

use super::config::EditorConfig;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub keybindings: Vec<KeybindingRule>,
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub lsp: LspSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LspSettings {
    /// LSP server command (e.g. "rust-analyzer" or "/usr/bin/rust-analyzer").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Extra arguments passed to the LSP server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Per-server overrides keyed by server name (e.g. "gopls", "pyright", "tsls").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, LspServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LspServerConfig {
    /// LSP server command override for this language server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Extra args override. When omitted, zcode uses server-specific defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Optional initialization options forwarded to the server as-is.
    ///
    /// Useful for server-specific knobs such as semantic token behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    #[serde(default = "default_worktree_bar_visible")]
    pub worktree_bar: bool,
}

fn default_worktree_bar_visible() -> bool {
    true
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            worktree_bar: default_worktree_bar_visible(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingRule {
    pub key: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}
