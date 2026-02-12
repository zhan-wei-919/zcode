# Markdown Showcase (Manual QA Fixture)

This file is for visually testing the editor's Markdown WYSIWYG behavior.
It intentionally includes common syntax and tricky edge cases.

## Quick Checklist

- [ ] Headings render with level-specific styles.
- [ ] Inline formatting hides markers and keeps correct range mapping.
- [ ] Code spans and fenced code blocks look distinct and readable.
- [ ] Links keep full label coloring (no split like `Licens` + `e`).
- [ ] Tables are readable and keep alignment.

## Inline Styles

Plain text with **bold**, *italic*, ~~strikethrough~~, and `inline_code`.

Adjacent markers:

- **LSP** should render bold without showing `**`.
- prefix**bold**suffix should not break surrounding text.
- mix `code` inside **bold and `inline` code**.

Backtick edge cases:

- Single ticks: `cargo test --all-targets`
- Double ticks containing a tick: ``code with ` inside``
- Triple ticks containing double ticks: ```a `` b```

Escapes and punctuation:

- Escaped brackets: \[not a link]
- Escaped paren: \(not grouped\)
- Escaped backtick: \`literal backtick\`

## Links

- [License](#license)
- [Rust Website](https://www.rust-lang.org/)
- [Nested [label] demo](https://example.com/path_(with_paren))
- [Email](mailto:test@example.com)

Inline link mix: before [link-text](#inline-styles) after.

## Lists

- Unordered item one
- Unordered item with `code` and **bold**
- Unordered item with nested list:
  - child a
  - child b

1. Ordered item one
2. Ordered item with [link](#tables)
3. Ordered item with ~~strike~~ and `code`.

## Blockquote

> Single line quote.
>
> Multi-line quote with **bold** and `code`.
> Also has a [link](#fenced-code-blocks).

## Horizontal Rule

---

## Fenced Code Blocks

```rust
fn main() {
    let msg = "hello";
    println!("{msg}");
}
```

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo test kernel::editor:: -- --nocapture
```

~~~json
{
  "name": "zcode",
  "features": ["markdown", "wysiwyg", "lsp"]
}
~~~

Fence mismatch stress case (should stay inside block until matching close):

```txt
line 1
~~~
line 2
```

## Tables

| Feature | Status | Notes |
| --- | --- | --- |
| Heading rendering | Done | H1-H6 have dedicated tokens |
| Inline code | In progress | Visual polish and range mapping |
| Fenced code | In progress | Block shape and language hint |
| Link highlight | Done | Full label should keep one color |

Alignment table:

| Left | Center | Right |
| :--- | :---: | ---: |
| a | b | c |
| 10 | 20 | 30 |

## Mixed Stress Paragraph

In one line: **bold**, *italic*, ~~strike~~, `code`, ``tick ` inside``, [link](#license), and plain text after punctuation: end.

## License

This section exists so `[License](#license)` has a local anchor target.
