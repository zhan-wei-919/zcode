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
    pub theme: ThemeSettings,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive_border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_comment_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_keyword_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_keyword_control_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_string_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_number_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_type_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_attribute_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_namespace_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_macro_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_function_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_variable_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_constant_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_regex_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_active_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_active_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_tab_active_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_tab_active_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_tab_inactive_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_selected_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_selected_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_muted_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_guide_fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statusbar_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_current_match_bg: Option<String>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            focus_border: Some("cyan".to_string()),
            inactive_border: Some("dark_gray".to_string()),
            separator: Some("dark_gray".to_string()),
            accent_fg: Some("yellow".to_string()),
            syntax_comment_fg: Some("#6A9955".to_string()),
            syntax_keyword_fg: Some("#569CD6".to_string()),
            syntax_keyword_control_fg: Some("#C586C0".to_string()),
            syntax_string_fg: Some("#CE9178".to_string()),
            syntax_number_fg: Some("#B5CEA8".to_string()),
            syntax_type_fg: Some("#4EC9B0".to_string()),
            syntax_attribute_fg: Some("#4EC9B0".to_string()),
            syntax_namespace_fg: Some("#4EC9B0".to_string()),
            syntax_macro_fg: Some("#569CD6".to_string()),
            syntax_function_fg: Some("#DCDCAA".to_string()),
            syntax_variable_fg: Some("#9CDCFE".to_string()),
            syntax_constant_fg: Some("#4FC1FF".to_string()),
            syntax_regex_fg: Some("#D16969".to_string()),
            error_fg: Some("red".to_string()),
            warning_fg: Some("yellow".to_string()),
            activity_bg: None,
            activity_fg: Some("dark_gray".to_string()),
            activity_active_bg: Some("dark_gray".to_string()),
            activity_active_fg: Some("white".to_string()),
            sidebar_tab_active_bg: Some("dark_gray".to_string()),
            sidebar_tab_active_fg: Some("white".to_string()),
            sidebar_tab_inactive_fg: Some("dark_gray".to_string()),
            header_fg: Some("cyan".to_string()),
            palette_border: Some("cyan".to_string()),
            palette_bg: None,
            palette_fg: Some("white".to_string()),
            palette_selected_bg: Some("dark_gray".to_string()),
            palette_selected_fg: Some("white".to_string()),
            palette_muted_fg: Some("dark_gray".to_string()),
            indent_guide_fg: Some("dark_gray".to_string()),
            editor_bg: None,
            sidebar_bg: None,
            popup_bg: None,
            statusbar_bg: None,
            search_match_bg: None,
            search_current_match_bg: None,
        }
    }
}
