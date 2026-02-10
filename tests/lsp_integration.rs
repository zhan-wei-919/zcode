#![cfg(feature = "tui")]

use std::ffi::OsString;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::tempdir;
use zcode::app::Workbench;
use zcode::core::event::{
    InputEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use zcode::kernel::editor::{HighlightKind, HighlightSpan};
use zcode::kernel::services::adapters::{AppMessage, AsyncRuntime};
use zcode::kernel::services::ports::{LspPositionEncoding, LspServerKind};
use zcode::kernel::{BottomPanelTab, FocusTarget};
use zcode::tui::view::View;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn set_str(key: &'static str, value: &str) -> Self {
        let saved = vec![(key, std::env::var_os(key))];
        std::env::set_var(key, value);
        Self { saved }
    }

    fn set(mut self, key: &'static str, value: &std::ffi::OsStr) -> Self {
        self.saved.push((key, std::env::var_os(key)));
        std::env::set_var(key, value);
        self
    }

    fn remove(mut self, key: &'static str) -> Self {
        self.saved.push((key, std::env::var_os(key)));
        std::env::remove_var(key);
        self
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn create_runtime() -> (AsyncRuntime, mpsc::Receiver<AppMessage>) {
    let (tx, rx) = mpsc::channel();
    (AsyncRuntime::new(tx).unwrap(), rx)
}

fn drain_runtime_messages(workbench: &mut Workbench, rx: &mpsc::Receiver<AppMessage>) {
    while let Ok(msg) = rx.try_recv() {
        workbench.handle_message(msg);
    }
}

fn drive_until(
    workbench: &mut Workbench,
    rx: &mpsc::Receiver<AppMessage>,
    timeout: Duration,
    mut done: impl FnMut(&Workbench) -> bool,
) {
    let start = Instant::now();
    loop {
        drain_runtime_messages(workbench, rx);
        workbench.tick();
        if done(workbench) {
            return;
        }
        if start.elapsed() > timeout {
            let trace = std::env::var_os("ZCODE_LSP_STUB_TRACE_PATH")
                .filter(|p| !p.is_empty())
                .and_then(|p| std::fs::read_to_string(&p).ok());
            if let Some(trace) = trace {
                panic!("timeout waiting for condition\n\nlsp stub trace:\n{trace}");
            }
            panic!("timeout waiting for condition");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn has_command_in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };

    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return true;
        }
    }

    false
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0)
        .is_some()
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

#[test]
fn test_lsp_spawn_sync_requests_and_diagnostics_are_wired() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let def_path = dir.path().join("definition_target.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());
    assert_eq!(
        std::env::var("ZCODE_LSP_COMMAND").unwrap(),
        stub_path.to_string_lossy()
    );

    std::fs::write(&a_path, "fn main() {}\n").unwrap();
    std::fs::write(&def_path, "pub fn target() {}\n").unwrap();
    let def_path_canon = std::fs::canonicalize(&def_path).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());
    let (cmd, args) = workbench.lsp_command_config().unwrap();
    assert_eq!(cmd, stub_path.to_string_lossy());
    assert!(args.is_empty());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let hover = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(hover));
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .ui
            .hover_message
            .as_deref()
            .is_some_and(|m| m.starts_with("stub hover @"))
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .problems
            .items()
            .iter()
            .any(|item| item.message == "didOpen")
    });

    let insert = KeyEvent {
        code: KeyCode::Char('X'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .problems
            .items()
            .iter()
            .any(|item| item.message == "didChange")
    });

    let save = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(save));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        std::fs::read_to_string(&a_path)
            .ok()
            .is_some_and(|content| content.starts_with('X'))
            && w.state()
                .problems
                .items()
                .iter()
                .any(|item| item.message == "didSave")
    });

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubItem")
    });

    let def = KeyEvent {
        code: KeyCode::F(12),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(def));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|p| p == def_path_canon)
    });
}

