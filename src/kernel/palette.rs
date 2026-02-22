use crate::core::Command;

pub struct PaletteMatch<'a> {
    pub label: &'a str,
    pub command: &'a Command,
}

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: &'static str,
    pub label_lc: &'static str,
    pub command: Command,
}

pub static PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem {
        label: "View: Toggle Sidebar",
        label_lc: "view: toggle sidebar",
        command: Command::ToggleSidebar,
    },
    PaletteItem {
        label: "View: Focus Explorer",
        label_lc: "view: focus explorer",
        command: Command::FocusExplorer,
    },
    PaletteItem {
        label: "Explorer: New File",
        label_lc: "explorer: new file",
        command: Command::ExplorerNewFile,
    },
    PaletteItem {
        label: "Explorer: New Folder",
        label_lc: "explorer: new folder",
        command: Command::ExplorerNewFolder,
    },
    PaletteItem {
        label: "Explorer: Delete",
        label_lc: "explorer: delete",
        command: Command::ExplorerDelete,
    },
    PaletteItem {
        label: "Explorer: Cut",
        label_lc: "explorer: cut",
        command: Command::ExplorerCut,
    },
    PaletteItem {
        label: "Explorer: Copy",
        label_lc: "explorer: copy",
        command: Command::ExplorerCopy,
    },
    PaletteItem {
        label: "Explorer: Paste",
        label_lc: "explorer: paste",
        command: Command::ExplorerPaste,
    },
    PaletteItem {
        label: "View: Focus Search",
        label_lc: "view: focus search",
        command: Command::FocusSearch,
    },
    PaletteItem {
        label: "View: Toggle Sidebar Tab",
        label_lc: "view: toggle sidebar tab",
        command: Command::ToggleSidebarTab,
    },
    PaletteItem {
        label: "View: Focus Editor",
        label_lc: "view: focus editor",
        command: Command::FocusEditor,
    },
    PaletteItem {
        label: "View: Split Editor (Vertical)",
        label_lc: "view: split editor (vertical)",
        command: Command::SplitEditorVertical,
    },
    PaletteItem {
        label: "View: Split Editor (Horizontal)",
        label_lc: "view: split editor (horizontal)",
        command: Command::SplitEditorHorizontal,
    },
    PaletteItem {
        label: "View: Close Editor Split",
        label_lc: "view: close editor split",
        command: Command::CloseEditorSplit,
    },
    PaletteItem {
        label: "View: Focus Next Editor Pane",
        label_lc: "view: focus next editor pane",
        command: Command::FocusNextEditorPane,
    },
    PaletteItem {
        label: "View: Focus Prev Editor Pane",
        label_lc: "view: focus prev editor pane",
        command: Command::FocusPrevEditorPane,
    },
    PaletteItem {
        label: "LSP: Hover",
        label_lc: "lsp: hover",
        command: Command::LspHover,
    },
    PaletteItem {
        label: "LSP: Go to Definition",
        label_lc: "lsp: go to definition",
        command: Command::LspDefinition,
    },
    PaletteItem {
        label: "LSP: Completion",
        label_lc: "lsp: completion",
        command: Command::LspCompletion,
    },
    PaletteItem {
        label: "LSP: Signature Help",
        label_lc: "lsp: signature help",
        command: Command::LspSignatureHelp,
    },
    PaletteItem {
        label: "LSP: Format Document",
        label_lc: "lsp: format document",
        command: Command::LspFormat,
    },
    PaletteItem {
        label: "LSP: Format Selection",
        label_lc: "lsp: format selection",
        command: Command::LspFormatSelection,
    },
    PaletteItem {
        label: "LSP: Rename Symbol",
        label_lc: "lsp: rename symbol",
        command: Command::LspRename,
    },
    PaletteItem {
        label: "LSP: Find References",
        label_lc: "lsp: find references",
        command: Command::LspReferences,
    },
    PaletteItem {
        label: "LSP: Document Symbols",
        label_lc: "lsp: document symbols",
        command: Command::LspDocumentSymbols,
    },
    PaletteItem {
        label: "LSP: Workspace Symbols",
        label_lc: "lsp: workspace symbols",
        command: Command::LspWorkspaceSymbols,
    },
    PaletteItem {
        label: "LSP: Code Action",
        label_lc: "lsp: code action",
        command: Command::LspCodeAction,
    },
    PaletteItem {
        label: "Editor: Fold",
        label_lc: "editor: fold",
        command: Command::EditorFold,
    },
    PaletteItem {
        label: "Editor: Unfold",
        label_lc: "editor: unfold",
        command: Command::EditorUnfold,
    },
    PaletteItem {
        label: "Editor: Add Cursor Above",
        label_lc: "editor: add cursor above",
        command: Command::AddCursorAbove,
    },
    PaletteItem {
        label: "Editor: Add Cursor Below",
        label_lc: "editor: add cursor below",
        command: Command::AddCursorBelow,
    },
    PaletteItem {
        label: "Editor: Add Cursor at Next Match",
        label_lc: "editor: add cursor at next match",
        command: Command::AddCursorAtNextMatch,
    },
    PaletteItem {
        label: "Editor: Add Cursor at All Matches",
        label_lc: "editor: add cursor at all matches",
        command: Command::AddCursorAtAllMatches,
    },
    PaletteItem {
        label: "Editor: Remove Secondary Cursors",
        label_lc: "editor: remove secondary cursors",
        command: Command::RemoveSecondaryCursors,
    },
    PaletteItem {
        label: "View: Toggle Bottom Panel",
        label_lc: "view: toggle bottom panel",
        command: Command::ToggleBottomPanel,
    },
    PaletteItem {
        label: "View: Focus Bottom Panel",
        label_lc: "view: focus bottom panel",
        command: Command::FocusBottomPanel,
    },
    PaletteItem {
        label: "Panel: Next Tab",
        label_lc: "panel: next tab",
        command: Command::NextBottomPanelTab,
    },
    PaletteItem {
        label: "Panel: Prev Tab",
        label_lc: "panel: prev tab",
        command: Command::PrevBottomPanelTab,
    },
    PaletteItem {
        label: "Settings: Reload",
        label_lc: "settings: reload",
        command: Command::ReloadSettings,
    },
    PaletteItem {
        label: "Preferences: Open Settings (JSON)",
        label_lc: "preferences: open settings (json)",
        command: Command::OpenSettings,
    },
    PaletteItem {
        label: "Preferences: Open Theme Editor",
        label_lc: "preferences: open theme editor",
        command: Command::OpenThemeEditor,
    },
    PaletteItem {
        label: "Git: Worktree (Open/Create)",
        label_lc: "git: worktree (open/create)",
        command: Command::GitWorktreeAdd,
    },
    PaletteItem {
        label: "App: Hard Reload",
        label_lc: "app: hard reload",
        command: Command::HardReload,
    },
    PaletteItem {
        label: "File: Reload from Disk",
        label_lc: "file: reload from disk",
        command: Command::ReloadFromDisk,
    },
    PaletteItem {
        label: "Quit",
        label_lc: "quit",
        command: Command::Quit,
    },
];

pub fn match_indices(query: &str) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return (0..PALETTE_ITEMS.len()).collect();
    }

    let query_lc = query.to_ascii_lowercase();
    let mut matches = Vec::with_capacity(PALETTE_ITEMS.len());
    for (i, item) in PALETTE_ITEMS.iter().enumerate() {
        if item.label_lc.contains(&query_lc) {
            matches.push(i);
        }
    }
    matches
}

pub fn match_items(query: &str) -> Vec<PaletteMatch<'static>> {
    let query = query.trim();
    if query.is_empty() {
        let mut items = Vec::with_capacity(PALETTE_ITEMS.len());
        for item in PALETTE_ITEMS {
            items.push(PaletteMatch {
                label: item.label,
                command: &item.command,
            });
        }
        return items;
    }

    let query_lc = query.to_ascii_lowercase();
    let mut matches = Vec::with_capacity(PALETTE_ITEMS.len());

    for item in PALETTE_ITEMS {
        if item.label_lc.contains(&query_lc) {
            matches.push(PaletteMatch {
                label: item.label,
                command: &item.command,
            });
        }
    }

    matches
}
