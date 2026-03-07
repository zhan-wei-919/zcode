use crate::kernel::language::adapter::default::DEFAULT_COMPLETION;
use crate::kernel::language::adapter::editing::PYTHON_EDITING_POLICY;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    language_features, LanguageAdapter, LanguageEditingPolicy, LanguageFeatures,
};
use crate::kernel::language::LanguageId;

pub(crate) struct PythonLanguageAdapter;

impl LanguageAdapter for PythonLanguageAdapter {
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

    fn editing(&self) -> &dyn LanguageEditingPolicy {
        &PYTHON_EDITING_POLICY
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(LanguageId::Python))
    }
}

pub(crate) static PYTHON_ADAPTER: PythonLanguageAdapter = PythonLanguageAdapter;
