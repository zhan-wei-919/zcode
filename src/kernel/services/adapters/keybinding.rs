//! 快捷键：按键 → 命令（支持上下文）

use crate::core::event::Key;
use crate::core::event::{KeyCode, KeyModifiers};
use crate::core::Command;
use crate::core::Service;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeybindingContext {
    Global,
    Editor,
    EditorSearchBar,
    SidebarExplorer,
    SidebarSearch,
    CommandPalette,
    BottomPanel,
    ThemeEditor,
}

impl KeybindingContext {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase();
        match v.as_str() {
            "global" => Some(Self::Global),
            "editor" => Some(Self::Editor),
            "searchbar" | "editorsearchbar" | "editor.searchbar" => Some(Self::EditorSearchBar),
            "explorer" | "sidebarexplorer" | "sidebar.explorer" => Some(Self::SidebarExplorer),
            "search" | "sidebarsearch" | "sidebar.search" | "globalsearch" => {
                Some(Self::SidebarSearch)
            }
            "palette" | "commandpalette" | "command_palette" => Some(Self::CommandPalette),
            "bottompanel" | "bottom_panel" | "panel" => Some(Self::BottomPanel),
            "themeeditor" | "theme_editor" => Some(Self::ThemeEditor),
            _ => None,
        }
    }
}

pub struct KeybindingService {
    global: FxHashMap<Key, Command>,
    editor: FxHashMap<Key, Command>,
    editor_search_bar: FxHashMap<Key, Command>,
    sidebar_explorer: FxHashMap<Key, Command>,
    sidebar_search: FxHashMap<Key, Command>,
    command_palette: FxHashMap<Key, Command>,
    bottom_panel: FxHashMap<Key, Command>,
    theme_editor: FxHashMap<Key, Command>,
}

impl KeybindingService {
    pub fn new() -> Self {
        Self::with_defaults()
    }

    pub fn with_defaults() -> Self {
        Self {
            global: default_global_keybindings(),
            editor: default_editor_keybindings(),
            editor_search_bar: default_editor_search_bar_keybindings(),
            sidebar_explorer: default_sidebar_explorer_keybindings(),
            sidebar_search: default_sidebar_search_keybindings(),
            command_palette: default_command_palette_keybindings(),
            bottom_panel: default_bottom_panel_keybindings(),
            theme_editor: FxHashMap::default(),
        }
    }

    pub fn resolve(&self, context: KeybindingContext, key: &Key) -> Option<&Command> {
        match context {
            KeybindingContext::Global => self.global.get(key),
            KeybindingContext::Editor => self.editor.get(key).or_else(|| self.global.get(key)),
            KeybindingContext::EditorSearchBar => self
                .editor_search_bar
                .get(key)
                .or_else(|| self.editor.get(key))
                .or_else(|| self.global.get(key)),
            KeybindingContext::SidebarExplorer => self
                .sidebar_explorer
                .get(key)
                .or_else(|| self.global.get(key)),
            KeybindingContext::SidebarSearch => self
                .sidebar_search
                .get(key)
                .or_else(|| self.global.get(key)),
            KeybindingContext::CommandPalette => self
                .command_palette
                .get(key)
                .or_else(|| self.global.get(key)),
            KeybindingContext::BottomPanel => {
                self.bottom_panel.get(key).or_else(|| self.global.get(key))
            }
            KeybindingContext::ThemeEditor => {
                self.theme_editor.get(key).or_else(|| self.global.get(key))
            }
        }
    }

    pub fn bindings(&self, context: KeybindingContext) -> &FxHashMap<Key, Command> {
        match context {
            KeybindingContext::Global => &self.global,
            KeybindingContext::Editor => &self.editor,
            KeybindingContext::EditorSearchBar => &self.editor_search_bar,
            KeybindingContext::SidebarExplorer => &self.sidebar_explorer,
            KeybindingContext::SidebarSearch => &self.sidebar_search,
            KeybindingContext::CommandPalette => &self.command_palette,
            KeybindingContext::BottomPanel => &self.bottom_panel,
            KeybindingContext::ThemeEditor => &self.theme_editor,
        }
    }

    pub fn bind(&mut self, context: KeybindingContext, key: Key, command: Command) {
        self.map_mut(context).insert(key, command);
    }

    pub fn unbind(&mut self, context: KeybindingContext, key: &Key) -> Option<Command> {
        self.map_mut(context).remove(key)
    }

