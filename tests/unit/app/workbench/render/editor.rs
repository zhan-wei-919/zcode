use super::{doc, MAX_DOC_RENDER_LINES};

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
