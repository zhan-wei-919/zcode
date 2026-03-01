use super::*;
use crate::kernel::services::ports::{AsyncExecutor, BoxFuture};
use crate::kernel::services::KernelServiceHost;
use lsp_server::Message;
#[cfg(unix)]
use std::collections::VecDeque;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

#[cfg(unix)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
struct EnvRestore {
    path: Option<std::ffi::OsString>,
    cargo_home: Option<std::ffi::OsString>,
    gobin: Option<std::ffi::OsString>,
    gopath: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
}

#[cfg(unix)]
impl EnvRestore {
    fn capture() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            cargo_home: std::env::var_os("CARGO_HOME"),
            gobin: std::env::var_os("GOBIN"),
            gopath: std::env::var_os("GOPATH"),
            home: std::env::var_os("HOME"),
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
        Self::restore_var("GOBIN", self.gobin.take());
        Self::restore_var("GOPATH", self.gopath.take());
        Self::restore_var("HOME", self.home.take());
    }
}

#[cfg(unix)]
fn ready_lsp_client_for_request_tests(
    server: LspServerKind,
) -> (LspClient, std::sync::mpsc::Receiver<Message>) {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut client = LspClient::new(PathBuf::from("."), server, host.context());

    let (tx, rx) = std::sync::mpsc::channel::<Message>();
    let pending = Arc::new(std::sync::Mutex::new(super::wire::LspPending {
        state: super::wire::InitState::Ready,
        queue: VecDeque::new(),
    }));
    let child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stub child process");
    client.process = Some(super::wire::LspProcess {
        tx,
        pending,
        child: Arc::new(std::sync::Mutex::new(child)),
    });

    (client, rx)
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

    let rust = crate::kernel::language::LanguageId::from_path(&rust_file).expect("rust lang");
    let resolved = crate::kernel::lsp_registry::language_root_for_file(root, rust, &rust_file);
    assert_eq!(resolved, rust_root);

    let ts = crate::kernel::language::LanguageId::from_path(&js_file).expect("ts lang");
    let resolved = crate::kernel::lsp_registry::language_root_for_file(root, ts, &js_file);
    assert_eq!(resolved, js_root);
}

#[test]
fn language_root_for_file_falls_back_to_workspace_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let rust_file = root.join("src").join("main.rs");
    std::fs::create_dir_all(rust_file.parent().unwrap()).expect("mkdir");

    let rust = crate::kernel::language::LanguageId::from_path(&rust_file).expect("rust lang");
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
        ("a.c", "c"),
        ("a.cpp", "cpp"),
        ("a.h", "cpp"),
        ("a.java", "java"),
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
fn hover_text_language_string_is_normalized_to_markdown_fence() {
    let hover = lsp_types::Hover {
        contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::LanguageString(
            lsp_types::LanguageString {
                language: "java".to_string(),
                value: "public class A {}".to_string(),
            },
        )),
        range: None,
    };

    let text = super::convert::hover_text(&hover).expect("hover text");
    assert_eq!(text, "```java\npublic class A {}\n```");
}

#[test]
fn hover_text_array_separates_segments_with_blank_line() {
    let hover = lsp_types::Hover {
        contents: lsp_types::HoverContents::Array(vec![
            lsp_types::MarkedString::String("signature".to_string()),
            lsp_types::MarkedString::LanguageString(lsp_types::LanguageString {
                language: "rust".to_string(),
                value: "fn demo()".to_string(),
            }),
            lsp_types::MarkedString::String("docs".to_string()),
        ]),
        range: None,
    };

    let text = super::convert::hover_text(&hover).expect("hover text");
    assert_eq!(text, "signature\n\n```rust\nfn demo()\n```\n\ndocs");
}

#[test]
fn client_capabilities_enable_definition_link_support() {
    let caps = super::convert::client_capabilities();
    let text_document = caps.text_document.expect("textDocument capabilities");
    let definition = text_document.definition.expect("definition capability");
    assert_eq!(definition.link_support, Some(true));
}

#[test]
fn documentation_text_preserves_markup_content_payload() {
    let doc = lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
        kind: lsp_types::MarkupKind::Markdown,
        value: "**bold**\n\n`code`".to_string(),
    });

    let text = super::convert::documentation_text(&doc).expect("documentation");
    assert_eq!(text, "**bold**\n\n`code`");
}

