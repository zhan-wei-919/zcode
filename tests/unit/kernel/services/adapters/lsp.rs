use super::*;
use crate::kernel::services::ports::{AsyncExecutor, BoxFuture};
use crate::kernel::services::KernelServiceHost;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

#[cfg(unix)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
struct EnvRestore {
    path: Option<std::ffi::OsString>,
    cargo_home: Option<std::ffi::OsString>,
}

#[cfg(unix)]
impl EnvRestore {
    fn capture() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            cargo_home: std::env::var_os("CARGO_HOME"),
        }
    }

    fn restore_var(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

#[cfg(unix)]
impl Drop for EnvRestore {
    fn drop(&mut self) {
        Self::restore_var("PATH", self.path.take());
        Self::restore_var("CARGO_HOME", self.cargo_home.take());
    }
}

#[test]
fn file_url_roundtrip_unix() {
    #[cfg(unix)]
    {
        let path = PathBuf::from("/tmp/zcode test.rs");
        let url = lsp_types::Url::from_file_path(&path).unwrap();
        let back = url.to_file_path().unwrap();
        assert_eq!(back, path);
    }
}

#[test]
fn lsp_restart_backoff_blocks_ensure_started() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut client = LspClient::new(
        PathBuf::from("."),
        LspServerKind::RustAnalyzer,
        host.context(),
    );

    client.schedule_restart_backoff();
    assert_eq!(client.restart_attempts, 1);
    assert!(client.restart_backoff_until.is_some());
    assert!(!client.ensure_started());
    assert!(client.restart_backoff_until.is_some());
}

#[test]
fn lsp_restart_backoff_is_capped() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut client = LspClient::new(
        PathBuf::from("."),
        LspServerKind::RustAnalyzer,
        host.context(),
    );

    for _ in 0..16 {
        client.schedule_restart_backoff();
    }

    let until = client.restart_backoff_until.expect("backoff set");
    let delay = until.saturating_duration_since(Instant::now());
    assert!(delay <= Duration::from_secs(5));
}

#[test]
fn semantic_tokens_error_does_not_clear_previous_highlight() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let mut host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let ctx = host.context();

    handle_response(
        LspRequestKind::SemanticTokens {
            path: PathBuf::from("a.rs"),
            version: 42,
        },
        Response {
            id: RequestId::from(1),
            result: None,
            error: Some(lsp_server::ResponseError {
                code: 1,
                message: "stub error".to_string(),
                data: None,
            }),
        },
        &ctx,
    );

    assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn semantic_tokens_null_result_does_not_clear_previous_highlight() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let mut host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let ctx = host.context();

    handle_response(
        LspRequestKind::SemanticTokens {
            path: PathBuf::from("a.rs"),
            version: 42,
        },
        Response {
            id: RequestId::from(1),
            result: Some(serde_json::Value::Null),
            error: None,
        },
        &ctx,
    );

    assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
#[cfg(unix)]
fn resolve_rust_analyzer_command_prefers_path() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let ra = temp.path().join("rust-analyzer");
    std::fs::write(&ra, "#!/usr/bin/env sh\nexit 0\n").expect("write stub ra");
    let mut perms = std::fs::metadata(&ra).expect("stat ra").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&ra, perms).expect("chmod ra");

    std::env::set_var("PATH", temp.path());
    std::env::set_var("CARGO_HOME", temp.path().join("cargo-empty"));

    let resolved = super::discovery::resolve_rust_analyzer_command(temp.path()).expect("resolved");
    assert_eq!(resolved, ra.to_string_lossy());
}

#[test]
#[cfg(unix)]
fn resolve_rust_analyzer_command_falls_back_to_cargo_home() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let cargo_home = temp.path().join("cargo");
    let cargo_bin = cargo_home.join("bin");
    std::fs::create_dir_all(&cargo_bin).expect("mkdir cargo bin");
    let ra = cargo_bin.join("rust-analyzer");
    std::fs::write(&ra, "#!/usr/bin/env sh\nexit 0\n").expect("write stub ra");
    let mut perms = std::fs::metadata(&ra).expect("stat ra").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&ra, perms).expect("chmod ra");

    std::env::set_var("PATH", "");
    std::env::set_var("CARGO_HOME", &cargo_home);

    let resolved = super::discovery::resolve_rust_analyzer_command(temp.path()).expect("resolved");
    assert_eq!(resolved, ra.to_string_lossy());
}

