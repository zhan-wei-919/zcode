use std::path::PathBuf;

use crate::kernel::panel::list_selection::ListSelectionState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationItem {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Default)]
pub struct LocationsState {
    inner: ListSelectionState<LocationItem>,
}

impl LocationsState {
    pub fn items(&self) -> &[LocationItem] {
        self.inner.items()
    }

    pub fn selected_index(&self) -> usize {
        self.inner.selected_index()
    }

    pub fn scroll_offset(&self) -> usize {
        self.inner.scroll_offset()
    }

    pub fn set_items(&mut self, mut items: Vec<LocationItem>) -> bool {
        items.sort_by(|a, b| {
            let path_a = a.path.as_os_str();
            let path_b = b.path.as_os_str();
            path_a
                .cmp(path_b)
                .then(a.line.cmp(&b.line))
                .then(a.column.cmp(&b.column))
        });
        items.dedup_by(|a, b| a.path == b.path && a.line == b.line && a.column == b.column);

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
