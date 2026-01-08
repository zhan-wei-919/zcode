# Repository Guidelines

## Project Structure & Module Organization

- `Cargo.toml` / `Cargo.lock`: Rust package manifest + pinned dependencies.
- `src/`: main codebase.
  - `lib.rs`: library entrypoint (shared code for the binary and tests).
  - `main.rs`: TUI executable entrypoint (crossterm + ratatui).
  - `app/`, `views/`: UI composition and widgets (editor/explorer/search panels).
  - `core/`: shared traits/types (events, views, commands, service registry).
  - `models/`: data structures (text buffer, edit history, file tree, selections).
  - `kernel/services/`: shared contracts (`ports/`) + implementations (`adapters/`).
  - `runtime/`: async plumbing (tokio + message passing).
- `tests/`: integration tests (Rust test harness).
- `docs/`: design notes and plans.

## Build, Test, and Development Commands

- `cargo build`: compile the crate.
- `cargo test`: run all unit + integration tests.
- `cargo run -- <path>`: run the TUI editor for a file/dir (needs a real TTY).
- `cargo test --offline`: run tests without network (useful in restricted envs).
- `cargo fmt` / `cargo clippy`: format and lint before submitting changes.
- `RUST_LOG=zcode=debug cargo run -- <path>`: enable verbose logs.

## Coding Style & Naming Conventions

- Rust 2021; use `rustfmt` defaults (4-space indentation).
- Performance-first: prefer better asymptotic complexity (aim for `O(log n)` when feasible), without CPU-cycle micro-optimizations.
- Keep diffs focused; avoid “drive-by” refactors.
- Minimize comments: use a short file header for context and brief notes only on complex/abstract parts.
- Keep structure elegant: small, cohesive modules; clear boundaries; avoid deep nesting and tangled dependencies.
- Naming: modules/functions in `snake_case`, types in `CamelCase`, constants in `SCREAMING_SNAKE_CASE`.

## Testing Guidelines

- Prefer unit tests in the module’s `#[cfg(test)] mod tests` block.
- Put cross-module / black-box tests in `tests/`.
- Keep tests deterministic; use `tempfile` for filesystem tests.

## Logging & Debugging

- Logging uses `tracing` and writes to a daily-rotated file under the platform data dir (macOS: `~/Library/Application Support/zcode/logs/`; falls back to a temp dir if needed).
- Avoid printing to stdout/stderr during TUI runtime; use `tracing::{info,warn,error}`.

## Settings

- UI colors and **global** keybindings can be overridden via the platform cache file:
  - macOS: `~/Library/Caches/.zcode/setting.json`
  - Linux: `$XDG_CACHE_HOME/.zcode/setting.json` (or `~/.cache/.zcode/setting.json`)
  - Windows: `%LOCALAPPDATA%\\.zcode\\setting.json` (fallback: `%APPDATA%\\.zcode\\setting.json`)
- Format:
  - `keybindings`: list of `{ "key": "ctrl+shift+p", "command": "commandPalette" }` (empty `command` unbinds)
  - `theme`: color names like `cyan`, `dark_gray`, `white` or hex like `#RRGGBB`

## Commit & Pull Request Guidelines

- Git history uses short, descriptive summaries (often Chinese). Follow the same: one-line summary + optional details.
- PRs should include: what/why, steps to verify, and screenshots/GIFs for UI changes.
- Before PR: run `cargo fmt`, `cargo clippy`, and `cargo test`.

## 代码风格
 
- 性能优先： 一切都要以最高理论性能为目标，如果理论上能达到O(log n)复杂度，那么我们应该尽量靠齐，但是不一定要追求CPU周期级别的优化。
- 尽量少写注释： 可以在文件开头写，复杂的函数开头写，比较抽象的类里面的属性后面写。优秀的代码风格能起到注释的功能
- 代码结构优雅
- 尽量避免耦合，不要出现所有的依赖都在同一个文件层次中
