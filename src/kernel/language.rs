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
            _ => None,
        }
    }

    pub fn server_kind(self) -> LspServerKind {
        match self {
            Self::Rust => LspServerKind::RustAnalyzer,
            Self::Go => LspServerKind::Gopls,
            Self::Python => LspServerKind::Pyright,
            Self::JavaScript | Self::TypeScript | Self::Jsx | Self::Tsx => {
                LspServerKind::TypeScriptLanguageServer
            }
            Self::C | Self::Cpp => LspServerKind::Clangd,
            Self::Java => LspServerKind::Jdtls,
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
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/language.rs"]
mod tests;
