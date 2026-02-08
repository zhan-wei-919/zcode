use crate::core::Command;
use crate::kernel::services::ports::config::LspInputTimingConfig;
use std::time::Duration;

mod bottom_panel;
mod command;
mod completion;
mod editor;
mod explorer;
mod key;
mod search;
mod terminal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LspDebouncePipeline {
    SemanticTokens,
    InlayHints,
    FoldingRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LspDebounceTrigger {
    Immediate,
    Identifier,
    Delete,
}

pub(super) fn classify_lsp_edit_trigger(
    cmd: &Command,
    timing: &LspInputTimingConfig,
) -> Option<LspDebounceTrigger> {
    match cmd {
        Command::InsertChar(ch) => {
            if timing.boundary_immediate && is_lsp_boundary_char(*ch, &timing.boundary_chars) {
                Some(LspDebounceTrigger::Immediate)
            } else {
                Some(LspDebounceTrigger::Identifier)
            }
        }
        Command::DeleteBackward | Command::DeleteForward => Some(LspDebounceTrigger::Delete),
        Command::InsertNewline
        | Command::InsertTab
        | Command::DeleteLine
        | Command::DeleteToLineEnd
        | Command::DeleteSelection
        | Command::Undo
        | Command::Redo
        | Command::Paste
        | Command::Cut => Some(LspDebounceTrigger::Identifier),
        _ => None,
    }
}

pub(super) fn lsp_debounce_duration(
    timing: &LspInputTimingConfig,
    pipeline: LspDebouncePipeline,
    trigger: LspDebounceTrigger,
) -> Duration {
    let millis = match trigger {
        LspDebounceTrigger::Immediate => 0,
        LspDebounceTrigger::Identifier => match pipeline {
            LspDebouncePipeline::SemanticTokens => timing.identifier_debounce_ms.semantic_tokens,
            LspDebouncePipeline::InlayHints => timing.identifier_debounce_ms.inlay_hints,
            LspDebouncePipeline::FoldingRange => timing.identifier_debounce_ms.folding_range,
        },
        LspDebounceTrigger::Delete => match pipeline {
            LspDebouncePipeline::SemanticTokens => timing.delete_debounce_ms.semantic_tokens,
            LspDebouncePipeline::InlayHints => timing.delete_debounce_ms.inlay_hints,
            LspDebouncePipeline::FoldingRange => timing.delete_debounce_ms.folding_range,
        },
    };
    Duration::from_millis(millis)
}

fn is_lsp_boundary_char(ch: char, boundary_chars: &str) -> bool {
    boundary_chars.chars().any(|boundary| boundary == ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_uses_boundary_immediate_when_enabled() {
        let timing = LspInputTimingConfig {
            boundary_chars: ".".to_string(),
            boundary_immediate: true,
            ..Default::default()
        };

        let trigger = classify_lsp_edit_trigger(&Command::InsertChar('.'), &timing);
        assert_eq!(trigger, Some(LspDebounceTrigger::Immediate));
    }

    #[test]
    fn classify_uses_identifier_when_boundary_immediate_disabled() {
        let timing = LspInputTimingConfig {
            boundary_chars: ".".to_string(),
            boundary_immediate: false,
            ..Default::default()
        };

        let trigger = classify_lsp_edit_trigger(&Command::InsertChar('.'), &timing);
        assert_eq!(trigger, Some(LspDebounceTrigger::Identifier));
    }

    #[test]
    fn debounce_duration_reads_pipeline_specific_values() {
        let timing = LspInputTimingConfig::default();

        let semantic = lsp_debounce_duration(
            &timing,
            LspDebouncePipeline::SemanticTokens,
            LspDebounceTrigger::Identifier,
        );
        let inlay_delete = lsp_debounce_duration(
            &timing,
            LspDebouncePipeline::InlayHints,
            LspDebounceTrigger::Delete,
        );

        assert_eq!(semantic, Duration::from_millis(360));
        assert_eq!(inlay_delete, Duration::from_millis(180));
    }
}