#[test]
#[cfg(unix)]
fn resolve_rust_analyzer_command_falls_back_to_rustup_which() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let ra = temp.path().join("ra-from-rustup");
    std::fs::write(&ra, "#!/bin/sh\nexit 0\n").expect("write stub ra");
    let mut perms = std::fs::metadata(&ra).expect("stat ra").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&ra, perms).expect("chmod ra");

    let rustup_dir = temp.path().join("bin");
    std::fs::create_dir_all(&rustup_dir).expect("mkdir rustup bin");
    let rustup = rustup_dir.join("rustup");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"which\" ] && [ \"$2\" = \"rust-analyzer\" ]; then\n  echo \"{}\"\n  exit 0\nfi\nexit 1\n",
        ra.to_string_lossy()
    );
    std::fs::write(&rustup, script).expect("write stub rustup");
    let mut perms = std::fs::metadata(&rustup)
        .expect("stat rustup")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&rustup, perms).expect("chmod rustup");

    std::env::set_var("PATH", &rustup_dir);
    std::env::set_var("CARGO_HOME", temp.path().join("cargo-empty"));

    let resolved = super::discovery::resolve_rust_analyzer_command(temp.path()).expect("resolved");
    assert_eq!(resolved, ra.to_string_lossy());
}

#[test]
fn language_root_for_file_prefers_nearest_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let rust_root = root.join("crates").join("app");
    std::fs::create_dir_all(rust_root.join("src")).expect("mkdir rust");
    std::fs::write(rust_root.join("Cargo.toml"), "[package]\nname = \"app\"\n").expect("cargo");
    let rust_file = rust_root.join("src").join("main.rs");

    let js_root = root.join("web");
    std::fs::create_dir_all(js_root.join("src")).expect("mkdir web");
    std::fs::write(js_root.join("package.json"), "{}").expect("package.json");
    let js_file = js_root.join("src").join("index.ts");

    let rust = crate::kernel::lsp_registry::LspLanguage::from_path(&rust_file).expect("rust lang");
    let resolved = crate::kernel::lsp_registry::language_root_for_file(root, rust, &rust_file);
    assert_eq!(resolved, rust_root);

    let ts = crate::kernel::lsp_registry::LspLanguage::from_path(&js_file).expect("ts lang");
    let resolved = crate::kernel::lsp_registry::language_root_for_file(root, ts, &js_file);
    assert_eq!(resolved, js_root);
}

#[test]
fn language_root_for_file_falls_back_to_workspace_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let rust_file = root.join("src").join("main.rs");
    std::fs::create_dir_all(rust_file.parent().unwrap()).expect("mkdir");

    let rust = crate::kernel::lsp_registry::LspLanguage::from_path(&rust_file).expect("rust lang");
    let resolved = crate::kernel::lsp_registry::language_root_for_file(root, rust, &rust_file);
    assert_eq!(resolved, root);
}

#[test]
fn language_id_for_path_matches_expected() {
    let cases = [
        ("a.rs", "rust"),
        ("a.go", "go"),
        ("a.py", "python"),
        ("a.pyi", "python"),
        ("a.js", "javascript"),
        ("a.jsx", "javascriptreact"),
        ("a.ts", "typescript"),
        ("a.tsx", "typescriptreact"),
        ("a.txt", "plaintext"),
    ];

    for (path, expected) in cases {
        assert_eq!(
            super::convert::language_id_for_path(std::path::Path::new(path)),
            expected
        );
    }
}

#[test]
#[cfg(unix)]
fn resolve_default_server_command_prefers_node_modules_bin() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let bin_dir = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir node_modules/.bin");

    let tls = bin_dir.join("typescript-language-server");
    std::fs::write(&tls, "#!/bin/sh\nexit 0\n").expect("write tls");
    let mut perms = std::fs::metadata(&tls).expect("stat tls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&tls, perms).expect("chmod tls");

    std::env::set_var("PATH", "");

    let (cmd, args) = super::discovery::resolve_default_server_command(
        root,
        root,
        crate::kernel::lsp_registry::LspLanguage::TypeScript,
    )
    .expect("resolved");
    assert_eq!(cmd, tls.to_string_lossy());
    assert_eq!(args, vec!["--stdio".to_string()]);
}

#[test]
#[cfg(unix)]
fn resolve_default_server_command_searches_node_modules_upwards() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();

    let bin_dir = workspace_root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir node_modules/.bin");

    let tls = bin_dir.join("typescript-language-server");
    std::fs::write(&tls, "#!/bin/sh\nexit 0\n").expect("write tls");
    let mut perms = std::fs::metadata(&tls).expect("stat tls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&tls, perms).expect("chmod tls");

    let nested_root = workspace_root.join("packages").join("foo");
    std::fs::create_dir_all(&nested_root).expect("mkdir nested root");

    std::env::set_var("PATH", "");

    let (cmd, args) = super::discovery::resolve_default_server_command(
        workspace_root,
        &nested_root,
        crate::kernel::lsp_registry::LspLanguage::TypeScript,
    )
    .expect("resolved");
    assert_eq!(cmd, tls.to_string_lossy());
    assert_eq!(args, vec!["--stdio".to_string()]);
}
