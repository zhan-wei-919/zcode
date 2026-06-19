use crate::kernel::panel::list_selection::ListSelectionState;
use crate::kernel::services::ports::LspCodeAction;

#[derive(Debug, Default)]
pub struct CodeActionsState {
    inner: ListSelectionState<LspCodeAction>,
}

impl CodeActionsState {
    pub fn items(&self) -> &[LspCodeAction] {
        self.inner.items()
    }

    pub fn selected_index(&self) -> usize {
        self.inner.selected_index()
    }

    pub fn scroll_offset(&self) -> usize {
        self.inner.scroll_offset()
    }

    pub fn selected(&self) -> Option<&LspCodeAction> {
        self.inner.selected()
    }

    pub fn set_items(&mut self, items: Vec<LspCodeAction>) -> bool {
        // LspCodeAction 没有 PartialEq，按标题判定是否真正变化。
        let changed = self.inner.items().len() != items.len()
            || self
                .inner
                .items()
                .iter()
                .zip(items.iter())
                .any(|(a, b)| a.title != b.title);
        if !changed {
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
