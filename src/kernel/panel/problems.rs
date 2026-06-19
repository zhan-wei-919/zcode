use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use crate::kernel::panel::list_selection::ListSelectionState;

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
    inner: ListSelectionState<ProblemItem>,
    // 每个文件路径在 `inner.items()` 中占据的连续区间，用于按路径增量更新而非整列重排。
    ranges_by_path: BTreeMap<PathBuf, Range<usize>>,
}

impl ProblemsState {
    pub fn items(&self) -> &[ProblemItem] {
        self.inner.items()
    }

    pub fn selected_index(&self) -> usize {
        self.inner.selected_index()
    }

    pub fn scroll_offset(&self) -> usize {
        self.inner.scroll_offset()
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

    pub fn update_path(&mut self, path: PathBuf, mut items: Vec<ProblemItem>) -> bool {
        sort_problem_items(&mut items);

        debug_assert!(items.iter().all(|item| item.path == path));

        let existing_range = self.ranges_by_path.get(&path).cloned();
        let changed = match existing_range.as_ref() {
            Some(range) => &self.inner.items()[range.clone()] != items.as_slice(),
            None => !items.is_empty(),
        };

        if !changed {
            return false;
        }

        let new_len = items.len();
        if let Some(range) = existing_range {
            let old_len = range.end.saturating_sub(range.start);
            self.inner.items_mut().splice(range.clone(), items);

            if new_len == 0 {
                self.ranges_by_path.remove(&path);
            } else {
                self.ranges_by_path
                    .insert(path.clone(), range.start..range.start + new_len);
            }

            let delta = new_len as isize - old_len as isize;
            self.shift_ranges_after(&path, delta);
        } else if new_len > 0 {
            let insert_at = self.insert_index_for_path(&path);
            self.inner.items_mut().splice(insert_at..insert_at, items);
            self.ranges_by_path
                .insert(path.clone(), insert_at..insert_at + new_len);
            self.shift_ranges_after(&path, new_len as isize);
        }

        self.inner.clamp_after_items_changed();
        true
    }

    fn insert_index_for_path(&self, path: &Path) -> usize {
        self.ranges_by_path
            .range((Excluded(path.to_path_buf()), Unbounded))
            .next()
            .map(|(_, range)| range.start)
            .unwrap_or(self.inner.items().len())
    }

    fn shift_ranges_after(&mut self, path: &Path, delta: isize) {
        if delta == 0 {
            return;
        }

        let shifted_keys: Vec<PathBuf> = self
            .ranges_by_path
            .range((Excluded(path.to_path_buf()), Unbounded))
            .map(|(key, _)| key.clone())
            .collect();
        for key in shifted_keys {
            if let Some(range) = self.ranges_by_path.get_mut(&key) {
                range.start = offset_index(range.start, delta);
                range.end = offset_index(range.end, delta);
            }
        }
    }
}

fn sort_problem_items(items: &mut [ProblemItem]) {
    items.sort_by(problem_item_cmp);
}

fn problem_item_cmp(a: &ProblemItem, b: &ProblemItem) -> Ordering {
    a.path
        .as_os_str()
        .cmp(b.path.as_os_str())
        .then(a.range.start_line.cmp(&b.range.start_line))
        .then(a.range.start_col.cmp(&b.range.start_col))
        .then(a.range.end_line.cmp(&b.range.end_line))
        .then(a.range.end_col.cmp(&b.range.end_col))
        .then(severity_rank(a.severity).cmp(&severity_rank(b.severity)))
        .then(a.message.cmp(&b.message))
        .then(a.source.cmp(&b.source))
}

fn severity_rank(severity: ProblemSeverity) -> u8 {
    match severity {
        ProblemSeverity::Error => 0,
        ProblemSeverity::Warning => 1,
        ProblemSeverity::Information => 2,
        ProblemSeverity::Hint => 3,
    }
}

fn offset_index(index: usize, delta: isize) -> usize {
    if delta >= 0 {
        index + delta as usize
    } else {
        let amount = (-delta) as usize;
        debug_assert!(index >= amount);
        index.saturating_sub(amount)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/problems.rs"]
mod tests;
