use ropey::Rope;
use tree_sitter::Node;

pub(super) fn node_text_trimmed(rope: &Rope, node: Node<'_>) -> Option<String> {
    let text = node_text(rope, node)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub(super) fn node_text(rope: &Rope, node: Node<'_>) -> Option<String> {
    if node.end_byte() > rope.len_bytes() {
        return None;
    }
    let start_char = rope.byte_to_char(node.start_byte());
    let end_char = rope.byte_to_char(node.end_byte());
    Some(rope.slice(start_char..end_char).to_string())
}

pub(super) fn node_is_field(parent: Node<'_>, field_name: &str, node: Node<'_>) -> bool {
    parent
        .child_by_field_name(field_name)
        .is_some_and(|field| same_node(field, node))
}

pub(super) fn node_in_field_subtree(parent: Node<'_>, field_name: &str, node: Node<'_>) -> bool {
    let Some(field_node) = parent.child_by_field_name(field_name) else {
        return false;
    };

    let mut current = Some(node);
    while let Some(cursor) = current {
        if same_node(cursor, field_node) {
            return true;
        }
        if same_node(cursor, parent) {
            break;
        }
        current = cursor.parent();
    }
    false
}

pub(super) fn node_contains(ancestor: Node<'_>, descendant: Node<'_>) -> bool {
    let mut current = Some(descendant);
    while let Some(cursor) = current {
        if same_node(cursor, ancestor) {
            return true;
        }
        current = cursor.parent();
    }
    false
}

pub(super) fn same_node(left: Node<'_>, right: Node<'_>) -> bool {
    left.start_byte() == right.start_byte() && left.end_byte() == right.end_byte()
}

pub(super) fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

pub(super) fn is_regex_kind(kind: &str) -> bool {
    kind.contains("regex") || kind == "regular_expression"
}

pub(super) fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || matches!(kind, "char_literal" | "byte_literal")
}