#[test]
fn test_lsp_spawns_for_multiple_languages() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let ts_path = dir.path().join("a.ts");
    let go_path = dir.path().join("b.go");
    let py_path = dir.path().join("c.py");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&ts_path, "export const x = 1;\n").unwrap();
    std::fs::write(&go_path, "package main\n\nfunc main() {}\n").unwrap();
    std::fs::write(&py_path, "x = 1\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    for path in [&ts_path, &go_path, &py_path] {
        workbench.handle_message(AppMessage::FileLoaded {
            path: path.to_path_buf(),
            content: std::fs::read_to_string(path).unwrap(),
        });
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let caps = &w.state().lsp.server_capabilities;
        caps.keys()
            .any(|k| k.server == LspServerKind::TypeScriptLanguageServer)
            && caps.keys().any(|k| k.server == LspServerKind::Gopls)
            && caps.keys().any(|k| k.server == LspServerKind::Pyright)
    });
}

#[test]
fn test_did_open_sends_language_id_for_python_go_and_js_ts() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let py_path = dir.path().join("a.py");
    let go_path = dir.path().join("b.go");
    let ts_path = dir.path().join("c.ts");
    let js_path = dir.path().join("d.js");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&py_path, "foo = 1\n").unwrap();
    std::fs::write(&go_path, "package main\n\nfunc main() {}\n").unwrap();
    std::fs::write(&ts_path, "export const x = 1;\n").unwrap();
    std::fs::write(&js_path, "export const y = 2;\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    for path in [&py_path, &go_path, &ts_path, &js_path] {
        workbench.handle_message(AppMessage::FileLoaded {
            path: path.to_path_buf(),
            content: std::fs::read_to_string(path).unwrap(),
        });
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .any(|line| line.trim() == "didOpen languageId python")
            && trace
                .lines()
                .any(|line| line.trim() == "didOpen languageId go")
            && trace
                .lines()
                .any(|line| line.trim() == "didOpen languageId typescript")
            && trace
                .lines()
                .any(|line| line.trim() == "didOpen languageId javascript")
    });
}

#[test]
fn test_hover_and_completion_work_for_python_go_and_js_ts() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let py_path = dir.path().join("a.py");
    let go_path = dir.path().join("b.go");
    let ts_path = dir.path().join("c.ts");
    let js_path = dir.path().join("d.js");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&py_path, "foo = 1\n").unwrap();
    std::fs::write(&go_path, "package main\n\nfunc main() {}\n").unwrap();
    std::fs::write(&ts_path, "export const x = 1;\n").unwrap();
    std::fs::write(&js_path, "export const y = 2;\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let hover = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let close = KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };

    for path in [&py_path, &go_path, &ts_path, &js_path] {
        workbench.handle_message(AppMessage::FileLoaded {
            path: path.to_path_buf(),
            content: std::fs::read_to_string(path).unwrap(),
        });

        let _ = workbench.handle_input(&InputEvent::Key(completion));
        drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
            w.state().ui.completion.visible
                && w.state()
                    .ui
                    .completion
                    .items
                    .iter()
                    .any(|item| item.label == "stubItem")
        });

        let _ = workbench.handle_input(&InputEvent::Key(close));
        assert!(!workbench.state().ui.completion.visible);

        let _ = workbench.handle_input(&InputEvent::Key(hover));
        drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
            w.state()
                .ui
                .hover_message
                .as_deref()
                .is_some_and(|m| m.starts_with("stub hover @"))
        });

        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
        drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
            w.state().ui.hover_message.is_none()
        });
    }
}

#[test]
fn test_hover_works_when_cursor_is_after_identifier() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let py_path = dir.path().join("a.py");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&py_path, "foo = 1\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: py_path.clone(),
        content: std::fs::read_to_string(&py_path).unwrap(),
    });

    // Cursor at start -> move right 3 times so it's positioned *after* `foo`.
    for _ in 0..3 {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let hover = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(hover));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .ui
            .hover_message
            .as_deref()
            .is_some_and(|m| m.starts_with("stub hover @"))
    });
}

