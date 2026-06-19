use super::*;

/// 构造一个 `len` 条目、视口高 `view_height` 的列表，选中下标与滚动偏移均为 0。
fn state_with(len: u32, view_height: usize) -> ListSelectionState<u32> {
    let mut state = ListSelectionState::default();
    state.set_view_height(view_height);
    state.replace_items((0..len).collect());
    state
}

#[test]
fn move_selection_steps_and_reports_change() {
    let mut state = state_with(5, 10);
    assert_eq!(state.selected_index(), 0);
    assert!(state.move_selection(1));
    assert_eq!(state.selected_index(), 1);
    assert!(state.move_selection(1));
    assert_eq!(state.selected_index(), 2);
    assert!(state.move_selection(-1));
    assert_eq!(state.selected_index(), 1);
}

#[test]
fn move_selection_wraps_at_both_ends() {
    let mut state = state_with(5, 10);
    // 顶端再向上 → 跳到末行
    assert!(state.move_selection(-1));
    assert_eq!(state.selected_index(), 4);
    // 末行再向下 → 回到首行
    assert!(state.move_selection(1));
    assert_eq!(state.selected_index(), 0);
}

#[test]
fn move_selection_noops_on_empty_or_zero_delta() {
    let mut empty = ListSelectionState::<u32>::default();
    assert!(!empty.move_selection(1));
    assert!(!empty.move_selection(-1));

    let mut state = state_with(5, 10);
    assert!(!state.move_selection(0));
    assert_eq!(state.selected_index(), 0);
}

#[test]
fn move_selection_single_item_is_noop() {
    let mut state = state_with(1, 10);
    assert!(!state.move_selection(1));
    assert!(!state.move_selection(-1));
    assert_eq!(state.selected_index(), 0);
}

#[test]
fn scroll_clamps_at_both_ends() {
    let mut state = state_with(10, 3); // max_scroll = 10 - 3 = 7
    assert!(state.scroll(100));
    assert_eq!(state.scroll_offset(), 7);
    assert!(!state.scroll(5)); // 已在底部 → 无变化
    assert_eq!(state.scroll_offset(), 7);
    assert!(state.scroll(-100));
    assert_eq!(state.scroll_offset(), 0);
    assert!(!state.scroll(-5)); // 已在顶部 → 无变化
}

#[test]
fn scroll_noops_on_empty() {
    let mut empty = ListSelectionState::<u32>::default();
    assert!(!empty.scroll(3));
    assert!(!empty.scroll(-3));
}

#[test]
fn click_row_selects_and_scrolls_into_view() {
    let mut state = state_with(10, 3);
    assert!(state.click_row(9));
    assert_eq!(state.selected_index(), 9);
    // keep_row_visible: 9 >= 0 + 3 → scroll = 9 + 1 - 3 = 7
    assert_eq!(state.scroll_offset(), 7);
}

#[test]
fn click_row_rejects_out_of_range_and_same_row() {
    let mut state = state_with(5, 10);
    assert!(!state.click_row(5)); // 越界（有效 0..=4）
    assert!(!state.click_row(99));
    assert!(state.click_row(3));
    assert!(!state.click_row(3)); // 点同一行
    assert_eq!(state.selected_index(), 3);
}

#[test]
fn set_view_height_enforces_minimum_and_reclamps_scroll() {
    let mut state = state_with(10, 2); // max_scroll = 8
    assert!(state.scroll(100));
    assert_eq!(state.scroll_offset(), 8);
    // 视口变高 → max_scroll 变小 → 滚动被钳回
    assert!(state.set_view_height(6)); // max_scroll = 10 - 6 = 4
    assert_eq!(state.scroll_offset(), 4);
    // 高度 0 视作 1（下限）
    assert!(state.set_view_height(0));
    // 同等高度 → 无变化
    assert!(!state.set_view_height(1));
}

#[test]
fn keep_row_visible_follows_selection_down_and_up() {
    let mut state = state_with(10, 3); // 视口 3，scroll 0，index 0
    for _ in 0..5 {
        state.move_selection(1);
    }
    assert_eq!(state.selected_index(), 5);
    // 选中行下移出视口 → scroll = 5 + 1 - 3 = 3
    assert_eq!(state.scroll_offset(), 3);
    // 选中行上移到 scroll_offset 之上 → scroll 跟随上移
    for _ in 0..3 {
        state.move_selection(-1);
    }
    assert_eq!(state.selected_index(), 2);
    assert_eq!(state.scroll_offset(), 2);
}

#[test]
fn clear_resets_and_reports_change() {
    let mut state = state_with(5, 3);
    state.click_row(4);
    assert!(state.clear());
    assert_eq!(state.selected_index(), 0);
    assert_eq!(state.scroll_offset(), 0);
    assert!(state.items().is_empty());
    // 已是空且归零 → 无变化
    assert!(!state.clear());
}

#[test]
fn replace_items_reclamps_stale_selection_and_scroll() {
    let mut state = state_with(10, 3);
    state.click_row(9);
    assert_eq!(state.selected_index(), 9);
    assert_eq!(state.scroll_offset(), 7);
    // 换成更短的列表 → 陈旧的选中下标与滚动偏移被钳回合法范围
    state.replace_items(vec![10, 11, 12]);
    assert_eq!(state.items(), &[10, 11, 12]);
    assert_eq!(state.selected_index(), 2); // min(9, len - 1)
    assert_eq!(state.scroll_offset(), 0); // max_scroll = 3 - 3 = 0
}

#[test]
fn items_mut_then_clamp_reclamps() {
    // Problems 走的增量路径：经 items_mut 直接改条目，再 clamp_after_items_changed 收尾。
    let mut state = state_with(10, 4);
    state.click_row(8); // index 8，scroll = 8 + 1 - 4 = 5
    assert_eq!(state.scroll_offset(), 5);
    state.items_mut().truncate(2);
    state.clamp_after_items_changed();
    assert_eq!(state.items().len(), 2);
    assert_eq!(state.selected_index(), 1); // min(8, len - 1)
    assert_eq!(state.scroll_offset(), 0); // max_scroll = 2 - 4 → 0
}

#[test]
fn selected_returns_current_or_none() {
    let mut state = state_with(3, 5);
    assert_eq!(state.selected(), Some(&0));
    state.click_row(2);
    assert_eq!(state.selected(), Some(&2));

    let empty = ListSelectionState::<u32>::default();
    assert_eq!(empty.selected(), None);
}
