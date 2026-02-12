use std::path::Path;

use crate::kernel::services::ports::LspServerKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LanguageId {
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
    C,
    Cpp,
    Java,
    Json,
    Yaml,
    Html,
    Xml,
    Css,
    Toml,
    Bash,
    Markdown,
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|s| s.to_str())? {
            "rs" => Some(Self::Rust),
            "go" => Some(Self::Go),
            "py" | "pyi" => Some(Self::Python),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "jsx" => Some(Self::Jsx),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "c" => Some(Self::C),
            "cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "h" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "html" | "htm" => Some(Self::Html),
            "xml" | "xsl" | "xslt" | "svg" => Some(Self::Xml),
            "css" => Some(Self::Css),
            "toml" => Some(Self::Toml),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "md" | "markdown" | "mdx" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub fn server_kind(self) -> Option<LspServerKind> {
        match self {
            Self::Rust => Some(LspServerKind::RustAnalyzer),
            Self::Go => Some(LspServerKind::Gopls),
            Self::Python => Some(LspServerKind::Pyright),
            Self::JavaScript | Self::TypeScript | Self::Jsx | Self::Tsx => {
                Some(LspServerKind::TypeScriptLanguageServer)
            }
            Self::C | Self::Cpp => Some(LspServerKind::Clangd),
            Self::Java => Some(LspServerKind::Jdtls),
            Self::Json
            | Self::Yaml
            | Self::Html
            | Self::Xml
            | Self::Css
            | Self::Toml
            | Self::Bash
            | Self::Markdown => None,
        }
    }

    pub fn language_id(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Jsx => "javascriptreact",
            Self::Tsx => "typescriptreact",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Html => "html",
            Self::Xml => "xml",
            Self::Css => "css",
            Self::Toml => "toml",
            Self::Bash => "shellscript",
            Self::Markdown => "markdown",
        }
    }

    pub fn markers(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["Cargo.toml", "rust-project.json"],
            Self::Go => &["go.work", "go.mod"],
            Self::Python => &[
                "pyproject.toml",
                "pyrightconfig.json",
                "setup.py",
                "setup.cfg",
                "requirements.txt",
            ],
            Self::JavaScript | Self::TypeScript | Self::Jsx | Self::Tsx => {
                &["tsconfig.json", "jsconfig.json", "package.json"]
            }
            Self::C | Self::Cpp => &[
                "compile_commands.json",
                "compile_flags.txt",
                "CMakeLists.txt",
                "meson.build",
                "Makefile",
            ],
            Self::Java => &[
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "settings.gradle.kts",
                "gradlew",
                ".project",
            ],
            Self::Json
            | Self::Yaml
            | Self::Html
            | Self::Xml
            | Self::Css
            | Self::Toml
            | Self::Bash
            | Self::Markdown => &[],
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Jsx => "JSX",
            Self::Tsx => "TSX",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Java => "Java",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Html => "HTML",
            Self::Xml => "XML",
            Self::Css => "CSS",
            Self::Toml => "TOML",
            Self::Bash => "Bash",
            Self::Markdown => "Markdown",
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/language.rs"]
mod tests;
