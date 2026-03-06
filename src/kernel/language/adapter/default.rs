use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    language_features, CompletionBehavior, LanguageAdapter, LanguageFeatures,
};
use crate::kernel::language::LanguageId;

pub(crate) struct DefaultCompletionBehavior;

impl CompletionBehavior for DefaultCompletionBehavior {}

pub(crate) static DEFAULT_COMPLETION: DefaultCompletionBehavior = DefaultCompletionBehavior;

pub(crate) struct DefaultLanguageAdapter {
    language: Option<LanguageId>,
}

impl DefaultLanguageAdapter {
    pub(crate) const fn new(language: Option<LanguageId>) -> Self {
        Self { language }
    }
}

impl LanguageAdapter for DefaultLanguageAdapter {
    fn completion(&self) -> &dyn CompletionBehavior {
        &DEFAULT_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn features(&self) -> LanguageFeatures {
        language_features(self.language)
    }
}

pub(crate) static DEFAULT_ADAPTER: DefaultLanguageAdapter = DefaultLanguageAdapter::new(None);
pub(crate) static JAVA_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Java));
pub(crate) static JSON_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Json));
pub(crate) static YAML_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Yaml));
pub(crate) static HTML_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Html));
pub(crate) static XML_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Xml));
pub(crate) static CSS_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Css));
pub(crate) static TOML_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Toml));
pub(crate) static SQL_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Sql));
pub(crate) static BASH_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Bash));
pub(crate) static MARKDOWN_ADAPTER: DefaultLanguageAdapter =
    DefaultLanguageAdapter::new(Some(LanguageId::Markdown));
