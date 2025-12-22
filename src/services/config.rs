//! 配置服务：管理编辑器配置
//!
//! 提供统一的配置管理，支持运行时修改

use crate::core::Service;

#[derive(Clone, Debug)]
pub struct EditorConfig {
    pub tab_size: u8,
    pub default_viewport_height: usize,
    pub double_click_ms: u64,
    pub triple_click_ms: u64,
    pub click_slop: u16,
    /// 每次滚轮滚动的行数
    pub scroll_lines: usize,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub auto_indent: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            default_viewport_height: 20,
            double_click_ms: 300,
            triple_click_ms: 450,
            click_slop: 2,
            scroll_lines: 1,
            show_line_numbers: true,
            word_wrap: false,
            auto_indent: true,
        }
    }
}

impl EditorConfig {
    pub fn scroll_step(&self) -> usize {
        self.scroll_lines
    }
}

pub struct ConfigService {
    editor: EditorConfig,
}

impl ConfigService {
    pub fn new() -> Self {
        Self {
            editor: EditorConfig::default(),
        }
    }

    pub fn with_editor_config(editor: EditorConfig) -> Self {
        Self { editor }
    }

    pub fn editor(&self) -> &EditorConfig {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut EditorConfig {
        &mut self.editor
    }

    pub fn set_tab_size(&mut self, size: u8) {
        self.editor.tab_size = size;
    }

    pub fn set_show_line_numbers(&mut self, show: bool) {
        self.editor.show_line_numbers = show;
    }

    pub fn set_word_wrap(&mut self, wrap: bool) {
        self.editor.word_wrap = wrap;
    }
}

impl Default for ConfigService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service for ConfigService {
    fn name(&self) -> &'static str {
        "ConfigService"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EditorConfig::default();
        assert_eq!(config.tab_size, 4);
        assert!(config.show_line_numbers);
    }

    #[test]
    fn test_scroll_step() {
        let config = EditorConfig::default();
        let step = config.scroll_step();
        assert_eq!(step, 1);
    }

    #[test]
    fn test_config_service() {
        let mut service = ConfigService::new();
        assert_eq!(service.editor().tab_size, 4);

        service.set_tab_size(2);
        assert_eq!(service.editor().tab_size, 2);
    }

    #[test]
    fn test_service_trait() {
        let service = ConfigService::new();
        assert_eq!(service.name(), "ConfigService");
    }
}