    fn map_mut(&mut self, context: KeybindingContext) -> &mut FxHashMap<Key, Command> {
        match context {
            KeybindingContext::Global => &mut self.global,
            KeybindingContext::Editor => &mut self.editor,
            KeybindingContext::EditorSearchBar => &mut self.editor_search_bar,
            KeybindingContext::SidebarExplorer => &mut self.sidebar_explorer,
            KeybindingContext::SidebarSearch => &mut self.sidebar_search,
            KeybindingContext::CommandPalette => &mut self.command_palette,
            KeybindingContext::BottomPanel => &mut self.bottom_panel,
            KeybindingContext::ThemeEditor => &mut self.theme_editor,
        }
    }
}

impl Default for KeybindingService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service for KeybindingService {
    fn name(&self) -> &'static str {
        "KeybindingService"
    }
}

fn default_global_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(32);

    bindings.insert(Key::simple(KeyCode::Esc), Command::Escape);

    bindings.insert(Key::ctrl(KeyCode::Char('q')), Command::Quit);
    bindings.insert(Key::ctrl(KeyCode::Char('s')), Command::Save);
    bindings.insert(Key::ctrl(KeyCode::Char('w')), Command::CloseTab);
    bindings.insert(Key::ctrl(KeyCode::Tab), Command::NextTab);
    bindings.insert(Key::ctrl_shift(KeyCode::Tab), Command::PrevTab);

    bindings.insert(Key::ctrl(KeyCode::Char('z')), Command::Undo);
    bindings.insert(Key::ctrl(KeyCode::Char('y')), Command::Redo);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('z')), Command::Redo);
    bindings.insert(Key::ctrl(KeyCode::Char('c')), Command::Copy);
    bindings.insert(Key::ctrl(KeyCode::Char('x')), Command::Cut);
    bindings.insert(Key::ctrl(KeyCode::Char('v')), Command::Paste);
    bindings.insert(Key::ctrl(KeyCode::Char('a')), Command::SelectAll);

    bindings.insert(Key::ctrl(KeyCode::Char('f')), Command::Find);
    bindings.insert(Key::ctrl(KeyCode::Char('h')), Command::Replace);
    bindings.insert(Key::simple(KeyCode::F(3)), Command::FindNext);
    bindings.insert(Key::shift(KeyCode::F(3)), Command::FindPrev);
    bindings.insert(Key::ctrl(KeyCode::Char('g')), Command::FindNext);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('g')), Command::FindPrev);

    bindings.insert(Key::simple(KeyCode::F(1)), Command::CommandPalette);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('p')), Command::CommandPalette);
    bindings.insert(Key::ctrl(KeyCode::Char('b')), Command::ToggleSidebar);
    bindings.insert(Key::ctrl(KeyCode::Char('p')), Command::ToggleSidebarTab);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('e')), Command::FocusExplorer);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('f')), Command::FocusSearch);
    bindings.insert(Key::ctrl(KeyCode::Char('j')), Command::ToggleBottomPanel);
    bindings.insert(
        Key::ctrl_shift(KeyCode::Char('j')),
        Command::FocusBottomPanel,
    );
    bindings.insert(Key::ctrl(KeyCode::Char('\\')), Command::SplitEditorVertical);
    bindings.insert(
        Key::ctrl_shift(KeyCode::Char('\\')),
        Command::CloseEditorSplit,
    );
    bindings.insert(Key::ctrl(KeyCode::Char(',')), Command::OpenSettings);

    bindings
}

