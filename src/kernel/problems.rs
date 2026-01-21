use rustc_hash::FxHashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl ProblemSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Information => "info",
            Self::Hint => "hint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemRange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemItem {
    pub path: PathBuf,
    pub range: ProblemRange,
    pub severity: ProblemSeverity,
    pub message: String,
    pub source: Option<String>,
}

#[derive(Debug, Default)]
pub struct ProblemsState {
    by_path: FxHashMap<PathBuf, Vec<ProblemItem>>,
    items: Vec<ProblemItem>,
    selected_index: usize,
    view_height: usize,
    scroll_offset: usize,
}

impl ProblemsState {
    pub fn items(&self) -> &[ProblemItem] {
        &self.items
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
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

    pub fn update_path(&mut self, path: PathBuf, items: Vec<ProblemItem>) -> bool {
        let changed = match self.by_path.get(&path) {
            Some(existing) => existing != &items,
            None => !items.is_empty(),
        };

        if items.is_empty() {
            self.by_path.remove(&path);
        } else {
            self.by_path.insert(path, items);
        }

        if changed {
            self.rebuild_items();
        }

        changed
    }

    fn rebuild_items(&mut self) {
        self.items.clear();
        for items in self.by_path.values() {
            self.items.extend(items.iter().cloned());
        }
        self.items.sort_by(|a, b| {
            let path_a = a.path.as_os_str();
            let path_b = b.path.as_os_str();
            path_a
                .cmp(path_b)
                .then(a.range.start_line.cmp(&b.range.start_line))
                .then(a.range.start_col.cmp(&b.range.start_col))
        });
        self.clamp_selection();
        self.clamp_scroll();
    }

    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
            return;
        }
        self.selected_index = self
            .selected_index
            .min(self.items.len().saturating_sub(1));
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
