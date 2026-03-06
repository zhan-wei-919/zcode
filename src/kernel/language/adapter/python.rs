use crate::kernel::language::adapter::default::DEFAULT_COMPLETION;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    language_features, CompletionBehavior, LanguageAdapter, LanguageFeatures,
};
use crate::kernel::language::LanguageId;

pub(crate) struct PythonLanguageAdapter;

impl LanguageAdapter for PythonLanguageAdapter {
    fn completion(&self) -> &dyn CompletionBehavior {
        &DEFAULT_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(LanguageId::Python))
    }
}

pub(crate) static PYTHON_ADAPTER: PythonLanguageAdapter = PythonLanguageAdapter;
