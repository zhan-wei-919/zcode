use super::*;

#[test]
fn test_lsp_input_timing_defaults() {
    let config = EditorConfig::default();
    let timing = &config.lsp_input_timing;

    assert_eq!(timing.boundary_chars, " \t\n.,;:()[]{}");
    assert!(timing.boundary_immediate);

    assert_eq!(timing.identifier_debounce_ms.completion, 240);
    assert_eq!(timing.identifier_debounce_ms.semantic_tokens, 360);
    assert_eq!(timing.identifier_debounce_ms.inlay_hints, 420);
    assert_eq!(timing.identifier_debounce_ms.folding_range, 480);

    assert_eq!(timing.delete_debounce_ms.completion, 120);
    assert_eq!(timing.delete_debounce_ms.semantic_tokens, 140);
    assert_eq!(timing.delete_debounce_ms.inlay_hints, 180);
    assert_eq!(timing.delete_debounce_ms.folding_range, 220);
}

#[test]
fn test_lsp_input_timing_serde_aliases() {
    #[derive(serde::Deserialize)]
    struct Wrapper {
        editor: EditorConfig,
    }

    let data = r#"{
      "editor": {
        "lspInputTiming": {
          "boundaryChars": ",.;",
          "boundaryImmediate": false,
          "identifierDebounceMs": {
            "completion": 11,
            "semantic_tokens": 22,
            "inlay_hints": 33,
            "folding_range": 44
          },
          "deleteDebounceMs": {
            "completion": 55,
            "semantic_tokens": 66,
            "inlay_hints": 77,
            "folding_range": 88
          }
        }
      }
    }"#;

    let parsed: Wrapper = serde_json::from_str(data).expect("parse settings");
    let timing = parsed.editor.lsp_input_timing;
    assert_eq!(timing.boundary_chars, ",.;");
    assert!(!timing.boundary_immediate);
    assert_eq!(timing.identifier_debounce_ms.completion, 11);
    assert_eq!(timing.identifier_debounce_ms.semantic_tokens, 22);
    assert_eq!(timing.identifier_debounce_ms.inlay_hints, 33);
    assert_eq!(timing.identifier_debounce_ms.folding_range, 44);
    assert_eq!(timing.delete_debounce_ms.completion, 55);
    assert_eq!(timing.delete_debounce_ms.semantic_tokens, 66);
    assert_eq!(timing.delete_debounce_ms.inlay_hints, 77);
    assert_eq!(timing.delete_debounce_ms.folding_range, 88);
}

#[test]
fn test_default_config() {
    let config = EditorConfig::default();
    assert_eq!(config.tab_size, 4);
    assert!(config.show_line_numbers);
    assert!(config.show_indent_guides);
    assert!(!config.format_on_save);
}

#[test]
fn test_show_indent_guides_can_be_configured_from_settings_json() {
    #[derive(serde::Deserialize)]
    struct Wrapper {
        editor: EditorConfig,
    }

    let snake_case = r#"{
      "editor": {
        "show_indent_guides": false
      }
    }"#;
    let parsed: Wrapper = serde_json::from_str(snake_case).expect("parse settings snake_case");
    assert!(!parsed.editor.show_indent_guides);

    let camel_case = r#"{
      "editor": {
        "showIndentGuides": false
      }
    }"#;
    let parsed: Wrapper = serde_json::from_str(camel_case).expect("parse settings camelCase");
    assert!(!parsed.editor.show_indent_guides);
}

#[test]
fn test_scroll_step() {
    let config = EditorConfig::default();
    assert_eq!(config.scroll_step(), 1);
}