fn default_editor_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(48);

    bindings.insert(Key::simple(KeyCode::Left), Command::CursorLeft);
    bindings.insert(Key::simple(KeyCode::Right), Command::CursorRight);
    bindings.insert(Key::simple(KeyCode::Up), Command::CursorUp);
    bindings.insert(Key::simple(KeyCode::Down), Command::CursorDown);
    bindings.insert(Key::simple(KeyCode::Home), Command::CursorLineStart);
    bindings.insert(Key::simple(KeyCode::End), Command::CursorLineEnd);
    bindings.insert(Key::ctrl(KeyCode::Home), Command::CursorFileStart);
    bindings.insert(Key::ctrl(KeyCode::End), Command::CursorFileEnd);
    bindings.insert(Key::ctrl(KeyCode::Left), Command::CursorWordLeft);
    bindings.insert(Key::ctrl(KeyCode::Right), Command::CursorWordRight);

    bindings.insert(Key::simple(KeyCode::Enter), Command::InsertNewline);
    bindings.insert(Key::simple(KeyCode::Tab), Command::InsertTab);
    bindings.insert(
        Key::simple(KeyCode::BackTab),
        Command::SnippetPrevPlaceholder,
    );
    bindings.insert(Key::simple(KeyCode::Backspace), Command::DeleteBackward);
    bindings.insert(Key::simple(KeyCode::Delete), Command::DeleteForward);
    bindings.insert(Key::ctrl(KeyCode::Char('d')), Command::AddCursorAtNextMatch);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('k')), Command::DeleteLine);
    bindings.insert(Key::ctrl(KeyCode::Char('k')), Command::DeleteToLineEnd);
    bindings.insert(
        Key::new(KeyCode::Up, KeyModifiers::CONTROL | KeyModifiers::ALT),
        Command::AddCursorAbove,
    );
    bindings.insert(
        Key::new(KeyCode::Down, KeyModifiers::CONTROL | KeyModifiers::ALT),
        Command::AddCursorBelow,
    );
    bindings.insert(
        Key::ctrl_shift(KeyCode::Char('l')),
        Command::AddCursorAtAllMatches,
    );
    bindings.insert(Key::simple(KeyCode::F(2)), Command::LspHover);
    bindings.insert(Key::simple(KeyCode::F(12)), Command::LspDefinition);
    bindings.insert(Key::shift(KeyCode::F(12)), Command::LspReferences);
    bindings.insert(Key::alt(KeyCode::Enter), Command::LspCodeAction);
    bindings.insert(Key::ctrl(KeyCode::Char('.')), Command::LspCompletion);
    bindings.insert(Key::ctrl(KeyCode::Char(' ')), Command::LspCompletion);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('r')), Command::LspRename);
    bindings.insert(
        Key::ctrl_shift(KeyCode::Char('o')),
        Command::LspDocumentSymbols,
    );
    bindings.insert(Key::ctrl(KeyCode::Char('t')), Command::LspWorkspaceSymbols);
    bindings.insert(Key::ctrl_shift(KeyCode::Char('[')), Command::EditorFold);
    bindings.insert(Key::ctrl_shift(KeyCode::Char(']')), Command::EditorUnfold);

    bindings.insert(Key::shift(KeyCode::Left), Command::ExtendSelectionLeft);
    bindings.insert(Key::shift(KeyCode::Right), Command::ExtendSelectionRight);
    bindings.insert(Key::shift(KeyCode::Up), Command::ExtendSelectionUp);
    bindings.insert(Key::shift(KeyCode::Down), Command::ExtendSelectionDown);
    bindings.insert(Key::shift(KeyCode::Home), Command::ExtendSelectionLineStart);
    bindings.insert(Key::shift(KeyCode::End), Command::ExtendSelectionLineEnd);
    bindings.insert(
        Key::ctrl_shift(KeyCode::Left),
        Command::ExtendSelectionWordLeft,
    );
    bindings.insert(
        Key::ctrl_shift(KeyCode::Right),
        Command::ExtendSelectionWordRight,
    );

    bindings.insert(Key::simple(KeyCode::PageUp), Command::PageUp);
    bindings.insert(Key::simple(KeyCode::PageDown), Command::PageDown);

    bindings
}

fn default_editor_search_bar_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(24);

    bindings.insert(Key::simple(KeyCode::Enter), Command::FindNext);
    bindings.insert(Key::shift(KeyCode::Enter), Command::FindPrev);
    bindings.insert(
        Key::ctrl(KeyCode::Enter),
        Command::EditorSearchBarReplaceCurrent,
    );
    bindings.insert(
        Key::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        Command::EditorSearchBarReplaceAll,
    );
    bindings.insert(
        Key::simple(KeyCode::Tab),
        Command::EditorSearchBarSwitchField,
    );
    bindings.insert(
        Key::alt(KeyCode::Char('c')),
        Command::EditorSearchBarToggleCaseSensitive,
    );
    bindings.insert(
        Key::alt(KeyCode::Char('x')),
        Command::EditorSearchBarToggleRegex,
    );
    bindings.insert(
        Key::alt(KeyCode::Char('r')),
        Command::EditorSearchBarToggleReplaceMode,
    );
    bindings.insert(
        Key::simple(KeyCode::Left),
        Command::EditorSearchBarCursorLeft,
    );
    bindings.insert(
        Key::simple(KeyCode::Right),
        Command::EditorSearchBarCursorRight,
    );
    bindings.insert(
        Key::simple(KeyCode::Home),
        Command::EditorSearchBarCursorHome,
    );
    bindings.insert(Key::simple(KeyCode::End), Command::EditorSearchBarCursorEnd);
    bindings.insert(
        Key::simple(KeyCode::Backspace),
        Command::EditorSearchBarBackspace,
    );
    bindings.insert(
        Key::simple(KeyCode::Delete),
        Command::EditorSearchBarDeleteForward,
    );

    bindings
}