#[test]
fn test_completion_debounce_triggers_for_python() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let py_path = dir.path().join("a.py");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&py_path, "\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: py_path.clone(),
        content: std::fs::read_to_string(&py_path).unwrap(),
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('p'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubItem")
    });
}

#[test]
#[ignore]
fn test_real_pyright_hover_and_completion_smoke() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());

    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "zcode::kernel::services::adapters::lsp=debug",
        ))
        .with_test_writer()
        .try_init();

    if !has_command_in_path("pyright-langserver") {
        eprintln!("skip: pyright-langserver not found on PATH");
        return;
    }

    // Ensure we use real discovery (not the stub override from other tests / user env).
    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .remove("ZCODE_LSP_COMMAND")
        .remove("ZCODE_LSP_ARGS")
        .remove("ZCODE_LSP_STUB_TRACE_PATH")
        .remove("ZCODE_LSP_STUB_DISABLE_OPTIONAL");

    let dir = tempdir().unwrap();
    let hover_path = dir.path().join("hover.py");
    let comp_path = dir.path().join("completion.py");

    std::fs::write(&hover_path, "pred = 1\n").unwrap();
    std::fs::write(&comp_path, "import os\nos.\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: hover_path.clone(),
        content: std::fs::read_to_string(&hover_path).unwrap(),
    });

    // `FileLoaded` doesn't go through `dispatch_kernel`, so it doesn't automatically trigger LSP
    // sync. Send a harmless command to kick the bridge's `sync_lsp()` once.
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    // Wait until server capabilities are published.
    eprintln!("[e2e] waiting for pyright capabilities...");
    drive_until(&mut workbench, &rx, Duration::from_secs(20), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::Pyright)
    });
    eprintln!("[e2e] pyright capabilities ready");

    // Hover should produce non-empty markdown (pyright typically returns a fenced code block).
    let hover = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(hover));
    eprintln!("[e2e] waiting for hover...");
    drive_until(&mut workbench, &rx, Duration::from_secs(10), |w| {
        w.state()
            .ui
            .hover_message
            .as_deref()
            .is_some_and(|m| !m.trim().is_empty())
    });
    eprintln!("[e2e] hover ready");

    let hover_text = workbench
        .state()
        .ui
        .hover_message
        .clone()
        .unwrap_or_default();
    assert!(
        hover_text.contains("pred"),
        "expected hover to mention `pred`, got:\n{hover_text}"
    );

    // Open a second file and request completion.
    workbench.handle_message(AppMessage::FileLoaded {
        path: comp_path.clone(),
        content: std::fs::read_to_string(&comp_path).unwrap(),
    });

    // Put the cursor on the `os.` line (end-of-line).
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let completion = KeyEvent {
        code: KeyCode::Char('.'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));
    eprintln!("[e2e] waiting for completion...");
    drive_until(&mut workbench, &rx, Duration::from_secs(10), |w| {
        w.state().ui.completion.visible && !w.state().ui.completion.items.is_empty()
    });
    eprintln!("[e2e] completion ready");
}

#[test]
fn test_optional_lsp_requests_are_gated_by_capabilities() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let py_path = dir.path().join("a.py");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .set("ZCODE_LSP_STUB_DISABLE_OPTIONAL", std::ffi::OsStr::new("1"));

    std::fs::write(&py_path, "x = 1\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: py_path.clone(),
        content: std::fs::read_to_string(&py_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().lsp.server_capabilities.iter().any(|(k, c)| {
            k.server == LspServerKind::Pyright
                && !c.semantic_tokens
                && !c.inlay_hints
                && !c.folding_range
        })
    });

    let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
    assert!(
        !trace
            .lines()
            .any(|line| line.trim() == "request textDocument/semanticTokens/full"),
        "unexpected semanticTokens/full request in lsp trace:\n{trace}"
    );
    assert!(
        !trace
            .lines()
            .any(|line| line.trim() == "request textDocument/inlayHint"),
        "unexpected inlayHint request in lsp trace:\n{trace}"
    );
    assert!(
        !trace
            .lines()
            .any(|line| line.trim() == "request textDocument/foldingRange"),
        "unexpected foldingRange request in lsp trace:\n{trace}"
    );
}

