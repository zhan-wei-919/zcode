use crate::core::Command;

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
    let mut matches = Vec::new();
    matches.reserve(PALETTE_ITEMS.len());
    for (i, item) in PALETTE_ITEMS.iter().enumerate() {
        if item.label_lc.contains(&query_lc) {
            matches.push(i);
        }
    }
    matches
}
