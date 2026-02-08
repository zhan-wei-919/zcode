use super::*;

#[test]
fn render_markdown_code_fence_preserves_indent_and_highlight() {
    let md = r#"Some text

```rust
struct StartupPaths {
    root: PathBuf,
    open_file: Option<PathBuf>,
}
```

More text
"#;

    let lines = render_markdown(md, 80, 100);
    assert!(!lines.is_empty());

    let code_first = lines
        .iter()
        .find(|l| l.text.contains("struct StartupPaths"))
        .expect("missing code block header line");
    assert!(
        code_first
            .highlight
            .as_ref()
            .is_some_and(|spans| spans.iter().any(|s| s.kind == HighlightKind::Keyword)),
        "expected keyword highlight on `struct`"
    );

    let field_line = lines
        .iter()
        .find(|l| l.text.contains("root: PathBuf"))
        .expect("missing indented field line");
    assert!(field_line.text.starts_with("    "));
}

#[test]
fn render_markdown_wraps_text_and_keeps_indentation() {
    let md = "    this is a long indented line that should wrap";
    let lines = render_markdown(md, 20, 10);

    // At least two wrapped lines, each keeping the original indentation prefix.
    assert!(lines.len() >= 2);
    assert!(lines.iter().all(|l| l.text.starts_with("    ")));
}

#[test]
fn natural_width_ignores_fence_markers() {
    let md = "```rust\nabc   \n```\n";
    assert_eq!(natural_width(md), 3);
}

#[test]
fn render_markdown_java_cpp_fences_apply_highlight() {
    let md = r#"```java
public class A {}
```

```cpp
class B {};
```
"#;

    let lines = render_markdown(md, 120, 20);

    let java_line = lines
        .iter()
        .find(|l| l.text.contains("public class"))
        .expect("missing java line");
    assert!(java_line
        .highlight
        .as_ref()
        .is_some_and(|spans| spans.iter().any(|s| s.kind == HighlightKind::Keyword)));

    let cpp_line = lines
        .iter()
        .find(|l| l.text.contains("class B"))
        .expect("missing cpp line");
    assert!(cpp_line
        .highlight
        .as_ref()
        .is_some_and(|spans| spans.iter().any(|s| s.kind == HighlightKind::Keyword)));
}

#[test]
fn clamp_scroll_offset_limits_scroll_range() {
    assert_eq!(clamp_scroll_offset(0, 10, 3), 0);
    assert_eq!(clamp_scroll_offset(7, 10, 3), 7);
    assert_eq!(clamp_scroll_offset(100, 10, 3), 7);
    assert_eq!(clamp_scroll_offset(5, 0, 3), 0);
    assert_eq!(clamp_scroll_offset(5, 10, 0), 0);
}
