use crate::kernel::services::ports::LspRange;

#[derive(Debug, Clone, Copy)]
pub struct DefinitionPreviewContext<'a> {
    pub lines: &'a [&'a str],
    pub anchor_line: usize,
    pub range: Option<LspRange>,
    pub max_lines: usize,
}

pub trait DefinitionPreviewPolicy: Send + Sync {
    fn definition_window(&self, ctx: &DefinitionPreviewContext<'_>) -> Option<(usize, usize)> {
        default_definition_window(ctx)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DefaultDefinitionPreviewPolicy;

pub(crate) static DEFAULT_DEFINITION_PREVIEW_POLICY: DefaultDefinitionPreviewPolicy =
    DefaultDefinitionPreviewPolicy;

impl DefinitionPreviewPolicy for DefaultDefinitionPreviewPolicy {}

pub(crate) fn default_definition_window(
    ctx: &DefinitionPreviewContext<'_>,
) -> Option<(usize, usize)> {
    if ctx.lines.is_empty() {
        return None;
    }

    let line_count = ctx.lines.len();
    let anchor = ctx.anchor_line.min(line_count.saturating_sub(1));
    let range_window = ctx
        .range
        .and_then(|range| line_window_from_range(range, line_count));
    let seed = range_window.map(|(start, _)| start).unwrap_or(anchor);
    let start = adjust_start_line(ctx.lines, seed);

    if let Some((_, range_end)) = range_window {
        if range_end > seed {
            return Some(clamp_window(start, range_end, ctx.max_lines, line_count));
        }
    }

    if let Some(end) = brace_scoped_end(ctx.lines, seed, ctx.max_lines) {
        return Some(clamp_window(start, end, ctx.max_lines, line_count));
    }

    let end = indentation_end(ctx.lines, seed, ctx.max_lines);
    Some(clamp_window(start, end, ctx.max_lines, line_count))
}

fn line_window_from_range(range: LspRange, line_count: usize) -> Option<(usize, usize)> {
    if line_count == 0 {
        return None;
    }

    let start = (range.start.line as usize).min(line_count.saturating_sub(1));
    let mut end = (range.end.line as usize).min(line_count.saturating_sub(1));
    if range.end.line > range.start.line && range.end.character == 0 {
        end = end.saturating_sub(1);
    }
    if end < start {
        end = start;
    }
    Some((start, end))
}

fn clamp_window(start: usize, end: usize, max_lines: usize, line_count: usize) -> (usize, usize) {
    let start = start.min(line_count.saturating_sub(1));
    let mut end = end.min(line_count.saturating_sub(1));
    if end < start {
        end = start;
    }
    let max_end = start
        .saturating_add(max_lines.saturating_sub(1))
        .min(line_count.saturating_sub(1));
    if end > max_end {
        end = max_end;
    }
    (start, end)
}

fn leading_indent(line: &str) -> usize {
    line.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn is_decorator_or_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("#[")
        || trimmed.starts_with("@")
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

fn adjust_start_line(lines: &[&str], mut start: usize) -> usize {
    while start > 0 {
        let prev = lines[start - 1];
        if prev.trim().is_empty() {
            break;
        }
        if !is_decorator_or_comment(prev) {
            break;
        }
        start -= 1;
    }
    start
}

fn is_definition_continuation_line(trimmed: &str) -> bool {
    trimmed.starts_with(')')
        || trimmed.starts_with(']')
        || trimmed.starts_with("where ")
        || trimmed.starts_with("->")
        || trimmed.starts_with(':')
}

fn brace_scoped_end(lines: &[&str], start: usize, max_lines: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut seen_open = false;
    let base_indent = leading_indent(lines[start]);
    let max_end = start.saturating_add(max_lines.saturating_sub(1));
    let last = lines.len().saturating_sub(1).min(max_end);

    for (idx, line) in lines.iter().enumerate().take(last + 1).skip(start) {
        let trimmed = line.trim_start();
        if !seen_open && idx > start && !trimmed.is_empty() {
            let indent = leading_indent(line);
            if indent <= base_indent
                && !is_definition_continuation_line(trimmed)
                && !is_decorator_or_comment(line)
                && !line.contains('{')
            {
                return None;
            }
        }

        for ch in line.chars() {
            if ch == '{' {
                depth = depth.saturating_add(1);
                seen_open = true;
            } else if ch == '}' && seen_open {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
        }
    }

    if seen_open {
        Some(last)
    } else {
        None
    }
}

fn indentation_end(lines: &[&str], start: usize, max_lines: usize) -> usize {
    let base_indent = leading_indent(lines[start]);
    let mut end = start;
    let max_end = start
        .saturating_add(max_lines.saturating_sub(1))
        .min(lines.len().saturating_sub(1));

    for (idx, line) in lines.iter().enumerate().take(max_end + 1).skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            end = idx;
            continue;
        }

        let indent = leading_indent(line);
        let continuation = is_definition_continuation_line(trimmed);
        if indent > base_indent || continuation {
            end = idx;
            continue;
        }
        break;
    }

    end
}
