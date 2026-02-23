pub(super) fn is_json_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false" | "null")
}

pub(super) fn is_yaml_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false" | "null")
}

pub(super) fn is_toml_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false")
}
