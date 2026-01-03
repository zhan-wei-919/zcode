use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub keybindings: Vec<KeybindingRule>,
    #[serde(default)]
    pub theme: ThemeSettings,
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
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            focus_border: Some("cyan".to_string()),
            inactive_border: Some("dark_gray".to_string()),
            separator: Some("dark_gray".to_string()),
            accent_fg: Some("yellow".to_string()),
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
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keybindings: Vec::new(),
            theme: ThemeSettings::default(),
        }
    }
}