#[test]
fn definition_preview_target_uses_location_link_target_range() {
    let uri = lsp_types::Url::parse("file:///tmp/demo.rs").expect("uri");
    let response = lsp_types::GotoDefinitionResponse::Link(vec![lsp_types::LocationLink {
        origin_selection_range: None,
        target_uri: uri,
        target_range: lsp_types::Range::new(
            lsp_types::Position::new(10, 0),
            lsp_types::Position::new(14, 1),
        ),
        target_selection_range: lsp_types::Range::new(
            lsp_types::Position::new(10, 7),
            lsp_types::Position::new(10, 13),
        ),
    }]);

    let target = super::convert::definition_preview_target(response).expect("target");
    assert_eq!(target.anchor_line, 10);
    assert_eq!(target.anchor_column, 7);
    let range = target.range.expect("range");
    assert_eq!(range.start.line, 10);
    assert_eq!(range.end.line, 14);
}

#[test]
fn lsp_server_kind_from_settings_key_includes_c_cpp_java() {
    assert_eq!(
        LspServerKind::from_settings_key("clangd"),
        Some(LspServerKind::Clangd)
    );
    assert_eq!(
        LspServerKind::from_settings_key("cpp"),
        Some(LspServerKind::Clangd)
    );
    assert_eq!(
        LspServerKind::from_settings_key("c++"),
        Some(LspServerKind::Clangd)
    );
    assert_eq!(
        LspServerKind::from_settings_key("jdtls"),
        Some(LspServerKind::Jdtls)
    );
    assert_eq!(
        LspServerKind::from_settings_key("java"),
        Some(LspServerKind::Jdtls)
    );
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
        crate::kernel::language::LanguageId::TypeScript,
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
        crate::kernel::language::LanguageId::TypeScript,
    )
    .expect("resolved");
    assert_eq!(cmd, tls.to_string_lossy());
    assert_eq!(args, vec!["--stdio".to_string()]);
}

#[test]
#[cfg(unix)]
fn resolve_default_server_command_go_falls_back_to_gopath_bin() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();
    let gopath = workspace_root.join("gopath");
    let gopath_bin = gopath.join("bin");
    std::fs::create_dir_all(&gopath_bin).expect("mkdir gopath bin");

    let gopls = gopath_bin.join("gopls");
    std::fs::write(&gopls, "#!/bin/sh\nexit 0\n").expect("write gopls");
    let mut perms = std::fs::metadata(&gopls).expect("stat gopls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&gopls, perms).expect("chmod gopls");

    std::env::set_var("PATH", "");
    std::env::set_var("GOPATH", &gopath);
    std::env::remove_var("GOBIN");

    let (cmd, args) = super::discovery::resolve_default_server_command(
        workspace_root,
        workspace_root,
        crate::kernel::language::LanguageId::Go,
    )
    .expect("resolved");

    assert_eq!(cmd, gopls.to_string_lossy());
    assert!(args.is_empty());
}

#[test]
#[cfg(unix)]
fn resolve_default_server_command_go_falls_back_to_home_go_bin() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();
    let home = workspace_root.join("home");
    let home_go_bin = home.join("go").join("bin");
    std::fs::create_dir_all(&home_go_bin).expect("mkdir home go bin");

    let gopls = home_go_bin.join("gopls");
    std::fs::write(&gopls, "#!/bin/sh\nexit 0\n").expect("write gopls");
    let mut perms = std::fs::metadata(&gopls).expect("stat gopls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&gopls, perms).expect("chmod gopls");

    std::env::set_var("PATH", "");
    std::env::remove_var("GOPATH");
    std::env::remove_var("GOBIN");
    std::env::set_var("HOME", &home);

    let (cmd, args) = super::discovery::resolve_default_server_command(
        workspace_root,
        workspace_root,
        crate::kernel::language::LanguageId::Go,
    )
    .expect("resolved");

    assert_eq!(cmd, gopls.to_string_lossy());
    assert!(args.is_empty());
}