#[test]
fn test_lsp_rename_applies_workspace_edit() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "old old\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let rename = KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(rename));
    assert!(workbench.state().ui.input_dialog.visible);

    for ch in ['n', 'e', 'w'] {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| tab.buffer.text() == "new new\n")
    });
}

#[test]
fn test_lsp_references_populates_locations_and_opens_selected_item() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let b_path = dir.path().join("b.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "old old\n").unwrap();
    std::fs::write(&b_path, "fn target() {}\n").unwrap();
    let b_path_canon = std::fs::canonicalize(&b_path).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let refs = KeyEvent {
        code: KeyCode::F(12),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(refs));
    assert!(workbench.state().ui.bottom_panel.visible);
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::Locations
    );

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().locations.items().len() >= 2
    });

    let focus_panel = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(focus_panel));

    let down = KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(down));

    let enter = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(enter));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.path.as_ref())
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|p| p == b_path_canon)
    });
}

#[test]
fn test_lsp_code_action_applies_edit_and_execute_command() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let code_action = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(code_action));
    assert!(workbench.state().ui.bottom_panel.visible);
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::CodeActions
    );
    assert_eq!(workbench.state().ui.focus, FocusTarget::BottomPanel);

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().code_actions.items().len() >= 2
    });

    let enter = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(enter));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| tab.buffer.text().starts_with("// edit\n"))
    });

    let _ = workbench.handle_input(&InputEvent::Key(code_action));
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::CodeActions
    );

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().code_actions.items().len() >= 2
    });

    let down = KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(down));
    let _ = workbench.handle_input(&InputEvent::Key(enter));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| tab.buffer.text().starts_with("// cmd\n// edit\n"))
    });
}

#[test]
fn test_lsp_range_format_replaces_selection() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "hello world\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    for _ in 0..6 {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    for _ in 0..5 {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::F(1),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(workbench.state().ui.command_palette.visible);
    assert_eq!(workbench.state().ui.focus, FocusTarget::CommandPalette);

    for ch in "format selection".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| tab.buffer.text() == "hello RANGE\n")
    });
}

#[test]
fn test_lsp_document_symbols_populates_symbols_and_jumps_to_item() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "line0\nline1\nline2\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::F(1),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(workbench.state().ui.command_palette.visible);
    assert_eq!(workbench.state().ui.focus, FocusTarget::CommandPalette);

    for ch in "document symbols".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    assert!(workbench.state().ui.bottom_panel.visible);
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::Symbols
    );
    assert_eq!(workbench.state().ui.focus, FocusTarget::BottomPanel);

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().symbols.items().len() >= 3
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| tab.buffer.cursor().0 == 1)
    });
}

#[test]
fn test_lsp_workspace_symbols_opens_selected_item() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let b_path = dir.path().join("b.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "line0\n").unwrap();
    std::fs::write(&b_path, "line0\n").unwrap();
    let b_path_canon = std::fs::canonicalize(&b_path).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::F(1),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(workbench.state().ui.command_palette.visible);
    assert_eq!(workbench.state().ui.focus, FocusTarget::CommandPalette);

    for ch in "workspace symbols".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    assert!(workbench.state().ui.input_dialog.visible);

    for ch in "stub".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    assert!(workbench.state().ui.bottom_panel.visible);
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::Symbols
    );
    assert_eq!(workbench.state().ui.focus, FocusTarget::BottomPanel);

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().symbols.items().len() >= 2
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.path.as_ref())
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|p| p == b_path_canon)
    });
}

