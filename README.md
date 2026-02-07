<div align="center">

# zcode

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![English](https://img.shields.io/badge/README-English-green.svg)](README.md)
[![Chinese](https://img.shields.io/badge/README-中文-red.svg)](README_zh.md)

</div>

`zcode` is a modern, high-performance TUI (Terminal User Interface) text editor written in Rust.

![main_UI.png](img/main_UI.png)

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

* [Rust toolchain](https://rustup.rs/) (recommended: the version pinned in `rust-toolchain.toml`)
* Linux: a C toolchain for linking (e.g. `cc`/`gcc`) (needed for building tree-sitter grammars)
* A terminal font with Nerd Font glyphs (recommended for UI icons)
* Optional (for LSP features): `rust-analyzer`, `gopls`, `pyright-langserver`, `typescript-language-server`

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
zcode
```

Alternatively, use the helper script:

```bash
./install.sh --user
# or: sudo ./install.sh --system
```

If you use `./install.sh --user`, make sure `~/.local/bin` is in your `PATH`.

## Usage Guide

### Keybindings

Default key mappings are as follows:

* **General**:
  
  * `Ctrl + q`: Quit editor
  * `Ctrl + s`: Save current file
  * `Ctrl + ,`: Open Settings
  * `Ctrl + b`: Toggle Sidebar visibility

* **Editor**:
  
  * `Ctrl + f`: Open Find in file
  * `Ctrl + h`: Open Replace
  * `F3` / `Ctrl + g`: Find Next
  * `Shift + F3` / `Ctrl + Shift + g`: Find Previous
  * `Ctrl + \`: Split Editor Vertically
  * `Ctrl + Shift + \`: Close Split
  * `Ctrl + Shift + e`: Focus Explorer
  * `Ctrl + Shift + f`: Focus Global Search

* **Tabs**:
  
  * `Ctrl + w`: Close current tab
  * `Ctrl + Tab`: Switch to next tab
  * `Ctrl + Shift + Tab`: Switch to previous tab

* **Cursor & Selection**:
  
  * `Ctrl + a`: Select All
  * `Ctrl + c` / `x` / `v`: Copy / Cut / Paste
  * `Ctrl + z`: Undo
  * `Ctrl + y`: Redo

* **LSP** (when a language server is available):
  
  * `F2`: Hover
  * `F12`: Go to Definition
  * `Shift + F12`: Find References
  * `Alt + Enter`: Code Action
  * `Ctrl + Space`: Completion
  * `Ctrl + Shift + r`: Rename

### Mouse Support

`zcode` has comprehensive mouse support:

* **Navigation**: Click folders in the Explorer to expand/collapse, double-click files to open.
* **Tab Management**:
  * **Left Click**: Switch tab.
  * **Middle Click**: Close tab.
  * **Click 'x'**: Close tab.
* **Splits**: Drag the divider between editors to resize splits.
* **Cursor**: Click anywhere in the editor to move the cursor.

## Core Features

* **Fast TUI editor**: Built on `ratatui` and `crossterm`, featuring a Sidebar, Bottom Panel, Tabs, and split panes.
* **Multi-language syntax highlighting**: Tree-sitter highlight for Rust/Go/Python/JavaScript/TypeScript (incl. JSX/TSX).
* **Multi-language LSP support** (optional): Diagnostics, hover, completion, go-to-definition, etc, for Rust/Go/Python/JS/TS.
  * Monorepo-friendly: LSP root is detected per language by searching the nearest marker file (then spawns per-(language,root)).
  * Server discovery: prefers project-local `node_modules/.bin` and Python virtualenvs when available.
* **Search**: Built-in `ripgrep`-based engine for high-performance global search and real-time in-file finding.
* **Key Mapping**: Flexible keybinding configuration support.
* **Clipboard Integration**: Seamless system clipboard support.
* **Theme Editor**: Built-in visual theme editor for customizing syntax highlighting colors. Open via Command Palette (`Ctrl + Shift + p` > "Open Theme Editor").
  * **Color Picker**: Hue Bar (vertical strip) + Saturation/Lightness palette. Click or drag to pick colors; the code preview updates in real time.
  * **Auto-save**: Color changes are automatically saved to `setting.json` (debounced 300ms). No manual save needed.
  * **Keyboard**: `Tab` to cycle focus (Token List / Hue Bar / SL Palette). Arrow keys to adjust values (`Shift` for larger steps). `l` to switch preview language. `Ctrl + r` to reset the selected token to its default color. `Esc` to close.
  * **Mouse**: Click or drag on the Hue Bar to change hue. Click or drag on the SL Palette to change saturation/lightness. Click a token in the list to select it.

## Configuration

Settings are stored in:

* Linux: `~/.cache/.zcode/setting.json`
* macOS: `~/Library/Caches/.zcode/setting.json`

### LSP configuration

You can override per-language LSP server command/args and optional initialize options in `setting.json`:

```json
{
  "lsp": {
    "servers": {
      "rust-analyzer": { "command": "rust-analyzer" },
      "gopls": { "command": "/home/you/go/bin/gopls" },
      "pyright": { "command": "pyright-langserver", "args": ["--stdio"] },
      "tsls": {
        "command": "typescript-language-server",
        "args": ["--stdio"],
        "initialization_options": {
          "preferences": {
            "includeCompletionsForModuleExports": true
          }
        }
      }
    }
  }
}
```

For Go, if `gopls` is installed under `~/go/bin`, prefer setting an absolute command path as above.

## Troubleshooting

### Only Unix-like operating systems are supported.

### `error: linker cc not found`

Install a C toolchain.

* Ubuntu/Debian: `sudo apt install build-essential`
* Fedora/RHEL: `sudo dnf install gcc`
* Arch: `sudo pacman -S base-devel`

### Clipboard unavailable on Linux

Install one of the following:

* Wayland: `wl-clipboard` (`wl-copy` / `wl-paste`)
* X11: `xclip` or `xsel`

### LSP server not found

`zcode` enables LSP by default, but the language servers are optional. Install the servers you want:

* Rust (`rust-analyzer`):
  
  ```bash
  rustup component add rust-analyzer rust-src
  ```

* Go (`gopls`):
  
  ```bash
  go install golang.org/x/tools/gopls@latest
  ```

* Python (`pyright-langserver`):
  
  ```bash
  npm i -g pyright
  # or: pipx install pyright
  ```

* JavaScript/TypeScript (`typescript-language-server`):
  
  ```bash
  npm i -g typescript-language-server typescript
  # or per-project: npm i -D typescript-language-server typescript
  ```

If you install JS/TS servers per-project, `zcode` will auto-detect `node_modules/.bin` (searching upwards from the detected project root).

### macOS Terminal mouse occasionally unresponsive

On macOS Terminal.app, there is a very small chance that mouse input stops working. This is a Terminal.app bug — restarting Terminal usually fixes it.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## License

GPL-3.0. See `LICENSE`.
