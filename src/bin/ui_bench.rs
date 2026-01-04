use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zcode::app::Workbench;
use zcode::core::event::InputEvent;
use zcode::core::View;
use zcode::kernel::services::adapters::{AppMessage, AsyncRuntime};
use zcode::kernel::services::adapters::perf;

fn main() -> std::io::Result<()> {
    let mut frames: usize = 300;
    let mut events: usize = 100_000;
    let mut width: u16 = 120;
    let mut height: u16 = 40;

    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--frames=") {
            frames = value.parse().unwrap_or(frames);
        } else if let Some(value) = arg.strip_prefix("--events=") {
            events = value.parse().unwrap_or(events);
        } else if let Some(value) = arg.strip_prefix("--width=") {
            width = value.parse().unwrap_or(width);
        } else if let Some(value) = arg.strip_prefix("--height=") {
            height = value.parse().unwrap_or(height);
        }
    }

    let root = create_fixture_dir()?;
    let (tx, _rx) = mpsc::channel();
    let runtime = AsyncRuntime::new(tx);
    let mut workbench = Workbench::new(&root, runtime, None)?;

    let file_path = root.join("big.txt");
    let content = generate_text(2000, 80);
    std::fs::write(&file_path, &content)?;
    workbench.handle_message(AppMessage::FileLoaded { path: file_path, content });

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
            state: KeyEventState::NONE,
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

fn create_fixture_dir() -> std::io::Result<PathBuf> {
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

    for i in 0..200 {
        std::fs::write(root.join("src").join(format!("file_{i}.rs")), "fn main() {}\n")?;
    }

    Ok(root)
}

fn generate_text(lines: usize, cols: usize) -> String {
    let line = "a".repeat(cols.saturating_sub(1));
    let mut out = String::with_capacity(lines.saturating_mul(cols + 1));
    for _ in 0..lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn print_render_summary(frames: usize, elapsed: Duration) {
    let per_frame = elapsed.as_secs_f64() / frames.max(1) as f64;
    let fps = if per_frame > 0.0 { 1.0 / per_frame } else { 0.0 };
    println!(
        "render: frames={frames} total={:?} us/frame={:.2} fps={:.1}",
        elapsed,
        per_frame * 1_000_000.0,
        fps
    );
}

fn print_input_summary(events: usize, elapsed: Duration) {
    let per_event = elapsed.as_secs_f64() / events.max(1) as f64;
    let eps = if per_event > 0.0 { 1.0 / per_event } else { 0.0 };
    println!(
        "input: events={events} total={:?} ns/event={:.1} events/s={:.0}",
        elapsed,
        per_event * 1_000_000_000.0,
        eps
    );
}
