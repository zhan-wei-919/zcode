<div align="center">

# zcode

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![English](https://img.shields.io/badge/README-English-green.svg)](README.md)
[![Chinese](https://img.shields.io/badge/README-中文-red.svg)](README_zh.md)

</div>

`zcode` is a high-performance, modern TUI (Terminal User Interface) text editor written in Rust. It features a Flux/Redux-like state management architecture combined with the Tokio asynchronous runtime, delivering efficient I/O handling for a smooth terminal coding experience.

## Table of Contents

- [Quick Start](#quick-start)
- [Usage Guide](#usage-guide)
- [Core Features](#core-features)
- [Architecture](#architecture)
- [Modules](#modules)

## Quick Start

### Prerequisites

*   [Rust Toolchain](https://rustup.rs/) (1.70+)

### Launching the Editor

Run the following command in the project root:

```bash
# 1. Open current directory
cargo run -- .

# 2. Open a specific file
cargo run -- src/main.rs
```

### Recommended: Release Mode

For the best performance and smoothest experience (avoiding potential debug-build lag), use release mode:

```bash
cargo run --release -- .
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

## Architecture

This project follows a clear layered architecture, primarily adhering to the **Model-View-Intent (MVI)** or **Flux/Redux** unidirectional data flow pattern.

### Core Data Flow

1.  **Event**: User input (keyboard, mouse) or system messages.
2.  **Dispatcher**: Converts Events into business-logic `Action`s.
3.  **Store**: Receives `Action`s and updates the global `State`.
4.  **View**: Renders the UI based on the latest `State`.

## Modules

### 1. Kernel - `src/kernel/`

The "brain" of the editor. Pure logic, independent of specific UI rendering libraries.

*   **State (`state.rs`)**:
    *   The Single Source of Truth. Contains UI layout state (visible sidebars, active tabs), editor data (open files, cursor positions), and caches.
*   **Store (`store.rs`)**:
    *   State container. Holds the State, receives Actions, and executes state transitions (Reducers).
    *   Manages Side Effect triggers.
*   **Actions (`action.rs`)**:
    *   Defines all operations that can mutate state, e.g., `EditorAction::InsertText`, `WorkbenchAction::ToggleSidebar`.

### 2. App Layer - `src/app/`

Connects the Kernel to the TUI rendering, with the **Workbench** being the core component.

*   **Workbench (`src/app/workbench/`)**:
    *   **Lifecycle Management**: Initializes services, loads config, starts and maintains the main Event Loop.
    *   **Event Dispatch**: Listens to raw `crossterm` events, converts them to `InputEvent`s, and dispatches them to the Kernel.
    *   **Layout**: Calculates the size and position (Rect) of screen areas (Sidebar, Editor, Status Bar) and delegates rendering tasks.

### 3. Views - `src/views/`

Responsible for specific UI component rendering logic.

*   **Editor**: Complex text rendering, scrolling, line numbers, syntax highlighting (basic), cursor tracking.
*   **Explorer**: Tree view for the file system, handling folder expansion/collapse.
*   **Search**: Search panel view, displaying global search results.

### 4. Services & Adapters - `src/kernel/services/`

Handles interactions with the OS and the external world, typically involving I/O and async tasks.

*   **AsyncRuntime**: Wraps the `tokio` runtime, allowing expensive tasks to run outside the synchronous TUI render loop.
*   **GlobalSearchService**: Wraps underlying search tools (like `ignore` and `grep` crates) to provide async search capabilities.
*   **Clipboard**: Wraps `arboard` for cross-platform clipboard access.

### 5. Models - `src/models/`

*   **TextBuffer**: Implemented using `ropey` (Rope data structure). Critical for high-performance editing of large files (O(log N) complexity for inserts/deletes), avoiding massive memory copies associated with standard Strings.
*   **EditHistory**: Manages the Undo and Redo stacks.
*   **FileTree**: Recursive file tree structure for the Explorer.