#[test]
fn test_lsp_utf8_position_encoding_applies_workspace_edit_to_unopened_file() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let b_path = dir.path().join("b.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .set(
            "ZCODE_LSP_STUB_POSITION_ENCODING",
            std::ffi::OsStr::new("utf-8"),
        );

    std::fs::write(&a_path, "fn main() {}\n").unwrap();
    std::fs::write(&b_path, "😀hello\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().lsp.server_capabilities.iter().any(|(k, c)| {
            k.server == LspServerKind::RustAnalyzer
                && c.position_encoding == LspPositionEncoding::Utf8
        })
    });

    let code_action = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(code_action));
    assert!(workbench.state().ui.bottom_panel.visible);
    assert_eq!(
        workbench.state().ui.bottom_panel.active_tab,
        BottomPanelTab::CodeActions
    );
    assert_eq!(workbench.state().ui.focus, FocusTarget::BottomPanel);

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .code_actions
            .items()
            .iter()
            .any(|item| item.title == "Stub: Edit unopened file (multibyte)")
    });

    let enter = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(enter));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        std::fs::read_to_string(&b_path)
            .ok()
            .is_some_and(|text| text == "😀rust\n")
    });
}

#[test]
fn test_lsp_resource_operations_create_rename_delete() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");
    let created_path = dir.path().join("resource_created.rs");
    let old_path = dir.path().join("resource_old.rs");
    let new_path = dir.path().join("resource_new.rs");
    let delete_path = dir.path().join("resource_delete.rs");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();
    std::fs::write(&old_path, "old\n").unwrap();
    std::fs::write(&delete_path, "delete\n").unwrap();
    let _ = std::fs::remove_file(&created_path);
    let _ = std::fs::remove_file(&new_path);

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let code_action = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(code_action));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .code_actions
            .items()
            .iter()
            .any(|item| item.title == "Stub: Resource operations")
    });

    for _ in 0..2 {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let created_ok = created_path
            .metadata()
            .ok()
            .is_some_and(|m| m.is_file() && m.len() == 0);
        let renamed_ok = !old_path.exists()
            && new_path.is_file()
            && std::fs::read_to_string(&new_path)
                .ok()
                .is_some_and(|text| text == "old\n");
        let deleted_ok = !delete_path.exists();
        created_ok && renamed_ok && deleted_ok
    });
}

#[test]
fn test_lsp_completion_resolve_and_confirm_applies_snippet_and_auto_import() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    for _ in 0.."fn main() {}".chars().count() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubSnippet")
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let completion = &w.state().ui.completion;
        let Some(item) = completion.items.get(completion.selected) else {
            return false;
        };
        item.label == "stubSnippet"
            && item.detail.as_deref() == Some("resolved:2")
            && item
                .documentation
                .as_deref()
                .is_some_and(|d| d.contains("stub resolved docs for 2"))
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let expected_line = "fn main() {}stubFn(arg)";
        let arg_start = expected_line.find("arg").unwrap_or(0);
        let arg_end = arg_start.saturating_add("arg".chars().count());

        w.state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| {
                tab.buffer.text() == "use auto_import;\nfn main() {}stubFn(arg)\n"
                    && tab.buffer.cursor() == (1, arg_end)
                    && tab
                        .buffer
                        .selection()
                        .is_some_and(|sel| sel.range() == ((1, arg_start), (1, arg_end)))
            })
    });
}

