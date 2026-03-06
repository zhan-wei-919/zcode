use ropey::Rope;

use crate::kernel::editor::syntax::{
    identifier_bounds_at, is_comment_kind, is_string_kind, line_directive_context_at,
    member_access_context_at, SyntaxDirectiveContext, SyntaxIncludeDelimiter,
    SyntaxMemberAccessKind,
};
use crate::kernel::editor::EditorTabState;
use crate::kernel::language::adapter::{
    IncludeContext, IncludeDelimiter, LineContext, MemberAccessKind, SyntaxBehavior, SyntaxFacts,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SyntaxBridge;

pub(crate) static SYNTAX_BRIDGE: SyntaxBridge = SyntaxBridge;

pub(crate) fn syntax_facts_for_tab(tab: &EditorTabState) -> SyntaxFacts {
    SYNTAX_BRIDGE.syntax_facts(tab)
}

impl SyntaxBehavior for SyntaxBridge {
    fn syntax_facts(&self, tab: &EditorTabState) -> SyntaxFacts {
        let rope = tab.buffer.rope();
        let char_offset = crate::kernel::language::adapter::cursor_char_offset(tab);
        let (in_string, in_comment) = token_context(tab, rope, char_offset);
        let syntax = tab.syntax();

        SyntaxFacts {
            in_string,
            in_comment,
            identifier_bounds: syntax
                .and_then(|syntax| syntax.identifier_bounds_at(rope, char_offset))
                .or_else(|| identifier_bounds_at(rope, char_offset)),
            member_access_kind: syntax
                .and_then(|syntax| syntax.member_access_context_at(rope, char_offset))
                .or_else(|| member_access_context_at(rope, char_offset))
                .map(map_member_access),
            line_context: map_line_context(
                syntax
                    .map(|syntax| syntax.line_directive_context_at(rope, char_offset))
                    .unwrap_or_else(|| {
                        line_directive_context_at(tab.language(), rope, char_offset)
                    }),
            ),
        }
    }
}

fn token_context(tab: &EditorTabState, rope: &Rope, char_offset: usize) -> (bool, bool) {
    let Some(syntax) = tab.syntax() else {
        return (false, false);
    };

    let byte_offset = rope.char_to_byte(char_offset.min(rope.len_chars()));
    let root = syntax.tree().root_node();
    let Some(mut node) = root.descendant_for_byte_range(byte_offset, byte_offset) else {
        return (false, false);
    };

    loop {
        let kind = node.kind();
        if is_comment_kind(kind) {
            return (false, true);
        }
        if is_string_kind(kind) {
            return (true, false);
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return (false, false),
        }
    }
}

fn map_member_access(kind: SyntaxMemberAccessKind) -> MemberAccessKind {
    match kind {
        SyntaxMemberAccessKind::Dot => MemberAccessKind::Dot,
        SyntaxMemberAccessKind::Scope => MemberAccessKind::Scope,
        SyntaxMemberAccessKind::Arrow => MemberAccessKind::Arrow,
    }
}

fn map_line_context(context: SyntaxDirectiveContext) -> LineContext {
    match context {
        SyntaxDirectiveContext::None => LineContext::None,
        SyntaxDirectiveContext::Directive => LineContext::Directive,
        SyntaxDirectiveContext::Import => LineContext::Import,
        SyntaxDirectiveContext::Include { bounds, delimiter } => {
            LineContext::Include(IncludeContext {
                bounds,
                delimiter: delimiter.map(map_include_delimiter),
            })
        }
    }
}

fn map_include_delimiter(delimiter: SyntaxIncludeDelimiter) -> IncludeDelimiter {
    match delimiter {
        SyntaxIncludeDelimiter::Angle => IncludeDelimiter::Angle,
        SyntaxIncludeDelimiter::Quote => IncludeDelimiter::Quote,
    }
}
