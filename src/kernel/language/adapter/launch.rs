use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::LspServerKind;

#[derive(Debug, Clone, Copy)]
pub struct LspLaunchContext<'a> {
    pub workspace_root: &'a Path,
    pub language_root: &'a Path,
    pub language: LanguageId,
    pub server: LspServerKind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LspLaunchPlan {
    pub command: Option<String>,
    pub args: Vec<String>,
    pub initialization_options: Option<Value>,
    pub install_hint: &'static str,
}

pub trait LspLaunchPolicy: Send + Sync {
    fn default_launch_plan(&self, ctx: &LspLaunchContext<'_>) -> LspLaunchPlan;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmptyLspLaunchPolicy;

impl LspLaunchPolicy for EmptyLspLaunchPolicy {
    fn default_launch_plan(&self, _ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        LspLaunchPlan::default()
    }
}

#[derive(Debug, Clone, Copy)]
struct RustAnalyzerLspLaunchPolicy;

impl LspLaunchPolicy for RustAnalyzerLspLaunchPolicy {
    fn default_launch_plan(&self, ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_rust_analyzer_command(ctx.language_root),
            &[],
            None,
            "install rust-analyzer (e.g. `rustup component add rust-analyzer`)",
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct GoplsLspLaunchPolicy;

impl LspLaunchPolicy for GoplsLspLaunchPolicy {
    fn default_launch_plan(&self, _ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_gopls_command(),
            &[],
            Some(json!({ "semanticTokens": true })),
            "install gopls (e.g. `go install golang.org/x/tools/gopls@latest`)",
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct PyrightLspLaunchPolicy;

impl LspLaunchPolicy for PyrightLspLaunchPolicy {
    fn default_launch_plan(&self, ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_pyright_langserver_command(ctx.workspace_root, ctx.language_root),
            &["--stdio"],
            None,
            "install pyright-langserver (e.g. `npm i -g pyright` or `pip install pyright`)",
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct TypeScriptLanguageServerLaunchPolicy;

impl LspLaunchPolicy for TypeScriptLanguageServerLaunchPolicy {
    fn default_launch_plan(&self, ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_typescript_language_server_command(ctx.workspace_root, ctx.language_root),
            &["--stdio"],
            None,
            "install typescript-language-server (e.g. `npm i -g typescript-language-server typescript`)",
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct ClangdLspLaunchPolicy;

impl LspLaunchPolicy for ClangdLspLaunchPolicy {
    fn default_launch_plan(&self, _ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_clangd_command(),
            &[],
            None,
            "install clangd (usually from llvm/clang toolchain packages)",
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct JdtlsLspLaunchPolicy;

impl LspLaunchPolicy for JdtlsLspLaunchPolicy {
    fn default_launch_plan(&self, _ctx: &LspLaunchContext<'_>) -> LspLaunchPlan {
        launch_plan(
            resolve_jdtls_command(),
            &[],
            None,
            "install jdtls (Eclipse JDT Language Server) and ensure `jdtls` is in PATH",
        )
    }
}

pub(crate) static EMPTY_LSP_LAUNCH_POLICY: EmptyLspLaunchPolicy = EmptyLspLaunchPolicy;
static RUST_ANALYZER_LSP_LAUNCH_POLICY: RustAnalyzerLspLaunchPolicy = RustAnalyzerLspLaunchPolicy;
static GOPLS_LSP_LAUNCH_POLICY: GoplsLspLaunchPolicy = GoplsLspLaunchPolicy;
static PYRIGHT_LSP_LAUNCH_POLICY: PyrightLspLaunchPolicy = PyrightLspLaunchPolicy;
static TYPESCRIPT_LANGUAGE_SERVER_LSP_LAUNCH_POLICY: TypeScriptLanguageServerLaunchPolicy =
    TypeScriptLanguageServerLaunchPolicy;
static CLANGD_LSP_LAUNCH_POLICY: ClangdLspLaunchPolicy = ClangdLspLaunchPolicy;
static JDTLS_LSP_LAUNCH_POLICY: JdtlsLspLaunchPolicy = JdtlsLspLaunchPolicy;

pub(crate) fn launch_policy_for(server: Option<LspServerKind>) -> &'static dyn LspLaunchPolicy {
    match server {
        Some(LspServerKind::RustAnalyzer) => &RUST_ANALYZER_LSP_LAUNCH_POLICY,
        Some(LspServerKind::Gopls) => &GOPLS_LSP_LAUNCH_POLICY,
        Some(LspServerKind::Pyright) => &PYRIGHT_LSP_LAUNCH_POLICY,
        Some(LspServerKind::TypeScriptLanguageServer) => {
            &TYPESCRIPT_LANGUAGE_SERVER_LSP_LAUNCH_POLICY
        }
        Some(LspServerKind::Clangd) => &CLANGD_LSP_LAUNCH_POLICY,
        Some(LspServerKind::Jdtls) => &JDTLS_LSP_LAUNCH_POLICY,
        None => &EMPTY_LSP_LAUNCH_POLICY,
    }
}

fn launch_plan(
    command: Option<String>,
    args: &[&str],
    initialization_options: Option<Value>,
    install_hint: &'static str,
) -> LspLaunchPlan {
    LspLaunchPlan {
        command,
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        initialization_options,
        install_hint,
    }
}

fn resolve_rust_analyzer_command(root: &Path) -> Option<String> {
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

fn resolve_clangd_command() -> Option<String> {
    find_in_path("clangd").map(|path| path.to_string_lossy().to_string())
}

fn resolve_jdtls_command() -> Option<String> {
    find_in_path("jdtls").map(|path| path.to_string_lossy().to_string())
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
