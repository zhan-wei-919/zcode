use super::super::Workbench;
use super::terminal::terminal_bytes_for_key_event;
use crate::core::event::{InputEvent, Key, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::state::ThemeEditorFocus;
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, SidebarTab,
};
use crate::tui::view::EventResult;
use crate::views::theme_editor::{col_to_saturation, row_to_hue, row_to_lightness};
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn record_user_input(&mut self, event: &InputEvent) {
        let preserve_hover = matches!(
            event,
            InputEvent::Mouse(me)
                if self.store.state().ui.hover_message.is_some()
                    && self.last_hover_popup_area.is_some_and(|a| {
                        super::super::util::rect_contains(a, me.column, me.row)
                    })
                    && matches!(
                        me.kind,
                        MouseEventKind::Moved
                            | MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                            | MouseEventKind::ScrollLeft
                            | MouseEventKind::ScrollRight
                    )
        );

        self.last_input_at = Instant::now();
        if !preserve_hover {
            self.idle_hover_last_request = None;
            self.idle_hover_last_anchor = None;
        }
        self.pending_completion_deadline = None;
        self.pending_inlay_hints_deadline = None;
        self.pending_folding_range_deadline = None;
        if self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal
        {
            self.terminal_cursor_visible = true;
            self.terminal_cursor_last_blink = Instant::now();
        }

        if !preserve_hover {
            if let Some(service) = self
                .kernel_services
                .get_mut::<crate::kernel::services::adapters::LspService>()
            {
                service.cancel_hover();
            }

            if self.store.state().ui.hover_message.is_some() {
                let _ = self.dispatch_kernel(KernelAction::LspHover {
                    text: String::new(),
                });
            }
        }
    }

    pub(in super::super) fn handle_key_event(&mut self, key_event: &KeyEvent) -> EventResult {
        let _scope = perf::scope("input.key");

        if self.store.state().ui.context_menu.visible {
            match key_event.code {
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuClose);
                    return EventResult::Consumed;
                }
                KeyCode::Up => {
                    let _ =
                        self.dispatch_kernel(KernelAction::ContextMenuMoveSelection { delta: -1 });
                    return EventResult::Consumed;
                }
                KeyCode::Down => {
                    let _ =
                        self.dispatch_kernel(KernelAction::ContextMenuMoveSelection { delta: 1 });
                    return EventResult::Consumed;
                }
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuConfirm);
                    return EventResult::Consumed;
                }
                _ => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuClose);
                }
            }
        }

        if self.store.state().ui.input_dialog.visible {
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Enter, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAccept);
                    return EventResult::Consumed;
                }
                (KeyCode::Esc, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCancel);
                    return EventResult::Consumed;
                }
                (KeyCode::Backspace, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogBackspace);
                    return EventResult::Consumed;
                }
                (KeyCode::Left, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorLeft);
                    return EventResult::Consumed;
                }
                (KeyCode::Right, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorRight);
                    return EventResult::Consumed;
                }
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAppend(ch));
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.confirm_dialog.visible {
            match key_event.code {
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogAccept);
                    return EventResult::Consumed;
                }
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogCancel);
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.theme_editor.visible {
            return self.handle_theme_editor_key(key_event);
        }

        if self.store.state().ui.completion.visible {
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Esc, _) => {
                    let _ = self.dispatch_kernel(KernelAction::CompletionClose);
                    self.completion_doc_scroll = 0;
                    self.completion_doc_total_lines = 0;
                    self.completion_doc_key = None;
                    self.last_completion_doc_area = None;
                    return EventResult::Consumed;
                }
                (KeyCode::Tab, _) => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: 1 });
                    self.reset_completion_doc_scroll();
                    return EventResult::Consumed;
                }
                (KeyCode::BackTab, _) => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: -1 });
                    self.reset_completion_doc_scroll();
                    return EventResult::Consumed;
                }
                (KeyCode::PageUp, _) => {
                    let step = self.completion_doc_view_height().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-step);
                    return EventResult::Consumed;
                }
                (KeyCode::PageDown, _) => {
                    let step = self.completion_doc_view_height().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(step);
                    return EventResult::Consumed;
                }
                (KeyCode::Char('u' | 'U'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    let half = (self.completion_doc_view_height() / 2).max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-half);
                    return EventResult::Consumed;
                }
                (KeyCode::Char('d' | 'D'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    let half = (self.completion_doc_view_height() / 2).max(1) as isize;
                    let _ = self.scroll_completion_doc_by(half);
                    return EventResult::Consumed;
                }
                (KeyCode::Enter, _) => {
                    let pane = self.store.state().ui.editor_layout.active_pane;
                    let before_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    let _ = self.dispatch_kernel(KernelAction::CompletionConfirm);
                    let after_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    if before_version != after_version {
                        let refresh = Command::Paste;
                        self.maybe_schedule_semantic_tokens_debounce(&refresh);
                        self.maybe_schedule_inlay_hints_debounce(&refresh);
                        self.maybe_schedule_folding_range_debounce(&refresh);
                    }

                    // Completion session ends; reset doc scroll for the next popup.
                    if !self.store.state().ui.completion.visible {
                        self.completion_doc_scroll = 0;
                        self.completion_doc_total_lines = 0;
                        self.completion_doc_key = None;
                        self.last_completion_doc_area = None;
                    }
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }

        let context = self.keybinding_context();
        let key: Key = (*key_event).into();

        let cmd = self
            .kernel_services
            .get::<KeybindingService>()
            .and_then(|service| service.resolve(context, &key).cloned());

        let terminal_active = self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal;

        if terminal_active
            && !matches!(
                cmd.as_ref(),
                Some(Command::ToggleBottomPanel | Command::FocusBottomPanel)
            )
        {
            if let Some(bytes) = terminal_bytes_for_key_event(key_event) {
                if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                    let _ = self.dispatch_kernel(KernelAction::TerminalWrite { id, bytes });
                }
                return EventResult::Consumed;
            }
        }

        if let Some(cmd) = cmd {
            if cmd == Command::Copy
                && self.store.state().ui.focus == FocusTarget::BottomPanel
                && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Logs
            {
                self.copy_logs_to_clipboard();
                return EventResult::Consumed;
            }

            let cmd_for_schedule = cmd.clone();
            let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
            self.maybe_schedule_completion_debounce(&cmd_for_schedule);
            self.maybe_schedule_semantic_tokens_debounce(&cmd_for_schedule);
            self.maybe_schedule_inlay_hints_debounce(&cmd_for_schedule);
            self.maybe_schedule_folding_range_debounce(&cmd_for_schedule);
            if cmd_for_schedule == Command::OpenThemeEditor {
                self.sync_theme_editor_hsl();
            }
            if self.store.state().ui.should_quit {
                return EventResult::Quit;
            }
            return EventResult::Consumed;
        }

        match context {
            KeybindingContext::EditorSearchBar => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                        let _ = self.dispatch_kernel(KernelAction::Editor(
                            EditorAction::SearchBarAppend { pane, ch },
                        ));
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                }
            }
            KeybindingContext::Editor => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let cmd = Command::InsertChar(ch);
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd.clone()));
                    self.maybe_schedule_completion_debounce(&cmd);
                    self.maybe_schedule_semantic_tokens_debounce(&cmd);
                    self.maybe_schedule_inlay_hints_debounce(&cmd);
                    self.maybe_schedule_folding_range_debounce(&cmd);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::SidebarSearch => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::CommandPalette => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::PaletteAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn keybinding_context(&self) -> KeybindingContext {
        let ui = &self.store.state().ui;

        if ui.theme_editor.visible {
            return KeybindingContext::ThemeEditor;
        }

        if ui.command_palette.visible && ui.focus == FocusTarget::CommandPalette {
            return KeybindingContext::CommandPalette;
        }

        match ui.focus {
            FocusTarget::Explorer => match ui.sidebar_tab {
                SidebarTab::Explorer => KeybindingContext::SidebarExplorer,
                SidebarTab::Search => KeybindingContext::SidebarSearch,
            },
            FocusTarget::BottomPanel => KeybindingContext::BottomPanel,
            FocusTarget::CommandPalette => KeybindingContext::CommandPalette,
            FocusTarget::ThemeEditor => KeybindingContext::ThemeEditor,
            FocusTarget::Editor => {
                let pane = ui.editor_layout.active_pane;
                let visible = self
                    .store
                    .state()
                    .editor
                    .pane(pane)
                    .is_some_and(|p| p.search_bar.visible);
                if visible {
                    KeybindingContext::EditorSearchBar
                } else {
                    KeybindingContext::Editor
                }
            }
        }
    }

    fn handle_theme_editor_key(&mut self, key_event: &KeyEvent) -> EventResult {
        let focus = self.store.state().ui.theme_editor.focus;

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Esc, _) => {
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorClose);
                EventResult::Consumed
            }
            (KeyCode::Tab, _) => {
                let next = match focus {
                    ThemeEditorFocus::TokenList => ThemeEditorFocus::HueBar,
                    ThemeEditorFocus::HueBar => ThemeEditorFocus::SvPalette,
                    ThemeEditorFocus::SvPalette => ThemeEditorFocus::TokenList,
                };
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus { focus: next });
                EventResult::Consumed
            }
            (KeyCode::Up, mods) => {
                let delta = if mods.contains(KeyModifiers::SHIFT) { 10 } else { 1 };
                match focus {
                    ThemeEditorFocus::TokenList => {
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorMoveTokenSelection {
                            delta: -1,
                        });
                        self.sync_theme_editor_hsl();
                    }
                    ThemeEditorFocus::HueBar => {
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustHue {
                            delta: -(delta as i16),
                        });
                        self.apply_theme_editor_color();
                    }
                    ThemeEditorFocus::SvPalette => {
                        // Up = increase lightness
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustLightness {
                            delta: delta as i8,
                        });
                        self.apply_theme_editor_color();
                    }
                }
                EventResult::Consumed
            }
            (KeyCode::Down, mods) => {
                let delta = if mods.contains(KeyModifiers::SHIFT) { 10 } else { 1 };
                match focus {
                    ThemeEditorFocus::TokenList => {
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorMoveTokenSelection {
                            delta: 1,
                        });
                        self.sync_theme_editor_hsl();
                    }
                    ThemeEditorFocus::HueBar => {
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustHue {
                            delta: delta as i16,
                        });
                        self.apply_theme_editor_color();
                    }
                    ThemeEditorFocus::SvPalette => {
                        // Down = decrease lightness
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustLightness {
                            delta: -(delta as i8),
                        });
                        self.apply_theme_editor_color();
                    }
                }
                EventResult::Consumed
            }
            (KeyCode::Left, mods) => {
                let delta = if mods.contains(KeyModifiers::SHIFT) { 10 } else { 1 };
                match focus {
                    ThemeEditorFocus::HueBar => {
                        // Left on hue bar = switch to SvPalette
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                            focus: ThemeEditorFocus::SvPalette,
                        });
                    }
                    ThemeEditorFocus::SvPalette => {
                        // Left = decrease saturation
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustSaturation {
                            delta: -(delta as i8),
                        });
                        self.apply_theme_editor_color();
                    }
                    _ => {}
                }
                EventResult::Consumed
            }
            (KeyCode::Right, mods) => {
                let delta = if mods.contains(KeyModifiers::SHIFT) { 10 } else { 1 };
                match focus {
                    ThemeEditorFocus::HueBar => {
                        // Right on hue bar = switch to SvPalette
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                            focus: ThemeEditorFocus::SvPalette,
                        });
                    }
                    ThemeEditorFocus::SvPalette => {
                        // Right = increase saturation
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorAdjustSaturation {
                            delta: delta as i8,
                        });
                        self.apply_theme_editor_color();
                    }
                    _ => {}
                }
                EventResult::Consumed
            }
            (KeyCode::Char('l'), mods) if mods.is_empty() => {
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorCycleLanguage);
                EventResult::Consumed
            }
            (KeyCode::Char('r'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorResetToken);
                self.reset_theme_editor_token_to_default();
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    pub(in super::super) fn handle_theme_editor_mouse(
        &mut self,
        event: &MouseEvent,
    ) -> EventResult {
        let is_click_or_drag = matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
        );
        if !is_click_or_drag {
            return EventResult::Ignored;
        }

        let col = event.column;
        let row = event.row;

        // Check Hue Bar
        if let Some(area) = self.last_theme_editor_hue_bar_area {
            if col >= area.x && col < area.x + area.w && row >= area.y && row < area.y + area.h {
                let rel_row = row - area.y;
                let hue = row_to_hue(rel_row, area.h);
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                    focus: ThemeEditorFocus::HueBar,
                });
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue });
                self.apply_theme_editor_color();
                return EventResult::Consumed;
            }
        }

        // Check SV Palette
        if let Some(area) = self.last_theme_editor_sv_palette_area {
            if col >= area.x && col < area.x + area.w && row >= area.y && row < area.y + area.h {
                let rel_col = col - area.x;
                let rel_row = row - area.y;
                let saturation = col_to_saturation(rel_col, area.w);
                let lightness = row_to_lightness(rel_row, area.h);
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                    focus: ThemeEditorFocus::SvPalette,
                });
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                    saturation,
                    lightness,
                });
                self.apply_theme_editor_color();
                return EventResult::Consumed;
            }
        }

        // Check Token List
        if let Some(area) = self.last_theme_editor_token_list_area {
            if col >= area.x
                && col < area.x + area.w
                && row >= area.y
                && row < area.y + area.h
                && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            {
                let rel_row = (row - area.y) as usize;
                let count = crate::kernel::state::ThemeEditorToken::ALL.len();
                if rel_row < count {
                    let cur = self.store.state().ui.theme_editor.selected_token.index();
                    let delta = rel_row as isize - cur as isize;
                    if delta != 0 {
                        let _ = self.dispatch_kernel(
                            KernelAction::ThemeEditorMoveTokenSelection { delta },
                        );
                        self.sync_theme_editor_hsl();
                    }
                    let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                        focus: ThemeEditorFocus::TokenList,
                    });
                    return EventResult::Consumed;
                }
            }
        }

        EventResult::Consumed
    }

    fn apply_theme_editor_color(&mut self) {
        let te = &self.store.state().ui.theme_editor;
        let token = te.selected_token;
        let (r, g, b) = crate::app::theme::hsl_to_rgb(te.hue, te.saturation, te.lightness);
        let color = crate::ui::core::style::Color::Rgb(r, g, b);

        match token {
            crate::kernel::state::ThemeEditorToken::Comment => {
                self.theme.syntax_comment_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Keyword => {
                self.theme.syntax_keyword_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::String => {
                self.theme.syntax_string_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Number => {
                self.theme.syntax_number_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Type => {
                self.theme.syntax_type_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Attribute => {
                self.theme.syntax_attribute_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Function => {
                self.theme.syntax_function_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Variable => {
                self.theme.syntax_variable_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Constant => {
                self.theme.syntax_constant_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Regex => {
                self.theme.syntax_regex_fg = color;
            }
        }
        self.ui_theme = crate::app::theme::to_core_theme(&self.theme);

        // Schedule debounced save
        self.pending_theme_save_deadline =
            Some(Instant::now() + std::time::Duration::from_millis(300));
    }

    fn reset_theme_editor_token_to_default(&mut self) {
        let token = self.store.state().ui.theme_editor.selected_token;
        let defaults = crate::app::theme::UiTheme::default();
        let color = match token {
            crate::kernel::state::ThemeEditorToken::Comment => defaults.syntax_comment_fg,
            crate::kernel::state::ThemeEditorToken::Keyword => defaults.syntax_keyword_fg,
            crate::kernel::state::ThemeEditorToken::String => defaults.syntax_string_fg,
            crate::kernel::state::ThemeEditorToken::Number => defaults.syntax_number_fg,
            crate::kernel::state::ThemeEditorToken::Type => defaults.syntax_type_fg,
            crate::kernel::state::ThemeEditorToken::Attribute => defaults.syntax_attribute_fg,
            crate::kernel::state::ThemeEditorToken::Function => defaults.syntax_function_fg,
            crate::kernel::state::ThemeEditorToken::Variable => defaults.syntax_variable_fg,
            crate::kernel::state::ThemeEditorToken::Constant => defaults.syntax_constant_fg,
            crate::kernel::state::ThemeEditorToken::Regex => defaults.syntax_regex_fg,
        };

        if let crate::ui::core::style::Color::Rgb(r, g, b) = color {
            let (h, s, l) = crate::app::theme::rgb_to_hsl(r, g, b);
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: h });
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                saturation: s,
                lightness: l,
            });
        }

        self.apply_theme_editor_color();
    }

    fn sync_theme_editor_hsl(&mut self) {
        let token = self.store.state().ui.theme_editor.selected_token;
        let color = match token {
            crate::kernel::state::ThemeEditorToken::Comment => self.theme.syntax_comment_fg,
            crate::kernel::state::ThemeEditorToken::Keyword => self.theme.syntax_keyword_fg,
            crate::kernel::state::ThemeEditorToken::String => self.theme.syntax_string_fg,
            crate::kernel::state::ThemeEditorToken::Number => self.theme.syntax_number_fg,
            crate::kernel::state::ThemeEditorToken::Type => self.theme.syntax_type_fg,
            crate::kernel::state::ThemeEditorToken::Attribute => self.theme.syntax_attribute_fg,
            crate::kernel::state::ThemeEditorToken::Function => self.theme.syntax_function_fg,
            crate::kernel::state::ThemeEditorToken::Variable => self.theme.syntax_variable_fg,
            crate::kernel::state::ThemeEditorToken::Constant => self.theme.syntax_constant_fg,
            crate::kernel::state::ThemeEditorToken::Regex => self.theme.syntax_regex_fg,
        };

        if let crate::ui::core::style::Color::Rgb(r, g, b) = color {
            let (h, s, l) = crate::app::theme::rgb_to_hsl(r, g, b);
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: h });
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                saturation: s,
                lightness: l,
            });
        }
    }
}
