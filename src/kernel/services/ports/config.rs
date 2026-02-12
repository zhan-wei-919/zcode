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
    #[serde(default = "default_show_indent_guides", alias = "showIndentGuides")]
    pub show_indent_guides: bool,
    #[serde(default, alias = "lspInputTiming")]
    pub lsp_input_timing: LspInputTimingConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LspInputTimingConfig {
    #[serde(default = "default_boundary_chars", alias = "boundaryChars")]
    pub boundary_chars: String,
    #[serde(default = "default_boundary_immediate", alias = "boundaryImmediate")]
    pub boundary_immediate: bool,
    #[serde(default, alias = "identifierDebounceMs")]
    pub identifier_debounce_ms: LspIdentifierDebounceMs,
    #[serde(default, alias = "deleteDebounceMs")]
    pub delete_debounce_ms: LspDeleteDebounceMs,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LspIdentifierDebounceMs {
    pub completion: u64,
    pub semantic_tokens: u64,
    pub inlay_hints: u64,
    pub folding_range: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LspDeleteDebounceMs {
    pub completion: u64,
    pub semantic_tokens: u64,
    pub inlay_hints: u64,
    pub folding_range: u64,
}

fn default_show_indent_guides() -> bool {
    true
}

fn default_boundary_chars() -> String {
    " \t\n.,;:()[]{}".to_string()
}

fn default_boundary_immediate() -> bool {
    true
}

impl Default for LspIdentifierDebounceMs {
    fn default() -> Self {
        Self {
            completion: 240,
            semantic_tokens: 360,
            inlay_hints: 420,
            folding_range: 480,
        }
    }
}

impl Default for LspDeleteDebounceMs {
    fn default() -> Self {
        Self {
            completion: 120,
            semantic_tokens: 140,
            inlay_hints: 180,
            folding_range: 220,
        }
    }
}

impl Default for LspInputTimingConfig {
    fn default() -> Self {
        Self {
            boundary_chars: default_boundary_chars(),
            boundary_immediate: default_boundary_immediate(),
            identifier_debounce_ms: LspIdentifierDebounceMs::default(),
            delete_debounce_ms: LspDeleteDebounceMs::default(),
        }
    }
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
            lsp_input_timing: LspInputTimingConfig::default(),
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
