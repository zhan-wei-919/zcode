use std::path::{Path, PathBuf};

use crate::kernel::lsp_registry::LspLanguage;

/// Resolve the `rust-analyzer` executable path on this machine.
///
/// Order:
/// 1) `$PATH`
/// 2) `$CARGO_HOME/bin` (or `$HOME/.cargo/bin`)
/// 3) `rustup which rust-analyzer` (using `root` as `current_dir` to honor overrides)
pub(super) fn resolve_rust_analyzer_command(root: &Path) -> Option<String> {
    let from_path = find_in_path("rust-analyzer");
    if let Some(path) = from_path {
        return Some(path.to_string_lossy().to_string());
    }

    let from_cargo_home = cargo_home_bin_path("rust-analyzer");
    if let Some(path) = from_cargo_home.filter(|p| is_executable_file(p)) {
        return Some(path.to_string_lossy().to_string());
    }

    rustup_which(root, "rust-analyzer").and_then(|path| {
        if is_executable_file(&path) {
            Some(path.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

pub(super) fn resolve_default_server_command(
    workspace_root: &Path,
    root: &Path,
    language: LspLanguage,
) -> Option<(String, Vec<String>)> {
    match language {
        LspLanguage::Rust => resolve_rust_analyzer_command(root).map(|cmd| (cmd, Vec::new())),
        LspLanguage::Go => resolve_gopls_command().map(|cmd| (cmd, Vec::new())),
        LspLanguage::Python => resolve_pyright_langserver_command(workspace_root, root)
            .map(|cmd| (cmd, vec!["--stdio".to_string()])),
        LspLanguage::JavaScript | LspLanguage::TypeScript | LspLanguage::Jsx | LspLanguage::Tsx => {
            resolve_typescript_language_server_command(workspace_root, root)
                .map(|cmd| (cmd, vec!["--stdio".to_string()]))
        }
    }
}

fn resolve_gopls_command() -> Option<String> {
    find_in_path("gopls")
        .or_else(|| resolve_gobin_command("gopls"))
        .or_else(|| resolve_gopath_command("gopls"))
        .map(|path| path.to_string_lossy().to_string())
}

fn resolve_typescript_language_server_command(
    workspace_root: &Path,
    root: &Path,
) -> Option<String> {
    resolve_node_modules_bin(workspace_root, root, "typescript-language-server")
        .or_else(|| find_in_path("typescript-language-server"))
        .map(|path| path.to_string_lossy().to_string())
}

fn resolve_pyright_langserver_command(workspace_root: &Path, root: &Path) -> Option<String> {
    resolve_node_modules_bin(workspace_root, root, "pyright-langserver")
        .or_else(|| resolve_venv_bin(workspace_root, root, "pyright-langserver"))
        .or_else(|| find_in_path("pyright-langserver"))
        .map(|path| path.to_string_lossy().to_string())
}

fn resolve_gobin_command(name: &str) -> Option<PathBuf> {
    let gobin = std::env::var_os("GOBIN")?;
    if gobin.is_empty() {
        return None;
    }

    executable_in_dir(&PathBuf::from(gobin), name)
}

fn resolve_gopath_command(name: &str) -> Option<PathBuf> {
    if let Some(gopath) = std::env::var_os("GOPATH") {
        for dir in std::env::split_paths(&gopath) {
            if dir.as_os_str().is_empty() {
                continue;
            }

            if let Some(path) = executable_in_dir(&dir.join("bin"), name) {
                return Some(path);
            }
        }
    }

    let home = std::env::var_os("HOME")?;
    executable_in_dir(&PathBuf::from(home).join("go").join("bin"), name)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .find_map(|dir| executable_in_dir(&dir, name))
}

fn executable_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    for exe in candidate_names(name) {
        let candidate = dir.join(exe);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_node_modules_bin(workspace_root: &Path, root: &Path, name: &str) -> Option<PathBuf> {
    if !root.starts_with(workspace_root) {
        return None;
    }

    // Support monorepo setups where the LSP server is installed at a higher-level
    // `node_modules/.bin` (e.g. workspace root), even if the language root is nested.
    let mut cur = root;
    loop {
        let bin_dir = cur.join("node_modules").join(".bin");
        for exe in candidate_names(name) {
            let candidate = bin_dir.join(exe);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }

        if cur == workspace_root {
            break;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => break,
        }
    }

    None
}

fn resolve_venv_bin(workspace_root: &Path, root: &Path, name: &str) -> Option<PathBuf> {
    if !root.starts_with(workspace_root) {
        return None;
    }

    #[cfg(unix)]
    {
        let mut cur = root;
        loop {
            for venv in [".venv", "venv", ".env"] {
                let bin = cur.join(venv).join("bin");
                let candidate = bin.join(name);
                if is_executable_file(&candidate) {
                    return Some(candidate);
                }
            }

            if cur == workspace_root {
                break;
            }
            match cur.parent() {
                Some(parent) => cur = parent,
                None => break,
            }
        }
    }

    #[cfg(windows)]
    {
        let mut cur = root;
        loop {
            for venv in [".venv", "venv", ".env"] {
                let bin = cur.join(venv).join("Scripts");
                for exe in candidate_names(name) {
                    let candidate = bin.join(exe);
                    if is_executable_file(&candidate) {
                        return Some(candidate);
                    }
                }
            }

            if cur == workspace_root {
                break;
            }
            match cur.parent() {
                Some(parent) => cur = parent,
                None => break,
            }
        }
    }

    None
}

fn cargo_home_bin_path(name: &str) -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))?;

    Some(cargo_home.join("bin").join(name))
}

fn rustup_which(root: &Path, name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("rustup")
        .arg("which")
        .arg(name)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next()?.trim();
    if first_line.is_empty() {
        return None;
    }
    Some(PathBuf::from(first_line))
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        meta.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn candidate_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            name.to_string(),
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
        ]
    }

    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}
