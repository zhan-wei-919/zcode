use crate::kernel::language::adapter::default::DEFAULT_COMPLETION;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    language_features, CompletionBehavior, LanguageAdapter, LanguageFeatures,
};
use crate::kernel::language::LanguageId;

pub(crate) struct JsLanguageAdapter {
    language: LanguageId,
}

impl JsLanguageAdapter {
    const fn new(language: LanguageId) -> Self {
        Self { language }
    }
}

impl LanguageAdapter for JsLanguageAdapter {
    fn completion(&self) -> &dyn CompletionBehavior {
        &DEFAULT_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(self.language))
    }
}

pub(crate) static JS_ADAPTER: JsLanguageAdapter = JsLanguageAdapter::new(LanguageId::JavaScript);
pub(crate) static TS_ADAPTER: JsLanguageAdapter = JsLanguageAdapter::new(LanguageId::TypeScript);
pub(crate) static JSX_ADAPTER: JsLanguageAdapter = JsLanguageAdapter::new(LanguageId::Jsx);
pub(crate) static TSX_ADAPTER: JsLanguageAdapter = JsLanguageAdapter::new(LanguageId::Tsx);
