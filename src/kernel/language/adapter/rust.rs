use crate::kernel::language::adapter::default::DEFAULT_COMPLETION;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{language_features, LanguageAdapter, LanguageFeatures};
use crate::kernel::language::LanguageId;

pub(crate) struct RustLanguageAdapter;

impl LanguageAdapter for RustLanguageAdapter {
    fn interaction(&self) -> &dyn crate::kernel::language::adapter::LanguageInteractionPolicy {
        &DEFAULT_COMPLETION
    }

    fn completion_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::CompletionProtocolAdapter {
        &DEFAULT_COMPLETION
    }

    fn signature_help_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::SignatureHelpProtocolAdapter {
        &DEFAULT_COMPLETION
    }

    fn hover_protocol(&self) -> &dyn crate::kernel::language::adapter::HoverProtocolAdapter {
        &DEFAULT_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(LanguageId::Rust))
    }
}

pub(crate) static RUST_ADAPTER: RustLanguageAdapter = RustLanguageAdapter;
