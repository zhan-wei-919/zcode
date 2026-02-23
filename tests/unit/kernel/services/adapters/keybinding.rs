use super::*;

#[test]
fn editor_has_cursor_bindings() {
    let service = KeybindingService::new();
    assert_eq!(
        service.resolve(KeybindingContext::Editor, &Key::simple(KeyCode::Left)),
        Some(&Command::CursorLeft)
    );
}

#[test]
fn global_commands_fall_through_in_editor() {
    let service = KeybindingService::new();
    assert_eq!(
        service.resolve(KeybindingContext::Editor, &Key::cmd(KeyCode::Char('b'))),
        Some(&Command::ToggleSidebar)
    );
}

#[test]
fn save_uses_cmd_keybinding() {
    let service = KeybindingService::new();
    assert_eq!(
        service.resolve(KeybindingContext::Global, &Key::cmd(KeyCode::Char('s'))),
        Some(&Command::Save)
    );
}

#[test]
fn cursor_word_left_still_uses_ctrl() {
    let service = KeybindingService::new();
    assert_eq!(
        service.resolve(KeybindingContext::Editor, &Key::ctrl(KeyCode::Left)),
        Some(&Command::CursorWordLeft)
    );
}

#[test]
fn esc_resolves_to_escape_in_all_contexts() {
    let service = KeybindingService::new();
    let esc = Key::simple(KeyCode::Esc);
    assert_eq!(
        service.resolve(KeybindingContext::Editor, &esc),
        Some(&Command::Escape)
    );
    assert_eq!(
        service.resolve(KeybindingContext::CommandPalette, &esc),
        Some(&Command::Escape)
    );
    assert_eq!(
        service.resolve(KeybindingContext::EditorSearchBar, &esc),
        Some(&Command::Escape)
    );
    assert_eq!(
        service.resolve(KeybindingContext::SidebarSearch, &esc),
        Some(&Command::Escape)
    );
    assert_eq!(
        service.resolve(KeybindingContext::BottomPanel, &esc),
        Some(&Command::Escape)
    );
}

#[test]
fn searchbar_overrides_backspace() {
    let service = KeybindingService::new();
    assert_eq!(
        service.resolve(
            KeybindingContext::EditorSearchBar,
            &Key::simple(KeyCode::Backspace)
        ),
        Some(&Command::EditorSearchBarBackspace)
    );
}
