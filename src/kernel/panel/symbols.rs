use std::path::PathBuf;

use crate::kernel::panel::list_selection::ListSelectionState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolItem {
    pub name: String,
    pub detail: Option<String>,
    pub kind: u32,
    pub level: usize,
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Default)]
pub struct SymbolsState {
    inner: ListSelectionState<SymbolItem>,
}

impl SymbolsState {
    pub fn items(&self) -> &[SymbolItem] {
        self.inner.items()
    }

    pub fn selected_index(&self) -> usize {
        self.inner.selected_index()
    }

    pub fn scroll_offset(&self) -> usize {
        self.inner.scroll_offset()
    }

    pub fn selected(&self) -> Option<&SymbolItem> {
        self.inner.selected()
    }

    pub fn set_items(&mut self, items: Vec<SymbolItem>) -> bool {
        if self.inner.items() == items.as_slice() {
            return false;
        }
        self.inner.replace_items(items);
        true
    }

    pub fn clear(&mut self) -> bool {
        self.inner.clear()
    }

    pub fn set_view_height(&mut self, height: usize) -> bool {
        self.inner.set_view_height(height)
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        self.inner.move_selection(delta)
    }

    pub fn scroll(&mut self, delta: isize) -> bool {
        self.inner.scroll(delta)
    }

    pub fn click_row(&mut self, row: usize) -> bool {
        self.inner.click_row(row)
    }
}