#[test]
fn test_lsp_completion_auto_import_uses_full_sync_after_multi_edit_apply() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    for _ in 0.."fn main() {}".chars().count() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubSnippet")
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let completion = &w.state().ui.completion;
        let Some(item) = completion.items.get(completion.selected) else {
            return false;
        };
        item.label == "stubSnippet" && item.detail.as_deref() == Some("resolved:2")
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace.contains("didChange detail")
    });

    let first_change = std::fs::read_to_string(&trace_path)
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let payload = line.strip_prefix("didChange detail ")?;
            let parsed: Value = serde_json::from_str(payload).ok()?;
            let version = parsed.get("version")?.as_i64()?;
            let full = parsed
                .get("changes")
                .and_then(Value::as_array)
                .and_then(|changes| changes.first())
                .and_then(|change| change.get("full"))
                .and_then(Value::as_bool)?;
            Some((version, full))
        })
        .unwrap_or((-1, false));

    assert_eq!(first_change, (2, true));

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .filter_map(|line| line.strip_prefix("didChange detail "))
            .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
            .any(|parsed| {
                parsed.get("version").and_then(Value::as_i64) == Some(4)
                    && parsed
                        .get("changes")
                        .and_then(Value::as_array)
                        .and_then(|changes| changes.first())
                        .and_then(|change| change.get("full"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('y'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .filter_map(|line| line.strip_prefix("didChange detail "))
            .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
            .any(|parsed| {
                parsed.get("version").and_then(Value::as_i64) == Some(5)
                    && parsed
                        .get("changes")
                        .and_then(Value::as_array)
                        .and_then(|changes| changes.first())
                        .and_then(|change| change.get("full"))
                        .and_then(Value::as_bool)
                        == Some(false)
            })
    });
}

#[test]
fn test_lsp_quit_sends_shutdown_and_exit() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let quit = KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(quit));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace.contains("request shutdown") && trace.contains("notification exit")
    });
}

#[test]
fn test_lsp_completion_session_reuse_skips_extra_requests() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .remove("ZCODE_LSP_STUB_COMPLETION_INCOMPLETE");

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .first()
                .and_then(|item| item.detail.as_deref())
                .is_some_and(|d| d == "resolved:1")
    });

    let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.trim() == "request textDocument/completion")
            .count(),
        1
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.trim() == "request completionItem/resolve")
            .count(),
        1
    );

    let _ = workbench.handle_input(&InputEvent::Key(completion));

    let started = Instant::now();
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        if started.elapsed() < Duration::from_millis(200) {
            return false;
        }
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .filter(|line| line.trim() == "request textDocument/completion")
            .count()
            == 1
    });
}

#[test]
fn test_lsp_completion_incomplete_disables_session_reuse() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .set(
            "ZCODE_LSP_STUB_COMPLETION_INCOMPLETE",
            std::ffi::OsStr::new("1"),
        );

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible && w.state().ui.completion.is_incomplete
    });

    let _ = workbench.handle_input(&InputEvent::Key(completion));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .filter(|line| line.trim() == "request textDocument/completion")
            .count()
            >= 2
    });
}

#[test]
fn test_completion_popup_does_not_close_on_background_lsp_requests() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .problems
            .items()
            .iter()
            .any(|item| item.message == "didOpen")
    });

    let insert = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible && !w.state().ui.completion.items.is_empty()
    });

    let started = Instant::now();
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        if !w.state().ui.completion.visible {
            panic!("completion popup closed after {:?}", started.elapsed());
        }
        started.elapsed() >= Duration::from_millis(350)
    });
}

#[test]
fn test_idle_hover_does_not_trigger_when_cursor_not_on_identifier() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::RustAnalyzer)
    });

    for ch in "let content = String::from(\"Hello\")".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        drain_runtime_messages(&mut workbench, &rx);
        workbench.tick();
        assert!(
            workbench.state().ui.hover_message.is_none(),
            "unexpected hover message: {:?}",
            workbench.state().ui.hover_message
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn test_hover_response_does_not_show_after_user_input() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .set("ZCODE_LSP_STUB_HOVER_DELAY_MS", std::ffi::OsStr::new("200"));

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::RustAnalyzer)
    });

    let hover = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(hover));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .any(|line| line.trim() == "request textDocument/hover")
    });

    let insert = KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(450) {
        drain_runtime_messages(&mut workbench, &rx);
        workbench.tick();
        assert!(
            workbench.state().ui.hover_message.is_none(),
            "unexpected hover message: {:?}",
            workbench.state().ui.hover_message
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
    assert!(
        trace
            .lines()
            .any(|line| line.trim() == "notification $/cancelRequest"),
        "expected cancelRequest notification, trace:\n{trace}"
    );
}

#[test]
fn test_completion_closes_after_deleting_trigger() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::RustAnalyzer)
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('.'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubItem")
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(400) {
        drain_runtime_messages(&mut workbench, &rx);
        workbench.tick();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(!workbench.state().ui.completion.visible);
    assert!(workbench.state().ui.completion.items.is_empty());
    assert!(workbench.state().ui.completion.request.is_none());
    assert!(workbench.state().ui.completion.pending_request.is_none());
}

#[test]
fn test_completion_filters_items_while_typing() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::RustAnalyzer)
    });

    let completion = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(completion));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.completion.visible
            && w.state()
                .ui
                .completion
                .items
                .iter()
                .any(|item| item.label == "stubSnippet")
    });

    for ch in "stubI".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }

    let labels = workbench
        .state()
        .ui
        .completion
        .items
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["stubItem", "stubItem2"]);
}