#[test]
#[cfg(unix)]
fn resolve_default_server_command_c_cpp_and_java_from_path() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();

    let clangd = workspace_root.join("clangd");
    std::fs::write(&clangd, "#!/bin/sh\nexit 0\n").expect("write clangd");
    let mut clangd_perms = std::fs::metadata(&clangd)
        .expect("stat clangd")
        .permissions();
    use std::os::unix::fs::PermissionsExt as _;
    clangd_perms.set_mode(0o755);
    std::fs::set_permissions(&clangd, clangd_perms).expect("chmod clangd");

    let jdtls = workspace_root.join("jdtls");
    std::fs::write(&jdtls, "#!/bin/sh\nexit 0\n").expect("write jdtls");
    let mut jdtls_perms = std::fs::metadata(&jdtls).expect("stat jdtls").permissions();
    jdtls_perms.set_mode(0o755);
    std::fs::set_permissions(&jdtls, jdtls_perms).expect("chmod jdtls");

    std::env::set_var("PATH", workspace_root);

    let (cmd_c, args_c) = super::discovery::resolve_default_server_command(
        workspace_root,
        workspace_root,
        crate::kernel::language::LanguageId::C,
    )
    .expect("resolved c");
    assert_eq!(cmd_c, clangd.to_string_lossy());
    assert!(args_c.is_empty());

    let (cmd_cpp, args_cpp) = super::discovery::resolve_default_server_command(
        workspace_root,
        workspace_root,
        crate::kernel::language::LanguageId::Cpp,
    )
    .expect("resolved cpp");
    assert_eq!(cmd_cpp, clangd.to_string_lossy());
    assert!(args_cpp.is_empty());

    let (cmd_java, args_java) = super::discovery::resolve_default_server_command(
        workspace_root,
        workspace_root,
        crate::kernel::language::LanguageId::Java,
    )
    .expect("resolved java");
    assert_eq!(cmd_java, jdtls.to_string_lossy());
    assert!(args_java.is_empty());
}

#[test]
#[cfg(unix)]
fn resolve_server_command_go_uses_default_semantic_init_options() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();
    let go_file = workspace_root.join("main.go");
    std::fs::write(&go_file, "package main\n").expect("write go file");

    let gopath = workspace_root.join("gopath");
    let gopath_bin = gopath.join("bin");
    std::fs::create_dir_all(&gopath_bin).expect("mkdir gopath bin");
    let gopls = gopath_bin.join("gopls");
    std::fs::write(&gopls, "#!/bin/sh\nexit 0\n").expect("write gopls");
    let mut perms = std::fs::metadata(&gopls).expect("stat gopls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&gopls, perms).expect("chmod gopls");

    std::env::set_var("PATH", "");
    std::env::set_var("GOPATH", &gopath);
    std::env::remove_var("GOBIN");

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut service = LspService::new(workspace_root.to_path_buf(), host.context());

    let (language, key) = service
        .client_key_for_path(&go_file)
        .expect("go client key");
    let (_cmd, _args, init_options) = service
        .resolve_server_command(language, &key)
        .expect("resolve go command");

    assert_eq!(
        init_options,
        Some(serde_json::json!({ "semanticTokens": true }))
    );
}

#[test]
#[cfg(unix)]
fn resolve_server_command_go_prefers_user_init_options() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvRestore::capture();

    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path();
    let go_file = workspace_root.join("main.go");
    std::fs::write(&go_file, "package main\n").expect("write go file");

    let gopath = workspace_root.join("gopath");
    let gopath_bin = gopath.join("bin");
    std::fs::create_dir_all(&gopath_bin).expect("mkdir gopath bin");
    let gopls = gopath_bin.join("gopls");
    std::fs::write(&gopls, "#!/bin/sh\nexit 0\n").expect("write gopls");
    let mut perms = std::fs::metadata(&gopls).expect("stat gopls").permissions();
    use std::os::unix::fs::PermissionsExt as _;
    perms.set_mode(0o755);
    std::fs::set_permissions(&gopls, perms).expect("chmod gopls");

    std::env::set_var("PATH", "");
    std::env::set_var("GOPATH", &gopath);
    std::env::remove_var("GOBIN");

    let mut overrides = rustc_hash::FxHashMap::default();
    overrides.insert(
        LspServerKind::Gopls,
        LspServerCommandOverride {
            command: None,
            args: None,
            initialization_options: Some(serde_json::json!({ "semanticTokens": false })),
        },
    );

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut service = LspService::new(workspace_root.to_path_buf(), host.context())
        .with_server_command_overrides(overrides);

    let (language, key) = service
        .client_key_for_path(&go_file)
        .expect("go client key");
    let (_cmd, _args, init_options) = service
        .resolve_server_command(language, &key)
        .expect("resolve go command");

    assert_eq!(
        init_options,
        Some(serde_json::json!({ "semanticTokens": false }))
    );
}

#[test]
#[cfg(unix)]
fn semantic_tokens_requests_for_different_files_do_not_cancel_each_other() {
    let (mut client, rx) = ready_lsp_client_for_request_tests(LspServerKind::Jdtls);

    let path_a = PathBuf::from("/tmp/AuthServiceApplication.java");
    let path_b = PathBuf::from("/tmp/JwtAuthenticationFilter.java");
    client.doc_versions.insert(path_a.clone(), 1);
    client.doc_versions.insert(path_b.clone(), 1);

    client.request_semantic_tokens(&path_a, 1);
    client.request_semantic_tokens(&path_b, 1);

    let first = rx.recv().expect("first message");
    let second = rx.recv().expect("second message");

    match first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected first semantic tokens request, got {other:?}"),
    }

    match second {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(2));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected second semantic tokens request, got {other:?}"),
    }
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    let pending = client
        .pending_requests
        .lock()
        .expect("lock pending requests");
    assert!(pending.contains_key(&lsp_server::RequestId::from(1)));
    assert!(pending.contains_key(&lsp_server::RequestId::from(2)));
}

