use serde::{Deserialize, Serialize};

use super::config::EditorConfig;
use crate::kernel::editor::{SyntaxColorGroup, DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX};
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
        let mut settings = Self {
            focus_border: Some("cyan".to_string()),
            inactive_border: Some("dark_gray".to_string()),
            separator: Some("dark_gray".to_string()),
            accent_fg: Some("yellow".to_string()),
            syntax_comment_fg: None,
            syntax_keyword_fg: None,
            syntax_keyword_control_fg: None,
            syntax_string_fg: None,
            syntax_number_fg: None,
            syntax_type_fg: None,
            syntax_attribute_fg: None,
            syntax_namespace_fg: None,
            syntax_macro_fg: None,
            syntax_function_fg: None,
            syntax_variable_fg: None,
            syntax_constant_fg: None,
            syntax_regex_fg: None,
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
        };

        for (idx, group) in SyntaxColorGroup::CONFIGURABLE.iter().copied().enumerate() {
            settings.set_syntax_color(
                group,
                Some(format!("#{:06X}", DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX[idx])),
            );
        }

        settings
    }
}

impl ThemeSettings {
    pub fn syntax_color_for(&self, group: SyntaxColorGroup) -> Option<&str> {
        match group {
            SyntaxColorGroup::Comment => self.syntax_comment_fg.as_deref(),
            SyntaxColorGroup::String => self.syntax_string_fg.as_deref(),
            SyntaxColorGroup::Regex => self.syntax_regex_fg.as_deref(),
            SyntaxColorGroup::Keyword => self.syntax_keyword_fg.as_deref(),
            SyntaxColorGroup::KeywordControl => self.syntax_keyword_control_fg.as_deref(),
            SyntaxColorGroup::Type => self.syntax_type_fg.as_deref(),
            SyntaxColorGroup::Number => self.syntax_number_fg.as_deref(),
            SyntaxColorGroup::Function => self.syntax_function_fg.as_deref(),
            SyntaxColorGroup::Macro => self.syntax_macro_fg.as_deref(),
            SyntaxColorGroup::Namespace => self.syntax_namespace_fg.as_deref(),
            SyntaxColorGroup::Variable => self.syntax_variable_fg.as_deref(),
            SyntaxColorGroup::Constant => self.syntax_constant_fg.as_deref(),
            SyntaxColorGroup::Attribute => self.syntax_attribute_fg.as_deref(),
            SyntaxColorGroup::Operator | SyntaxColorGroup::Tag => None,
        }
    }

    pub fn set_syntax_color(&mut self, group: SyntaxColorGroup, value: Option<String>) {
        match group {
            SyntaxColorGroup::Comment => self.syntax_comment_fg = value,
            SyntaxColorGroup::String => self.syntax_string_fg = value,
            SyntaxColorGroup::Regex => self.syntax_regex_fg = value,
            SyntaxColorGroup::Keyword => self.syntax_keyword_fg = value,
            SyntaxColorGroup::KeywordControl => self.syntax_keyword_control_fg = value,
            SyntaxColorGroup::Type => self.syntax_type_fg = value,
            SyntaxColorGroup::Number => self.syntax_number_fg = value,
            SyntaxColorGroup::Function => self.syntax_function_fg = value,
            SyntaxColorGroup::Macro => self.syntax_macro_fg = value,
            SyntaxColorGroup::Namespace => self.syntax_namespace_fg = value,
            SyntaxColorGroup::Variable => self.syntax_variable_fg = value,
            SyntaxColorGroup::Constant => self.syntax_constant_fg = value,
            SyntaxColorGroup::Attribute => self.syntax_attribute_fg = value,
            SyntaxColorGroup::Operator | SyntaxColorGroup::Tag => {}
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/ports/settings.rs"]
mod tests;
