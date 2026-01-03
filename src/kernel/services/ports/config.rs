#[derive(Clone, Debug)]
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
        assert_eq!(config.scroll_step(), 1);
    }
}

