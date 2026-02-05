use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct EditorConfig {
    pub tab_size: u8,
    pub default_viewport_height: usize,
    pub double_click_ms: u64,
    pub triple_click_ms: u64,
    pub click_slop: u16,
    pub scroll_lines: usize,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub auto_indent: bool,
    #[serde(default, alias = "formatOnSave")]
    pub format_on_save: bool,
    #[serde(default = "default_show_indent_guides")]
    pub show_indent_guides: bool,
}

fn default_show_indent_guides() -> bool {
    true
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
            format_on_save: false,
            show_indent_guides: default_show_indent_guides(),
        }
    }
}

impl EditorConfig {
    pub fn scroll_step(&self) -> usize {
        self.scroll_lines
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/ports/config.rs"]
mod tests;
