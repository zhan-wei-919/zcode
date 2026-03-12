use crate::kernel::language::adapter::editing::RUST_EDITING_POLICY;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    default_normalize_completion_item, language_features, CompletionBehavior, CompletionContext,
    LanguageAdapter, LanguageEditingPolicy, LanguageFeatures, TextEditPlan, TextEditStrategy,
};
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::LspInsertTextFormat;

pub(crate) struct RustCompletionBehavior;

impl CompletionBehavior for RustCompletionBehavior {
    fn normalize_completion_item(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        rust_enhanced_keyword_snippet(context)
            .unwrap_or_else(|| default_normalize_completion_item(context))
    }
}

fn rust_enhanced_keyword_snippet(context: &CompletionContext<'_>) -> Option<TextEditPlan> {
    let item = context.item;
    if item.label.trim() != "fn" {
        return None;
    }

    let raw_insert = item.insert_text.replace("\r\n", "\n");
    let is_plain_keyword = matches!(item.insert_text_format, LspInsertTextFormat::PlainText)
        && raw_insert.trim() == "fn";
    let is_rust_analyzer_fn_snippet =
        matches!(item.insert_text_format, LspInsertTextFormat::Snippet)
            && raw_insert.starts_with("fn ")
            && raw_insert.contains("$1(")
            && raw_insert.contains("$2)")
            && raw_insert.contains("$0")
            && !raw_insert.contains("->");

    if !is_plain_keyword && !is_rust_analyzer_fn_snippet {
        return None;
    }

    let mut plan = TextEditPlan::from_snippet("fn ${1:name}(${2:args}) -> ${3:Ret} {\n    $0\n}");
    plan.strategy = TextEditStrategy::SynthesizedSnippet;
    Some(plan)
}

pub(crate) static RUST_COMPLETION: RustCompletionBehavior = RustCompletionBehavior;

pub(crate) struct RustLanguageAdapter;

impl LanguageAdapter for RustLanguageAdapter {
    fn interaction(&self) -> &dyn crate::kernel::language::adapter::LanguageInteractionPolicy {
        &RUST_COMPLETION
    }

    fn completion_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::CompletionProtocolAdapter {
        &RUST_COMPLETION
    }

    fn signature_help_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::SignatureHelpProtocolAdapter {
        &RUST_COMPLETION
    }

    fn hover_protocol(&self) -> &dyn crate::kernel::language::adapter::HoverProtocolAdapter {
        &RUST_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn editing(&self) -> &dyn LanguageEditingPolicy {
        &RUST_EDITING_POLICY
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(LanguageId::Rust))
    }
}

pub(crate) static RUST_ADAPTER: RustLanguageAdapter = RustLanguageAdapter;