#[test]
fn test_semantic_tokens_apply_expected_highlight_kinds() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .iter()
            .any(|(k, c)| k.server == LspServerKind::RustAnalyzer && c.semantic_tokens)
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.semantic_highlight_lines(0, 1) else {
            return false;
        };
        lines.first().is_some_and(|spans| {
            spans.contains(&HighlightSpan {
                start: 0,
                end: 2,
                kind: HighlightKind::Keyword,
            }) && spans.contains(&HighlightSpan {
                start: 3,
                end: 7,
                kind: HighlightKind::Function,
            })
        })
    });
}

#[test]
fn test_inlay_hints_are_applied_for_single_line_file() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "let mut logging_guard = logging::init();").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.inlay_hint_lines(0, 1) else {
            return false;
        };
        lines
            .first()
            .is_some_and(|hints| hints.iter().any(|hint| hint == ": hint0"))
    });

    let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
    assert!(
        trace
            .lines()
            .any(|line| line.trim() == "request textDocument/inlayHint"),
        "expected inlayHint request in lsp trace"
    );
}

#[test]
fn test_inlay_hints_refresh_after_viewport_size_change() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    let mut content = String::new();
    for i in 0..60usize {
        content.push_str(&format!("// line {i}\n"));
    }
    std::fs::write(&a_path, content).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    // Render once so `SetViewportSize` is dispatched; the follow-up inlay-hints request should
    // expand beyond the default viewport height (20 lines).
    {
        let mut backend = zcode::ui::backend::test::TestBackend::new(120, 80);
        workbench.render(
            &mut backend,
            zcode::ui::core::geom::Rect::new(0, 0, 120, 80),
        );
        let _ = workbench.flush_post_render_sync();
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.inlay_hint_lines(22, 23) else {
            return false;
        };
        lines
            .first()
            .is_some_and(|hints| hints.iter().any(|hint| hint == ": hint22"))
    });
}

#[test]
fn test_inlay_hints_refresh_after_mouse_scroll() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    let mut content = String::new();
    for i in 0..200usize {
        content.push_str(&format!("// line {i}\n"));
    }
    std::fs::write(&a_path, content).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    // Render once so editor mouse events have a hit-testable area.
    {
        let mut backend = zcode::ui::backend::test::TestBackend::new(120, 50);
        workbench.render(
            &mut backend,
            zcode::ui::core::geom::Rect::new(0, 0, 120, 50),
        );
        let _ = workbench.flush_post_render_sync();
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.inlay_hint_lines(0, 1) else {
            return false;
        };
        lines
            .first()
            .is_some_and(|hints| hints.iter().any(|hint| hint == ": hint0"))
    });

    let scroll = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: KeyModifiers::NONE,
    };
    let _ = workbench.handle_input(&InputEvent::Mouse(scroll));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };

        // Ensure the viewport moved and a follow-up inlay-hints request replaced the range.
        if tab.viewport.line_offset == 0 {
            return false;
        }

        let start = tab.viewport.line_offset;
        let target = format!(": hint{start}");

        tab.inlay_hint_lines(0, 1).is_none()
            && tab
                .inlay_hint_lines(start, start + 1)
                .and_then(|lines| lines.first())
                .is_some_and(|hints| hints.iter().any(|hint| hint == &target))
    });
}

