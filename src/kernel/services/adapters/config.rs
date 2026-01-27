//! 配置服务：管理编辑器配置
//!
//! 提供统一的配置管理，支持运行时修改

use crate::core::Service;
use crate::kernel::services::ports::config::EditorConfig;

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
#[path = "../../../../tests/unit/kernel/services/adapters/config.rs"]
mod tests;
