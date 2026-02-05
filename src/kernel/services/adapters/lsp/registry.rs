use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LspLanguage {
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
}

impl LspLanguage {
    pub(super) fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|s| s.to_str())? {
            "rs" => Some(Self::Rust),
            "go" => Some(Self::Go),
            "py" | "pyi" => Some(Self::Python),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "jsx" => Some(Self::Jsx),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    pub(super) fn language_id(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Jsx => "javascriptreact",
            Self::Tsx => "typescriptreact",
        }
    }

    pub(super) fn markers(self) -> &'static [&'static str] {
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
        }
    }

    pub(super) fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Jsx => "JSX",
            Self::Tsx => "TSX",
        }
    }
}

pub(super) fn language_root_for_file(
    workspace_root: &Path,
    language: LspLanguage,
    file_path: &Path,
) -> PathBuf {
    let Some(start_dir) = file_path.parent() else {
        return workspace_root.to_path_buf();
    };

    if !file_path.starts_with(workspace_root) {
        return workspace_root.to_path_buf();
    }

    let markers = language.markers();
    find_nearest_marker_root(workspace_root, start_dir, markers)
}

fn find_nearest_marker_root(workspace_root: &Path, start_dir: &Path, markers: &[&str]) -> PathBuf {
    let mut cur = start_dir;
    loop {
        if cur.starts_with(workspace_root)
            && markers
                .iter()
                .any(|name| cur.join(name).try_exists().unwrap_or(false))
        {
            return cur.to_path_buf();
        }

        if cur == workspace_root {
            break;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => break,
        }
    }

    workspace_root.to_path_buf()
}
