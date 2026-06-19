//! 列表面板共享的光标选择 / 滚动机制。
//!
//! CodeActions / Symbols / Locations / Problems 四个面板都是「一列可上下选择、可滚动、
//! 可点击选行」的列表，光标移动、滚动钳制、保持选中行可见等逻辑完全一致。这里把这套机制
//! 抽成泛型 `ListSelectionState<T>`，各面板组合它，只保留自己专属的 `set_items` / `update_path`
//! 变更检测；改一次滚动语义即对四个面板同时生效，避免四份副本漂移。

/// 一列可选择 / 可滚动列表的状态：持有条目、选中下标与视口滚动偏移。
///
/// 对 `T` 不加任何约束——元素的相等比较等专属逻辑留在各面板的 `set_items` 里
/// （例如 `LspCodeAction` 没有 `PartialEq`，只能比标题）。
#[derive(Debug)]
pub struct ListSelectionState<T> {
    items: Vec<T>,
    selected_index: usize,
    view_height: usize,
    scroll_offset: usize,
}

// 手写 Default：泛型 derive 会附加 `T: Default`，而面板的元素类型都没有 Default。
impl<T> Default for ListSelectionState<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
            view_height: 0,
            scroll_offset: 0,
        }
    }
}

impl<T> ListSelectionState<T> {
    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.selected_index)
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

        // 在首/尾环绕：向上越过顶端跳到末行，向下越过末行回到首行。
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

    pub fn clear(&mut self) -> bool {
        if self.items.is_empty() && self.selected_index == 0 && self.scroll_offset == 0 {
            return false;
        }
        self.items.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        true
    }

    /// 整列替换条目后重新钳制选中/滚动（简单面板的 `set_items` 末段）。
    pub(crate) fn replace_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.clamp_after_items_changed();
    }

    /// 直接可变借用底层条目，供 Problems 的增量 `splice` 使用。
    /// 调用方改完条目后必须调用 [`Self::clamp_after_items_changed`] 收尾。
    pub(crate) fn items_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }

    /// 条目结构变更后把选中下标与滚动偏移钳回合法范围。
    pub(crate) fn clamp_after_items_changed(&mut self) {
        self.clamp_selection();
        self.clamp_scroll();
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

#[cfg(test)]
#[path = "../../../tests/unit/kernel/list_selection.rs"]
mod tests;
