<div align="center">

# zcode

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![English](https://img.shields.io/badge/README-English-green.svg)](README.md)
[![Chinese](https://img.shields.io/badge/README-中文-red.svg)](README_zh.md)

</div>

`zcode` is a modern, high-performance TUI (Terminal User Interface) text editor written in Rust.

## Table of Contents

- [Quick Start](#quick-start)
- [Install](#install)
- [Usage Guide](#usage-guide)
  - [Keybindings](#keybindings)
  - [Mouse Support](#mouse-support)
- [Core Features](#core-features)
- [Configuration](#configuration)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)

## Quick Start

### Prerequisites

*   [Rust toolchain](https://rustup.rs/) (recommended: the version pinned in `rust-toolchain.toml`)
*   Linux: a C toolchain for linking (e.g. `cc`/`gcc`)
*   A terminal font with Nerd Font glyphs (recommended for UI icons)

### Launching the Editor

Run the following command in the project root:

```bash
# 1. Open current directory
cargo run

# 2. Open current directory (explicit)
cargo run -- .

# 3. Open a specific file
cargo run -- src/main.rs
```

### Recommended: Release Mode

For the best performance and smoothest experience (avoiding potential debug-build lag), use release mode:

```bash
cargo run --release -- .
```

## Install

Build and install `zcode` to your Cargo bin directory (usually `~/.cargo/bin`):

```bash
cargo install --path . --locked
zcode .
```

Alternatively, use the helper script:

```bash
./install.sh --user
# or: sudo ./install.sh --system
```

## Usage Guide

### Keybindings

Default key mappings are as follows:

*   **General**:
    *   `Ctrl + q`: Quit editor
    *   `Ctrl + s`: Save current file
    *   `Ctrl + ,`: Open Settings
    *   `Ctrl + b`: Toggle Sidebar visibility

*   **Editor**:
    *   `Ctrl + f`: Open Find in file
    *   `Ctrl + h`: Open Replace
    *   `F3` / `Ctrl + g`: Find Next
    *   `Shift + F3` / `Ctrl + Shift + g`: Find Previous
    *   `Ctrl + \`: Split Editor Vertically
    *   `Ctrl + Shift + \`: Close Split
    *   `Ctrl + Shift + e`: Focus Explorer
    *   `Ctrl + Shift + f`: Focus Global Search

*   **Tabs**:
    *   `Ctrl + w`: Close current tab
    *   `Ctrl + Tab`: Switch to next tab
    *   `Ctrl + Shift + Tab`: Switch to previous tab

*   **Cursor & Selection**:
    *   `Ctrl + a`: Select All
    *   `Ctrl + c` / `x` / `v`: Copy / Cut / Paste
    *   `Ctrl + z`: Undo
    *   `Ctrl + y`: Redo

### Mouse Support

`zcode` has comprehensive mouse support:

*   **Navigation**: Click folders in the Explorer to expand/collapse, double-click files to open.
*   **Tab Management**:
    *   **Left Click**: Switch tab.
    *   **Middle Click**: Close tab.
    *   **Click 'x'**: Close tab.
*   **Splits**: Drag the divider between editors to resize splits.
*   **Cursor**: Click anywhere in the editor to move the cursor.

## Core Features

*   **⚡️ High-Performance Async Architecture**: Powered by `tokio`, handling file I/O and global search asynchronously. This ensures the main UI thread remains buttery smooth, never blocking on large file loads or heavy searches.
*   **🎨 Modern UI**: Built on `ratatui` and `crossterm`, featuring a Sidebar, Activity Bar, Bottom Panel, Tabs, and flexible Split Panes.
*   **🔍 Powerful Search**: Built-in `ripgrep`-based engine for high-performance global search and real-time in-file finding.
*   **⌨️ Key Mapping**: Flexible keybinding configuration support.
*   **📋 Clipboard Integration**: Seamless system clipboard support.

## Configuration

Settings are stored in:

*   Linux: `~/.cache/.zcode/setting.json`
*   macOS: `~/Library/Caches/.zcode/setting.json`

## Troubleshooting

### `error: linker cc not found`

Install a C toolchain.

*   Ubuntu/Debian: `sudo apt install build-essential`
*   Fedora/RHEL: `sudo dnf install gcc`
*   Arch: `sudo pacman -S base-devel`

### Clipboard unavailable on Linux

Install one of the following:

*   Wayland: `wl-clipboard` (`wl-copy` / `wl-paste`)
*   X11: `xclip` or `xsel`

### Rust LSP (`rust-analyzer`) not found

Make sure `rust-analyzer` is installed and available in PATH. If you use `rustup`:

```bash
rustup component add rust-analyzer rust-src
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## License

GPL-3.0. See `LICENSE`.
