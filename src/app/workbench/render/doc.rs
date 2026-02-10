use crate::core::text_window;
use crate::kernel::editor::{highlight_snippet, HighlightKind, HighlightSpan, LanguageId};
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style;
use crate::ui::core::theme::Theme;
use unicode_width::UnicodeWidthStr;

pub(super) const MAX_RENDER_LINES: usize = 2000;

#[derive(Debug, Clone)]
enum Block {
    Text(Vec<String>),
    Code {
        language: Option<LanguageId>,
        lines: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct DocLine {
    pub(super) text: String,
    pub(super) highlight: Option<Vec<HighlightSpan>>,
}

pub(super) fn natural_width(markdown: &str) -> usize {
    markdown
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(|line| UnicodeWidthStr::width(line.trim_end()))
        .max()
        .unwrap_or(0)
}

pub(super) fn clamp_scroll_offset(scroll: usize, total_lines: usize, view_height: usize) -> usize {
    if view_height == 0 {
        return 0;
    }
    let max_scroll = total_lines.saturating_sub(view_height);
    scroll.min(max_scroll)
}

pub(super) fn paint_doc_lines(
    painter: &mut Painter,
    area: Rect,
    lines: &[DocLine],
    theme: &Theme,
    base_style: Style,
    scroll_y: usize,
) {
    if area.is_empty() || lines.is_empty() {
        return;
    }

    let view_h = area.h as usize;
    let start = scroll_y.min(lines.len());
    let end = (start + view_h).min(lines.len());

    for (i, line) in lines[start..end].iter().enumerate() {
        let y = area.y.saturating_add(i.min(u16::MAX as usize) as u16);
        if y >= area.bottom() {
            break;
        }
        let row_clip = Rect::new(area.x, y, area.w, 1);
        paint_doc_line(
            painter,
            Pos::new(area.x, y),
            area.w,
            line,
            theme,
            base_style,
            row_clip,
        );
    }
}

fn paint_doc_line(
    painter: &mut Painter,
    pos: Pos,
    width: u16,
    line: &DocLine,
    theme: &Theme,
    base_style: Style,
    clip: Rect,
) {
    let max_w = width as usize;
    if max_w == 0 {
        return;
    }

    let end = text_window::truncate_to_width(line.text.as_str(), max_w);
    let visible = line.text.get(..end).unwrap_or_default();

    let Some(spans) = line.highlight.as_deref() else {
        painter.text_clipped(pos, visible, base_style, clip);
        return;
    };

    // Paint highlighted segments. Spans are byte offsets relative to `line.text`.
    let mut x = pos.x;
    let mut cur = 0usize;
    for span in spans {
        if span.start >= span.end || span.start >= visible.len() {
            continue;
        }
        let s = span.start.min(visible.len());
        let e = span.end.min(visible.len());

        if cur < s {
            let seg = &visible[cur..s];
            painter.text_clipped(Pos::new(x, pos.y), seg, base_style, clip);
            x = x.saturating_add(seg.width().min(u16::MAX as usize) as u16);
        }

        let seg = &visible[s..e];
        let style = base_style.patch(style_for_highlight(span.kind, theme));
        painter.text_clipped(Pos::new(x, pos.y), seg, style, clip);
        x = x.saturating_add(seg.width().min(u16::MAX as usize) as u16);
        cur = e;

        if x >= clip.right() {
            return;
        }
    }

    if cur < visible.len() && x < clip.right() {
        let seg = &visible[cur..];
        painter.text_clipped(Pos::new(x, pos.y), seg, base_style, clip);
    }
}

fn style_for_highlight(kind: HighlightKind, theme: &Theme) -> Style {
    match kind {
        HighlightKind::Comment => Style::default().fg(theme.syntax_comment_fg),
        HighlightKind::String => Style::default().fg(theme.syntax_string_fg),
        HighlightKind::Regex => Style::default().fg(theme.syntax_regex_fg),
        HighlightKind::Keyword => Style::default().fg(theme.syntax_keyword_fg),
        HighlightKind::Type => Style::default().fg(theme.syntax_type_fg),
        HighlightKind::Number => Style::default().fg(theme.syntax_number_fg),
        HighlightKind::Attribute => Style::default().fg(theme.syntax_attribute_fg),
        HighlightKind::Lifetime => Style::default().fg(theme.syntax_keyword_fg),
        HighlightKind::Function => Style::default().fg(theme.syntax_function_fg),
        HighlightKind::Macro => Style::default().fg(theme.syntax_macro_fg),
        HighlightKind::Namespace => Style::default().fg(theme.syntax_namespace_fg),
        HighlightKind::Variable => Style::default().fg(theme.syntax_variable_fg),
        HighlightKind::Constant => Style::default().fg(theme.syntax_constant_fg),
    }
}

pub(super) fn render_markdown(markdown: &str, width: u16, max_lines: usize) -> Vec<DocLine> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let blocks = parse_markdown_blocks(markdown);
    let mut out: Vec<DocLine> = Vec::new();

    for block in blocks {
        if out.len() >= max_lines {
            break;
        }

        match block {
            Block::Text(lines) => {
                for line in lines {
                    if out.len() >= max_lines {
                        break;
                    }
                    wrap_and_push_text_lines(&mut out, &line, width, max_lines);
                }
            }
            Block::Code { language, lines } => {
                if lines.is_empty() {
                    continue;
                }

                // Compute highlights once per fenced block.
                let highlights = language.map(|lang| highlight_snippet(lang, &lines.join("\n")));

                for (i, line) in lines.into_iter().enumerate() {
                    if out.len() >= max_lines {
                        break;
                    }
                    let hl = highlights
                        .as_ref()
                        .and_then(|h| h.get(i))
                        .filter(|v| !v.is_empty())
                        .cloned();
                    out.push(DocLine {
                        text: line,
                        highlight: hl,
                    });
                }
            }
        }
    }

    out
}

fn wrap_and_push_text_lines(out: &mut Vec<DocLine>, line: &str, width: u16, max_lines: usize) {
    if width == 0 || out.len() >= max_lines {
        return;
    }

    if line.is_empty() {
        out.push(DocLine {
            text: String::new(),
            highlight: None,
        });
        return;
    }

    let max_w = width as usize;

    let indent_end = line
        .char_indices()
        .find(|(_, ch)| *ch != ' ' && *ch != '\t')
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    let indent = &line[..indent_end];
    let indent_w = indent.width();
    let mut rest = &line[indent_end..];

    // If the indent itself already fills the line, render a truncated indent line.
    if indent_w >= max_w {
        let end = text_window::truncate_to_width(indent, max_w);
        out.push(DocLine {
            text: indent.get(..end).unwrap_or_default().to_string(),
            highlight: None,
        });
        return;
    }

    let mut first = true;
    while out.len() < max_lines {
        let avail = max_w.saturating_sub(indent_w).max(1);
        let end = text_window::truncate_to_width(rest, avail);
        if end == 0 {
            break;
        }

        let mut part = &rest[..end];
        let mut next = &rest[end..];

        // Prefer breaking at whitespace when possible.
        if end < rest.len() {
            if let Some(ws) = part.rfind([' ', '\t']) {
                if ws > 0 {
                    part = &part[..ws];
                    next = &rest[ws..];
                }
            }
        }

        let mut text = String::new();
        text.push_str(indent);
        if first {
            // Keep original spacing after indentation on the first line.
            text.push_str(part);
        } else {
            text.push_str(part.trim_start_matches([' ', '\t']));
        }

        out.push(DocLine {
            text,
            highlight: None,
        });

        rest = next;
        rest = rest
            .strip_prefix(' ')
            .or_else(|| rest.strip_prefix('\t'))
            .unwrap_or(rest);

        if rest.is_empty() {
            break;
        }
        first = false;
    }
}

fn parse_markdown_blocks(markdown: &str) -> Vec<Block> {
    let mut out = Vec::new();

    let mut text_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut language: Option<LanguageId> = None;

    // Rough markdown: recognize ``` fences and treat the rest as text.
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if in_code {
                out.push(Block::Code {
                    language,
                    lines: std::mem::take(&mut code_lines),
                });
                in_code = false;
                language = None;
            } else {
                if !text_lines.is_empty() {
                    out.push(Block::Text(std::mem::take(&mut text_lines)));
                }
                let fence_lang = rest.split_whitespace().next().unwrap_or_default();
                language = language_id_for_fence(fence_lang);
                in_code = true;
            }
            continue;
        }

        if in_code {
            code_lines.push(line.to_string());
        } else {
            text_lines.push(line.to_string());
        }
    }

    if in_code {
        out.push(Block::Code {
            language,
            lines: code_lines,
        });
    } else if !text_lines.is_empty() {
        out.push(Block::Text(text_lines));
    }

    out
}

fn language_id_for_fence(lang: &str) -> Option<LanguageId> {
    match lang.to_ascii_lowercase().as_str() {
        "rust" | "rs" => Some(LanguageId::Rust),
        "go" => Some(LanguageId::Go),
        "python" | "py" => Some(LanguageId::Python),
        "java" => Some(LanguageId::Java),
        "c" => Some(LanguageId::C),
        "cpp" | "c++" | "cc" | "cxx" => Some(LanguageId::Cpp),
        "javascript" | "js" => Some(LanguageId::JavaScript),
        "jsx" | "javascriptreact" => Some(LanguageId::Jsx),
        "typescript" | "ts" => Some(LanguageId::TypeScript),
        "tsx" | "typescriptreact" => Some(LanguageId::Tsx),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/app/workbench/render/doc.rs"]
mod tests;
