use super::{completion_doc_area, doc, UiRect, MAX_DOC_RENDER_LINES};

#[test]
fn rendered_doc_cache_reuses_entry_for_same_text_hash_and_width() {
    let mut cache = doc::RenderCache::default();

    let (_, first_lines, first_hit) = cache.get_or_render("alpha\nbeta", 24, MAX_DOC_RENDER_LINES);
    assert!(!first_hit);
    let first_len = first_lines.len();

    let (_, second_lines, second_hit) =
        cache.get_or_render("alpha\nbeta", 24, MAX_DOC_RENDER_LINES);
    assert!(second_hit);
    assert_eq!(second_lines.len(), first_len);
}

#[test]
fn rendered_doc_cache_misses_when_width_changes() {
    let mut cache = doc::RenderCache::default();

    let _ = cache.get_or_render("alpha\nbeta", 24, MAX_DOC_RENDER_LINES);
    let (_, _, hit) = cache.get_or_render("alpha\nbeta", 32, MAX_DOC_RENDER_LINES);
    assert!(!hit);
}

#[test]
fn rendered_doc_cache_misses_when_text_changes() {
    let mut cache = doc::RenderCache::default();

    let _ = cache.get_or_render("alpha", 24, MAX_DOC_RENDER_LINES);
    let (_, _, hit) = cache.get_or_render("beta", 24, MAX_DOC_RENDER_LINES);
    assert!(!hit);
}

#[test]
fn rendered_doc_cache_clear_drops_key_and_lines() {
    let mut cache = doc::RenderCache::default();
    let _ = cache.get_or_render("alpha", 24, MAX_DOC_RENDER_LINES);
    assert!(!cache.lines().is_empty());
    assert!(cache.key().is_some());

    cache.clear();
    assert!(cache.lines().is_empty());
    assert!(cache.key().is_none());
}

#[test]
fn completion_doc_area_above_is_anchored_near_completion() {
    let screen = UiRect::new(10, 5, 80, 40);
    let popup = UiRect::new(70, 30, 10, 8);
    let cursor_y = 29;
    let side_threshold = 30;

    // Right side is too narrow, so the doc panel should be placed above/below. When placed above,
    // it should be anchored near the completion popup, not stuck to the top of the screen.
    let area = completion_doc_area(screen, popup, cursor_y, side_threshold).unwrap();
    assert_eq!(area.h, 15);
    assert_eq!(area.y, 13);
}
