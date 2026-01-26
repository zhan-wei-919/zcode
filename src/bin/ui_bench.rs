use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zcode::app::Workbench;
use zcode::core::event::InputEvent;
use zcode::core::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use zcode::core::Command;
use zcode::kernel::services::adapters::perf;
use zcode::kernel::services::adapters::AsyncRuntime;
use zcode::tui::view::View;

fn main() -> std::io::Result<()> {
    let mut frames: usize = 300;
    let mut events: usize = 100_000;
    let mut width: u16 = 120;
    let mut height: u16 = 40;
    let mut panes: usize = 2;
    let mut tabs_per_pane: usize = 5;
    let mut explorer_files: usize = 200;
    let mut normal_lines: usize = 2000;
    let mut normal_cols: usize = 80;
    let mut long_lines: usize = 200;
    let mut long_cols: usize = 2000;

    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--frames=") {
            frames = value.parse().unwrap_or(frames);
        } else if let Some(value) = arg.strip_prefix("--events=") {
            events = value.parse().unwrap_or(events);
        } else if let Some(value) = arg.strip_prefix("--width=") {
            width = value.parse().unwrap_or(width);
        } else if let Some(value) = arg.strip_prefix("--height=") {
            height = value.parse().unwrap_or(height);
        } else if let Some(value) = arg.strip_prefix("--panes=") {
            panes = value.parse().unwrap_or(panes);
        } else if let Some(value) = arg.strip_prefix("--tabs=") {
            tabs_per_pane = value.parse().unwrap_or(tabs_per_pane);
        } else if let Some(value) = arg.strip_prefix("--explorer-files=") {
            explorer_files = value.parse().unwrap_or(explorer_files);
        } else if let Some(value) = arg.strip_prefix("--lines=") {
            normal_lines = value.parse().unwrap_or(normal_lines);
        } else if let Some(value) = arg.strip_prefix("--cols=") {
            normal_cols = value.parse().unwrap_or(normal_cols);
        } else if let Some(value) = arg.strip_prefix("--long-lines=") {
            long_lines = value.parse().unwrap_or(long_lines);
        } else if let Some(value) = arg.strip_prefix("--long-cols=") {
            long_cols = value.parse().unwrap_or(long_cols);
        }
    }

    let panes = panes.clamp(1, 2);
    let tabs_per_pane = tabs_per_pane.max(1);
    let required_tabs = panes.saturating_mul(tabs_per_pane);
    let explorer_files = explorer_files.max(required_tabs);

    let root = create_fixture_dir(explorer_files)?;
    let (tx, _rx) = mpsc::channel();
    let runtime = AsyncRuntime::new(tx)?;
    let mut workbench = Workbench::new(&root, runtime, None)?;

    if panes == 2 {
        workbench.bench_run_command(Command::SplitEditorVertical);
    }

    let normal = generate_rust_like(normal_lines, normal_cols);
    let long = generate_rust_like(long_lines, long_cols);
    let small = "fn main() {}\n".to_string();

    for pane in 0..panes {
        for tab in 0..tabs_per_pane {
            let idx = pane.saturating_mul(tabs_per_pane).saturating_add(tab);
            let path = root.join("src").join(format!("file_{idx}.rs"));
            let content = if tab + 1 == tabs_per_pane {
                long.clone()
            } else if tab == 0 {
                normal.clone()
            } else {
                small.clone()
            };
            workbench.bench_open_file(pane, path, content);
        }
    }

    workbench.bench_set_active_pane(0);
    workbench.bench_run_command(Command::FocusEditor);
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;

    for _ in 0..10 {
        terminal.draw(|frame| {
            let area = frame.area();
            workbench.render(frame, area);
        })?;
    }

    let render_start = Instant::now();
    for _ in 0..frames {
        terminal.draw(|frame| {
            let area = frame.area();
            workbench.render(frame, area);
        })?;
    }
    let render_elapsed = render_start.elapsed();
    print_render_summary(frames, render_elapsed);

    let key_events: [(KeyCode, KeyModifiers); 4] = [
        (KeyCode::Char('a'), KeyModifiers::NONE),
        (KeyCode::Backspace, KeyModifiers::NONE),
        (KeyCode::Right, KeyModifiers::NONE),
        (KeyCode::Left, KeyModifiers::NONE),
    ];

    let input_start = Instant::now();
    for i in 0..events {
        let (code, modifiers) = key_events[i % key_events.len()];
        let key_event = KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(key_event));
    }
    let input_elapsed = input_start.elapsed();
    print_input_summary(events, input_elapsed);

    let report = perf::report_and_reset();
    if !report.is_empty() {
        println!("perf:\n{}", report);
    }

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

fn create_fixture_dir(src_files: usize) -> std::io::Result<PathBuf> {
    let mut root = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    root.push(format!("zcode-ui-bench-{pid}-{nanos}"));
    std::fs::create_dir_all(&root)?;

    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("docs"))?;
    std::fs::write(root.join("README.md"), "# bench\n")?;

    for i in 0..src_files {
        std::fs::write(
            root.join("src").join(format!("file_{i}.rs")),
            "fn main() {}\n",
        )?;
    }

    Ok(root)
}

fn generate_rust_like(lines: usize, cols: usize) -> String {
    if lines == 0 || cols == 0 {
        return String::new();
    }

    let prefix = "fn bench() { let n: usize = 123; let s = \"hello\"; } // ";
    let mut out = String::with_capacity(lines.saturating_mul(cols + 1));
    for i in 0..lines {
        out.push_str(prefix);
        out.push_str(&i.to_string());
        if cols > prefix.len() + 16 {
            let pad = cols.saturating_sub(prefix.len() + 16);
            out.push_str(&"a".repeat(pad.saturating_sub(1)));
        }
        out.push('\n');
    }
    out
}

fn print_render_summary(frames: usize, elapsed: Duration) {
    let per_frame = elapsed.as_secs_f64() / frames.max(1) as f64;
    let fps = if per_frame > 0.0 {
        1.0 / per_frame
    } else {
        0.0
    };
    println!(
        "render: frames={frames} total={:?} us/frame={:.2} fps={:.1}",
        elapsed,
        per_frame * 1_000_000.0,
        fps
    );
}

fn print_input_summary(events: usize, elapsed: Duration) {
    let per_event = elapsed.as_secs_f64() / events.max(1) as f64;
    let eps = if per_event > 0.0 {
        1.0 / per_event
    } else {
        0.0
    };
    println!(
        "input: events={events} total={:?} ns/event={:.1} events/s={:.0}",
        elapsed,
        per_event * 1_000_000_000.0,
        eps
    );
}
