use super::*;
use crate::ui::backend::test::TestBackend;
use crate::ui::backend::Backend;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style;
use crate::ui::core::theme::Theme;
use crate::views::editor::markdown::{MdRenderedLine, MdSpanKind, MdStyleSpan};
use std::hint::black_box;
use std::time::Instant;

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
            .spans
            .iter()
            .any(|s| matches!(s.kind, DocSpanKind::Syntax(HighlightKind::Keyword))),
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

    assert!(lines.len() >= 2);
    // First line preserves the original indentation.
    assert!(lines[0].text.starts_with("    "));
    // Continuation lines contain wrapped content (no forced indent repetition).
    assert!(!lines[1].text.is_empty());
}

#[test]
fn natural_width_ignores_fence_markers() {
    let md = "```rust\nabc   \n```\n";
    assert_eq!(natural_width(md), 3);
}

#[test]
fn paint_doc_line_expands_tabs_in_code_blocks() {
    let md = "```go\n\tlog.Fatal(err)\n```";
    let lines = render_markdown(md, 80, 50);
    let line = lines
        .iter()
        .find(|l| l.text.contains("log.Fatal"))
        .expect("missing code line");

    let theme = Theme::default();
    let base_style = Style::default().bg(theme.popup_bg).fg(theme.palette_fg);

    let area = Rect::new(0, 0, 40, 1);
    let mut painter = Painter::new();
    paint_doc_line(
        &mut painter,
        Pos::new(0, 0),
        area.w,
        DocPaintLineParams {
            line,
            theme: &theme,
            base_style,
            clip: area,
            horiz_offset: 0,
            tab_size: 4,
        },
    );

    let mut backend = TestBackend::new(area.w, area.h);
    backend.draw(area, painter.cmds());

    // The tab expands to 4 spaces (tab_size=4). We must not emit a literal '\t'
    // because terminals treat it as a cursor movement control char.
    for x in 0..4u16 {
        let cell = backend.buffer().cell(x, 0).expect("missing cell");
        assert_eq!(cell.symbol, " ");
    }
    let cell = backend.buffer().cell(4, 0).expect("missing cell");
    assert_eq!(cell.symbol, "l");
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
        .spans
        .iter()
        .any(|s| matches!(s.kind, DocSpanKind::Syntax(HighlightKind::Keyword))));

    let cpp_line = lines
        .iter()
        .find(|l| l.text.contains("class B"))
        .expect("missing cpp line");
    assert!(cpp_line
        .spans
        .iter()
        .any(|s| matches!(s.kind, DocSpanKind::Syntax(HighlightKind::Keyword))));
}

#[test]
fn clamp_scroll_offset_limits_scroll_range() {
    assert_eq!(clamp_scroll_offset(0, 10, 3), 0);
    assert_eq!(clamp_scroll_offset(7, 10, 3), 7);
    assert_eq!(clamp_scroll_offset(100, 10, 3), 7);
    assert_eq!(clamp_scroll_offset(5, 0, 3), 0);
    assert_eq!(clamp_scroll_offset(5, 10, 0), 0);
}

#[test]
fn from_markdown_rendered_preserves_text_spans_and_offset_map() {
    let rendered = MdRenderedLine {
        text: "title".to_string(),
        spans: vec![MdStyleSpan {
            start: 0,
            end: 5,
            kind: MdSpanKind::Heading(2),
        }],
        offset_map: vec![(0, 2), (5, 7)],
    };

    let line = from_markdown_rendered(rendered);
    assert_eq!(line.text, "title");
    assert_eq!(line.offset_map.as_deref(), Some(&[(0, 2), (5, 7)][..]));
    assert!(line
        .spans
        .iter()
        .any(|s| matches!(s.kind, DocSpanKind::Markdown(MdSpanKind::Heading(2)))));
}

#[test]
fn experiment_doc_render_cache_benefit_baseline() {
    let doc = r#"
### SecurityFilterChain(HttpSecurity)

`HttpSecurity` allows configuring web based security for specific http requests.

```java
@Bean
public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(auth -> auth
        .requestMatchers("/actuator/health").permitAll()
        .anyRequest().authenticated());
    return http.build();
}
```

Additional notes:
- Uses a builder-like API.
- Commonly combined with request matchers and CSRF/session options.
- Type info and examples can be long in hover/completion docs.
"#;

    let width = 76u16;
    let loops = 3_000usize;
    let max_lines = MAX_RENDER_LINES;

    let uncached_start = Instant::now();
    let mut uncached_lines = 0usize;
    for _ in 0..loops {
        let rendered = render_markdown(black_box(doc), width, max_lines);
        uncached_lines = uncached_lines.saturating_add(rendered.len());
        black_box(rendered);
    }
    let uncached_elapsed = uncached_start.elapsed();

    let cached_start = Instant::now();
    let mut cache = RenderCache::default();
    let mut cached_lines = 0usize;
    let mut cache_hits = 0usize;
    for _ in 0..loops {
        let (_key, rendered, hit) = cache.get_or_render(black_box(doc), width, max_lines);
        cached_lines = cached_lines.saturating_add(rendered.len());
        cache_hits += usize::from(hit);
        black_box(rendered.len());
    }
    let cached_elapsed = cached_start.elapsed();

    let uncached_us = uncached_elapsed.as_secs_f64() * 1_000_000.0;
    let cached_us = cached_elapsed.as_secs_f64() * 1_000_000.0;
    let speedup = if cached_us > 0.0 {
        uncached_us / cached_us
    } else {
        0.0
    };
    let saved_percent = if uncached_us > 0.0 {
        (1.0 - (cached_us / uncached_us)) * 100.0
    } else {
        0.0
    };

    eprintln!(
        "[experiment] doc_render_cache loops={} width={} uncached_us={:.2} cached_us={:.2} speedup_x={:.2} saved_percent={:.1} cache_hits={} ({:.1}%)",
        loops,
        width,
        uncached_us,
        cached_us,
        speedup,
        saved_percent,
        cache_hits,
        (cache_hits as f64) * 100.0 / (loops as f64)
    );

    assert_eq!(uncached_lines, cached_lines);
    assert!(cache_hits >= loops.saturating_sub(1));
}