#[test]
#[cfg(unix)]
fn hover_request_without_definition_preview_sends_only_hover_request() {
    let (mut client, rx) = ready_lsp_client_for_request_tests(LspServerKind::Jdtls);
    let path = PathBuf::from("/tmp/AuthServiceApplication.java");
    client.doc_versions.insert(path.clone(), 1);

    client.request_hover(
        &path,
        crate::kernel::services::ports::LspPosition {
            line: 3,
            character: 5,
        },
        super::HoverRequestOptions {
            include_definition_source: false,
            definition_max_lines: 400,
        },
    );

    let first = rx.recv().expect("first message");
    match first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/hover");
        }
        other => panic!("expected hover request, got {other:?}"),
    }
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
#[cfg(unix)]
fn hover_request_with_definition_preview_sends_hover_and_definition_requests() {
    let (mut client, rx) = ready_lsp_client_for_request_tests(LspServerKind::Jdtls);
    let path = PathBuf::from("/tmp/AuthServiceApplication.java");
    client.doc_versions.insert(path.clone(), 1);

    client.request_hover(
        &path,
        crate::kernel::services::ports::LspPosition {
            line: 3,
            character: 5,
        },
        super::HoverRequestOptions {
            include_definition_source: true,
            definition_max_lines: 400,
        },
    );

    let first = rx.recv().expect("first message");
    let second = rx.recv().expect("second message");

    match first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/hover");
        }
        other => panic!("expected hover request, got {other:?}"),
    }
    match second {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(2));
            assert_eq!(req.method, "textDocument/definition");
        }
        other => panic!("expected definition preview request, got {other:?}"),
    }
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
#[cfg(unix)]
fn semantic_tokens_requests_for_same_file_cancel_previous_request() {
    let (mut client, rx) = ready_lsp_client_for_request_tests(LspServerKind::Jdtls);

    let path = PathBuf::from("/tmp/AuthServiceApplication.java");
    client.doc_versions.insert(path.clone(), 1);

    client.request_semantic_tokens(&path, 1);
    client.request_semantic_tokens(&path, 1);

    let first = rx.recv().expect("first message");
    let second = rx.recv().expect("second message");
    let third = rx.recv().expect("third message");

    match first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected first semantic tokens request, got {other:?}"),
    }

    match second {
        Message::Notification(notification) => {
            assert_eq!(notification.method, "$/cancelRequest");
            let params = serde_json::from_value::<lsp_types::CancelParams>(notification.params)
                .expect("decode cancel params");
            assert_eq!(params.id, lsp_types::NumberOrString::Number(1));
        }
        other => panic!("expected cancel notification, got {other:?}"),
    }

    match third {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(2));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected second semantic tokens request, got {other:?}"),
    }
}

#[test]
#[cfg(unix)]
fn semantic_tokens_cancellation_does_not_cross_lsp_clients() {
    let (mut java_client, java_rx) = ready_lsp_client_for_request_tests(LspServerKind::Jdtls);
    let (mut rust_client, rust_rx) =
        ready_lsp_client_for_request_tests(LspServerKind::RustAnalyzer);

    let java_path = PathBuf::from("/tmp/AuthServiceApplication.java");
    let rust_path = PathBuf::from("/tmp/main.rs");
    java_client.doc_versions.insert(java_path.clone(), 1);
    rust_client.doc_versions.insert(rust_path.clone(), 1);

    java_client.request_semantic_tokens(&java_path, 1);
    rust_client.request_semantic_tokens(&rust_path, 1);

    let java_first = java_rx.recv().expect("java first message");
    let rust_first = rust_rx.recv().expect("rust first message");

    match java_first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected java semantic tokens request, got {other:?}"),
    }

    match rust_first {
        Message::Request(req) => {
            assert_eq!(req.id, lsp_server::RequestId::from(1));
            assert_eq!(req.method, "textDocument/semanticTokens/full");
        }
        other => panic!("expected rust semantic tokens request, got {other:?}"),
    }

    assert!(matches!(java_rx.try_recv(), Err(TryRecvError::Empty)));
    assert!(matches!(rust_rx.try_recv(), Err(TryRecvError::Empty)));
}