#[test]
fn test_semantic_tokens_range_is_used_for_large_files() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    let mut content = String::new();
    for _ in 0..2100 {
        content.push_str("fn main() {}\n");
    }
    std::fs::write(&a_path, content).unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .problems
            .items()
            .iter()
            .any(|item| item.message == "didOpen")
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |_| {
        let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
        trace
            .lines()
            .any(|line| line.trim() == "request textDocument/semanticTokens/range")
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.semantic_highlight_lines(0, 1) else {
            return false;
        };
        lines.first().is_some_and(|spans| {
            spans.contains(&HighlightSpan {
                start: 0,
                end: 2,
                kind: HighlightKind::Keyword,
            }) && spans.contains(&HighlightSpan {
                start: 3,
                end: 7,
                kind: HighlightKind::Function,
            })
        })
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(
        workbench
            .state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.semantic_highlight_lines(0, 1))
            .is_some(),
        "semantic highlight unexpectedly cleared after edit"
    );
}

#[test]
fn test_signature_help_closes_after_cursor_leaves_call() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str());

    std::fs::write(&a_path, "").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state()
            .lsp
            .server_capabilities
            .keys()
            .any(|k| k.server == LspServerKind::RustAnalyzer)
    });

    for ch in "String::from".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char('('),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        w.state().ui.signature_help.visible && !w.state().ui.signature_help.text.trim().is_empty()
    });

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Char(';'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(400) {
        drain_runtime_messages(&mut workbench, &rx);
        workbench.tick();
        assert!(
            !workbench.state().ui.signature_help.visible,
            "signature help popup did not close: {:?}",
            workbench.state().ui.signature_help
        );
        assert!(workbench.state().ui.signature_help.request.is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn test_lsp_progress_end_triggers_inlay_hints_refresh() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let stub_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_zcode_lsp_stub"));
    assert!(
        stub_path.is_file(),
        "stub binary missing at {}",
        stub_path.display()
    );

    let dir = tempdir().unwrap();
    let a_path = dir.path().join("a.rs");
    let trace_path = dir.path().join("lsp_trace.txt");

    let _env = EnvGuard::set_str("ZCODE_DISABLE_SETTINGS", "1")
        .remove("ZCODE_DISABLE_LSP")
        .set("ZCODE_LSP_COMMAND", stub_path.as_os_str())
        .remove("ZCODE_LSP_ARGS")
        .set("ZCODE_LSP_STUB_TRACE_PATH", trace_path.as_os_str())
        .set("ZCODE_LSP_STUB_SEND_PROGRESS", std::ffi::OsStr::new("1"));

    std::fs::write(&a_path, "fn main() {}\n").unwrap();

    let (runtime, rx) = create_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();
    assert!(workbench.has_lsp_service());

    workbench.handle_message(AppMessage::FileLoaded {
        path: a_path.clone(),
        content: std::fs::read_to_string(&a_path).unwrap(),
    });

    // The stub sends $/progress end right after initialized, which should
    // trigger an immediate inlay hints refresh via the debounce deadline.
    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let Some(tab) = w.state().editor.pane(0).and_then(|pane| pane.active_tab()) else {
            return false;
        };
        let Some(lines) = tab.inlay_hint_lines(0, 1) else {
            return false;
        };
        lines
            .first()
            .is_some_and(|hints| hints.iter().any(|hint| hint == ": hint0"))
    });

    let trace = std::fs::read_to_string(&trace_path).unwrap_or_default();
    assert!(
        trace
            .lines()
            .any(|line| line.trim() == "request textDocument/inlayHint"),
        "expected inlayHint request in lsp trace"
    );
}
