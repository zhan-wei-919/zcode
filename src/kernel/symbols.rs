use std::path::PathBuf;

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
    items: Vec<SymbolItem>,
    selected_index: usize,
    view_height: usize,
    scroll_offset: usize,
}

impl SymbolsState {
    pub fn items(&self) -> &[SymbolItem] {
        &self.items
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn selected(&self) -> Option<&SymbolItem> {
        self.items.get(self.selected_index)
    }

    pub fn set_items(&mut self, items: Vec<SymbolItem>) -> bool {
        if self.items == items {
            return false;
        }
        self.items = items;
        self.clamp_selection();
        self.clamp_scroll();
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.items.is_empty() && self.selected_index == 0 && self.scroll_offset == 0 {
            return false;
        }
        self.items.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        true
    }

    pub fn set_view_height(&mut self, height: usize) -> bool {
        let height = height.max(1);
        if self.view_height == height {
            return false;
        }
        self.view_height = height;
        self.clamp_scroll();
        true
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        if self.items.is_empty() || delta == 0 {
            return false;
        }

        let prev = self.selected_index;
        let len = self.items.len();

        if delta < 0 {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                self.selected_index = len - 1;
            }
        } else if self.selected_index + 1 < len {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }

        self.keep_row_visible(self.selected_index);
        self.selected_index != prev
    }

    pub fn scroll(&mut self, delta: isize) -> bool {
        if self.items.is_empty() || delta == 0 {
            return false;
        }

        let max_scroll = self.items.len().saturating_sub(self.view_height.max(1));
        let prev = self.scroll_offset;
        if delta > 0 {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_scroll);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        }
        self.scroll_offset != prev
    }

    pub fn click_row(&mut self, row: usize) -> bool {
        if row >= self.items.len() {
            return false;
        }
        if self.selected_index == row {
            return false;
        }
        self.selected_index = row;
        self.keep_row_visible(self.selected_index);
        true
    }

    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
            return;
        }
        self.selected_index = self.selected_index.min(self.items.len().saturating_sub(1));
        self.keep_row_visible(self.selected_index);
    }

    fn clamp_scroll(&mut self) {
        let max_scroll = self.items.len().saturating_sub(self.view_height.max(1));
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    fn keep_row_visible(&mut self, row: usize) {
        let view_height = self.view_height.max(1);
        if row < self.scroll_offset {
            self.scroll_offset = row;
            return;
        }
        if row >= self.scroll_offset + view_height {
            self.scroll_offset = row.saturating_add(1).saturating_sub(view_height);
        }
        self.clamp_scroll();
    }
}