fn default_sidebar_explorer_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(16);

    bindings.insert(Key::simple(KeyCode::Up), Command::ExplorerUp);
    bindings.insert(Key::simple(KeyCode::Down), Command::ExplorerDown);
    bindings.insert(Key::simple(KeyCode::Enter), Command::ExplorerActivate);
    bindings.insert(Key::simple(KeyCode::Right), Command::ExplorerActivate);
    bindings.insert(Key::simple(KeyCode::Left), Command::ExplorerCollapse);
    bindings.insert(Key::simple(KeyCode::PageUp), Command::ExplorerScrollUp);
    bindings.insert(Key::simple(KeyCode::PageDown), Command::ExplorerScrollDown);
    bindings.insert(Key::simple(KeyCode::Char('a')), Command::ExplorerNewFile);
    bindings.insert(Key::shift(KeyCode::Char('a')), Command::ExplorerNewFolder);
    bindings.insert(Key::simple(KeyCode::Char('d')), Command::ExplorerDelete);

    bindings
}

fn default_sidebar_search_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(20);

    bindings.insert(Key::simple(KeyCode::Enter), Command::GlobalSearchStart);
    bindings.insert(Key::simple(KeyCode::Left), Command::GlobalSearchCursorLeft);
    bindings.insert(
        Key::simple(KeyCode::Right),
        Command::GlobalSearchCursorRight,
    );
    bindings.insert(
        Key::simple(KeyCode::Backspace),
        Command::GlobalSearchBackspace,
    );
    bindings.insert(
        Key::alt(KeyCode::Char('c')),
        Command::GlobalSearchToggleCaseSensitive,
    );
    bindings.insert(
        Key::alt(KeyCode::Char('x')),
        Command::GlobalSearchToggleRegex,
    );

    bindings.insert(Key::simple(KeyCode::Up), Command::SearchResultsMoveUp);
    bindings.insert(Key::simple(KeyCode::Down), Command::SearchResultsMoveDown);
    bindings.insert(
        Key::ctrl(KeyCode::Enter),
        Command::SearchResultsOpenSelected,
    );
    bindings.insert(
        Key::simple(KeyCode::Char(' ')),
        Command::SearchResultsToggleExpand,
    );

    bindings
}

fn default_command_palette_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(16);

    bindings.insert(Key::simple(KeyCode::Backspace), Command::PaletteBackspace);
    bindings.insert(Key::simple(KeyCode::Up), Command::PaletteMoveUp);
    bindings.insert(Key::simple(KeyCode::Down), Command::PaletteMoveDown);
    bindings.insert(Key::simple(KeyCode::Enter), Command::PaletteConfirm);

    bindings
}

fn default_bottom_panel_keybindings() -> FxHashMap<Key, Command> {
    let mut bindings = FxHashMap::default();
    bindings.reserve(20);

    bindings.insert(Key::simple(KeyCode::Tab), Command::NextBottomPanelTab);
    bindings.insert(Key::simple(KeyCode::BackTab), Command::PrevBottomPanelTab);
    bindings.insert(Key::simple(KeyCode::Up), Command::SearchResultsMoveUp);
    bindings.insert(Key::simple(KeyCode::Down), Command::SearchResultsMoveDown);
    bindings.insert(
        Key::simple(KeyCode::Enter),
        Command::SearchResultsOpenSelected,
    );
    bindings.insert(
        Key::simple(KeyCode::Char(' ')),
        Command::SearchResultsToggleExpand,
    );
    bindings.insert(Key::simple(KeyCode::PageUp), Command::SearchResultsScrollUp);
    bindings.insert(
        Key::simple(KeyCode::PageDown),
        Command::SearchResultsScrollDown,
    );

    bindings
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/adapters/keybinding.rs"]
mod tests;
