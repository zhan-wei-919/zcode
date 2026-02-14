use super::super::Workbench;
use super::terminal::terminal_bytes_for_key_event;
use crate::core::event::{
    InputEvent, Key, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::state::{PreviewLanguage, ThemeEditorFocus};
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, SidebarTab,
};
use crate::tui::view::EventResult;
use crate::ui::core::style::Color;
use crate::views::theme_editor::{
    col_to_saturation, picker_pos_to_ansi_index, row_to_hue, row_to_lightness,
};
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn record_user_input(&mut self, event: &InputEvent) {
        let preserve_hover = matches!(
            event,
            InputEvent::Mouse(me)
                if self.store.state().ui.hover_message.is_some()
                    && self.hover_popup.last_area.is_some_and(|a| {
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
            self.hover_popup.last_request = None;
            self.hover_popup.last_anchor = None;
        }
        self.lsp_debounce.completion = None;
        self.lsp_debounce.inlay_hints = None;
        self.lsp_debounce.folding_range = None;
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
                    self.completion_doc.scroll = 0;
                    self.completion_doc.total_lines = 0;
                    self.completion_doc.key = None;
                    self.completion_doc.last_area = None;
                    self.completion_doc.render_cache.clear();
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

                    // Schedule debounced save of completion frequency data.
                    self.pending_completion_rank_save_deadline =
                        Some(Instant::now() + std::time::Duration::from_secs(2));

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
                        self.completion_doc.scroll = 0;
                        self.completion_doc.total_lines = 0;
                        self.completion_doc.key = None;
                        self.completion_doc.last_area = None;
                        self.completion_doc.render_cache.clear();
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
            if matches!(cmd.as_ref(), Some(Command::Copy))
                && self.copy_terminal_selection_to_clipboard()
            {
                return EventResult::Consumed;
            }

            if key_event.code == KeyCode::PageUp {
                if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                    let _ = self.dispatch_kernel(KernelAction::TerminalScroll { id, delta: 20 });
                }
                return EventResult::Consumed;
            }

            if key_event.code == KeyCode::PageDown {
                if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                    let _ = self.dispatch_kernel(KernelAction::TerminalScroll { id, delta: -20 });
                }
                return EventResult::Consumed;
            }

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
                self.theme_editor_layout.ansi_cursor = None;
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
        let color_support = self.terminal_color_support;

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Esc, _) => {
                self.theme_editor_layout.ansi_cursor = None;
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorClose);
                EventResult::Consumed
            }
            (KeyCode::Tab, _) => {
                let next = match color_support {
                    crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                        match focus {
                            ThemeEditorFocus::TokenList => ThemeEditorFocus::HueBar,
                            ThemeEditorFocus::HueBar => ThemeEditorFocus::SvPalette,
                            ThemeEditorFocus::SvPalette => ThemeEditorFocus::TokenList,
                        }
                    }
                    crate::ui::core::color_support::TerminalColorSupport::Ansi256
                    | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => match focus {
                        ThemeEditorFocus::TokenList => ThemeEditorFocus::SvPalette,
                        ThemeEditorFocus::HueBar | ThemeEditorFocus::SvPalette => {
                            ThemeEditorFocus::TokenList
                        }
                    },
                };
                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus { focus: next });
                EventResult::Consumed
            }
            (KeyCode::Up, mods) => {
                match color_support {
                    crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                        let delta = if mods.contains(KeyModifiers::SHIFT) {
                            10
                        } else {
                            1
                        };
                        match focus {
                            ThemeEditorFocus::TokenList => {
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorMoveTokenSelection { delta: -1 },
                                );
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
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorAdjustLightness { delta: delta as i8 },
                                );
                                self.apply_theme_editor_color();
                            }
                        }
                    }
                    crate::ui::core::color_support::TerminalColorSupport::Ansi256
                    | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => match focus {
                        ThemeEditorFocus::TokenList => {
                            let _ =
                                self.dispatch_kernel(KernelAction::ThemeEditorMoveTokenSelection {
                                    delta: -1,
                                });
                            self.sync_theme_editor_hsl();
                        }
                        ThemeEditorFocus::HueBar | ThemeEditorFocus::SvPalette => {
                            self.adjust_theme_editor_ansi_index(0, -1, mods);
                        }
                    },
                }
                EventResult::Consumed
            }
            (KeyCode::Down, mods) => {
                match color_support {
                    crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                        let delta = if mods.contains(KeyModifiers::SHIFT) {
                            10
                        } else {
                            1
                        };
                        match focus {
                            ThemeEditorFocus::TokenList => {
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorMoveTokenSelection { delta: 1 },
                                );
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
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorAdjustLightness {
                                        delta: -(delta as i8),
                                    },
                                );
                                self.apply_theme_editor_color();
                            }
                        }
                    }
                    crate::ui::core::color_support::TerminalColorSupport::Ansi256
                    | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => match focus {
                        ThemeEditorFocus::TokenList => {
                            let _ =
                                self.dispatch_kernel(KernelAction::ThemeEditorMoveTokenSelection {
                                    delta: 1,
                                });
                            self.sync_theme_editor_hsl();
                        }
                        ThemeEditorFocus::HueBar | ThemeEditorFocus::SvPalette => {
                            self.adjust_theme_editor_ansi_index(0, 1, mods);
                        }
                    },
                }
                EventResult::Consumed
            }
            (KeyCode::Left, mods) => {
                match color_support {
                    crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                        let delta = if mods.contains(KeyModifiers::SHIFT) {
                            10
                        } else {
                            1
                        };
                        match focus {
                            ThemeEditorFocus::HueBar => {
                                // Left on hue bar = switch to SvPalette
                                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                                    focus: ThemeEditorFocus::SvPalette,
                                });
                            }
                            ThemeEditorFocus::SvPalette => {
                                // Left = decrease saturation
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorAdjustSaturation {
                                        delta: -(delta as i8),
                                    },
                                );
                                self.apply_theme_editor_color();
                            }
                            _ => {}
                        }
                    }
                    crate::ui::core::color_support::TerminalColorSupport::Ansi256
                    | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => match focus {
                        ThemeEditorFocus::TokenList => {}
                        ThemeEditorFocus::HueBar | ThemeEditorFocus::SvPalette => {
                            self.adjust_theme_editor_ansi_index(-1, 0, mods);
                        }
                    },
                }
                EventResult::Consumed
            }
            (KeyCode::Right, mods) => {
                match color_support {
                    crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                        let delta = if mods.contains(KeyModifiers::SHIFT) {
                            10
                        } else {
                            1
                        };
                        match focus {
                            ThemeEditorFocus::HueBar => {
                                // Right on hue bar = switch to SvPalette
                                let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                                    focus: ThemeEditorFocus::SvPalette,
                                });
                            }
                            ThemeEditorFocus::SvPalette => {
                                // Right = increase saturation
                                let _ = self.dispatch_kernel(
                                    KernelAction::ThemeEditorAdjustSaturation {
                                        delta: delta as i8,
                                    },
                                );
                                self.apply_theme_editor_color();
                            }
                            _ => {}
                        }
                    }
                    crate::ui::core::color_support::TerminalColorSupport::Ansi256
                    | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => match focus {
                        ThemeEditorFocus::TokenList => {}
                        ThemeEditorFocus::HueBar | ThemeEditorFocus::SvPalette => {
                            self.adjust_theme_editor_ansi_index(1, 0, mods);
                        }
                    },
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

        match self.terminal_color_support {
            crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                // Check Hue Bar
                if let Some(area) = self.theme_editor_layout.hue_bar_area {
                    if col >= area.x
                        && col < area.x + area.w
                        && row >= area.y
                        && row < area.y + area.h
                    {
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
                if let Some(area) = self.theme_editor_layout.sv_palette_area {
                    if col >= area.x
                        && col < area.x + area.w
                        && row >= area.y
                        && row < area.y + area.h
                    {
                        let rel_col = col - area.x;
                        let rel_row = row - area.y;
                        let saturation = col_to_saturation(rel_col, area.w);
                        let lightness = row_to_lightness(rel_row, area.h);
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                            focus: ThemeEditorFocus::SvPalette,
                        });
                        let _ =
                            self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                                saturation,
                                lightness,
                            });
                        self.apply_theme_editor_color();
                        return EventResult::Consumed;
                    }
                }
            }
            crate::ui::core::color_support::TerminalColorSupport::Ansi256
            | crate::ui::core::color_support::TerminalColorSupport::Ansi16 => {
                if let Some(area) = self.theme_editor_layout.sv_palette_area {
                    if col >= area.x
                        && col < area.x + area.w
                        && row >= area.y
                        && row < area.y + area.h
                    {
                        let rel_col = col - area.x;
                        let rel_row = row - area.y;
                        if let Some(index) = picker_pos_to_ansi_index(
                            rel_col,
                            rel_row,
                            area.w,
                            area.h,
                            self.terminal_color_support,
                        ) {
                            self.theme_editor_layout.ansi_cursor = Some((rel_col, rel_row));
                            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetFocus {
                                focus: ThemeEditorFocus::SvPalette,
                            });
                            let _ = self
                                .dispatch_kernel(KernelAction::ThemeEditorSetAnsiIndex { index });
                            self.apply_theme_editor_color();
                            return EventResult::Consumed;
                        }
                    }
                }
            }
        }

        // Check Language Bar
        if let Some(area) = self.theme_editor_layout.language_bar_area {
            if col >= area.x
                && col < area.x + area.w
                && row >= area.y
                && row < area.y + area.h
                && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            {
                // Compute which button was clicked based on x position
                let mut x = area.x.saturating_add(1);
                for (i, lang) in PreviewLanguage::ALL.iter().enumerate() {
                    let btn_w = lang.label().len() as u16 + 2; // "[" + label + "]"
                    if col >= x && col < x.saturating_add(btn_w) {
                        let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetLanguage {
                            language: PreviewLanguage::from_index(i),
                        });
                        return EventResult::Consumed;
                    }
                    x = x.saturating_add(btn_w).saturating_add(1); // +1 gap
                }
                return EventResult::Consumed;
            }
        }

        // Check Token List
        if let Some(area) = self.theme_editor_layout.token_list_area {
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
                        let _ = self
                            .dispatch_kernel(KernelAction::ThemeEditorMoveTokenSelection { delta });
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

    pub(in super::super) fn apply_theme_editor_color(&mut self) {
        let te = &self.store.state().ui.theme_editor;
        let token = te.selected_token;
        let (r, g, b) = match self.terminal_color_support {
            crate::ui::core::color_support::TerminalColorSupport::TrueColor => {
                crate::app::theme::hsl_to_rgb(te.hue, te.saturation, te.lightness)
            }
            crate::ui::core::color_support::TerminalColorSupport::Ansi256 => {
                let idx = te.ansi_index.max(16);
                crate::ui::core::theme_adapter::color_to_rgb(Color::Indexed(idx))
                    .unwrap_or((0, 0, 0))
            }
            crate::ui::core::color_support::TerminalColorSupport::Ansi16 => {
                let idx = te.ansi_index % 16;
                crate::ui::core::theme_adapter::color_to_rgb(Color::Indexed(idx))
                    .unwrap_or((0, 0, 0))
            }
        };
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
            crate::kernel::state::ThemeEditorToken::Namespace => {
                self.theme.syntax_namespace_fg = color;
            }
            crate::kernel::state::ThemeEditorToken::Macro => {
                self.theme.syntax_macro_fg = color;
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
            crate::kernel::state::ThemeEditorToken::EditorBg => {
                self.theme.editor_bg = color;
            }
            crate::kernel::state::ThemeEditorToken::SidebarBg => {
                self.theme.sidebar_bg = color;
            }
            crate::kernel::state::ThemeEditorToken::ActivityBg => {
                self.theme.activity_bg = color;
            }
            crate::kernel::state::ThemeEditorToken::PopupBg => {
                self.theme.popup_bg = color;
            }
            crate::kernel::state::ThemeEditorToken::StatusbarBg => {
                self.theme.statusbar_bg = color;
            }
        }
        let core_theme = crate::app::theme::to_core_theme(&self.theme);
        self.ui_theme =
            crate::ui::core::theme_adapter::adapt_theme(&core_theme, self.terminal_color_support);

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
            crate::kernel::state::ThemeEditorToken::Namespace => defaults.syntax_namespace_fg,
            crate::kernel::state::ThemeEditorToken::Macro => defaults.syntax_macro_fg,
            crate::kernel::state::ThemeEditorToken::Function => defaults.syntax_function_fg,
            crate::kernel::state::ThemeEditorToken::Variable => defaults.syntax_variable_fg,
            crate::kernel::state::ThemeEditorToken::Constant => defaults.syntax_constant_fg,
            crate::kernel::state::ThemeEditorToken::Regex => defaults.syntax_regex_fg,
            crate::kernel::state::ThemeEditorToken::EditorBg => defaults.editor_bg,
            crate::kernel::state::ThemeEditorToken::SidebarBg => defaults.sidebar_bg,
            crate::kernel::state::ThemeEditorToken::ActivityBg => defaults.activity_bg,
            crate::kernel::state::ThemeEditorToken::PopupBg => defaults.popup_bg,
            crate::kernel::state::ThemeEditorToken::StatusbarBg => defaults.statusbar_bg,
        };

        if let Some((r, g, b)) = crate::ui::core::theme_adapter::color_to_rgb(color) {
            let (h, s, l) = crate::app::theme::rgb_to_hsl(r, g, b);
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: h });
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                saturation: s,
                lightness: l,
            });

            if self.terminal_color_support
                != crate::ui::core::color_support::TerminalColorSupport::TrueColor
            {
                if let Color::Indexed(index) = crate::ui::core::theme_adapter::map_color_to_support(
                    Color::Rgb(r, g, b),
                    self.terminal_color_support,
                ) {
                    let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetAnsiIndex { index });
                }
            }
        }

        self.apply_theme_editor_color();
    }

    pub(in super::super) fn sync_theme_editor_hsl(&mut self) {
        let token = self.store.state().ui.theme_editor.selected_token;
        let color = match token {
            crate::kernel::state::ThemeEditorToken::Comment => self.theme.syntax_comment_fg,
            crate::kernel::state::ThemeEditorToken::Keyword => self.theme.syntax_keyword_fg,
            crate::kernel::state::ThemeEditorToken::String => self.theme.syntax_string_fg,
            crate::kernel::state::ThemeEditorToken::Number => self.theme.syntax_number_fg,
            crate::kernel::state::ThemeEditorToken::Type => self.theme.syntax_type_fg,
            crate::kernel::state::ThemeEditorToken::Attribute => self.theme.syntax_attribute_fg,
            crate::kernel::state::ThemeEditorToken::Namespace => self.theme.syntax_namespace_fg,
            crate::kernel::state::ThemeEditorToken::Macro => self.theme.syntax_macro_fg,
            crate::kernel::state::ThemeEditorToken::Function => self.theme.syntax_function_fg,
            crate::kernel::state::ThemeEditorToken::Variable => self.theme.syntax_variable_fg,
            crate::kernel::state::ThemeEditorToken::Constant => self.theme.syntax_constant_fg,
            crate::kernel::state::ThemeEditorToken::Regex => self.theme.syntax_regex_fg,
            crate::kernel::state::ThemeEditorToken::EditorBg => self.theme.editor_bg,
            crate::kernel::state::ThemeEditorToken::SidebarBg => self.theme.sidebar_bg,
            crate::kernel::state::ThemeEditorToken::ActivityBg => self.theme.activity_bg,
            crate::kernel::state::ThemeEditorToken::PopupBg => self.theme.popup_bg,
            crate::kernel::state::ThemeEditorToken::StatusbarBg => self.theme.statusbar_bg,
        };

        if let Some((r, g, b)) = crate::ui::core::theme_adapter::color_to_rgb(color) {
            let (h, s, l) = crate::app::theme::rgb_to_hsl(r, g, b);
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: h });
            let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
                saturation: s,
                lightness: l,
            });

            if self.terminal_color_support
                != crate::ui::core::color_support::TerminalColorSupport::TrueColor
            {
                if let Color::Indexed(index) = crate::ui::core::theme_adapter::map_color_to_support(
                    Color::Rgb(r, g, b),
                    self.terminal_color_support,
                ) {
                    let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetAnsiIndex { index });
                }
            }
        }
    }

    fn adjust_theme_editor_ansi_index(&mut self, dx: i16, dy: i16, mods: KeyModifiers) {
        let cur = self.store.state().ui.theme_editor.ansi_index;
        let shift = mods.contains(KeyModifiers::SHIFT);

        let next = match self.terminal_color_support {
            crate::ui::core::color_support::TerminalColorSupport::TrueColor => cur,
            crate::ui::core::color_support::TerminalColorSupport::Ansi16 => {
                let idx = (cur % 16) as i16;
                let mut row = idx / 8;
                let mut col = idx % 8;
                let step_x = if shift { 2 } else { 1 };
                let step_y = 1;
                col = (col + dx * step_x).clamp(0, 7);
                row = (row + dy * step_y).clamp(0, 1);
                (row * 8 + col) as u8
            }
            crate::ui::core::color_support::TerminalColorSupport::Ansi256 => {
                let idx = cur.max(16);
                if idx >= 232 {
                    let mut gray = (idx - 232) as i16;
                    let step = if shift { 6 } else { 1 };
                    if dx != 0 {
                        gray = (gray + dx * step).clamp(0, 23);
                    } else if dy != 0 {
                        // Treat vertical movement as a faster horizontal step on the 1-row ramp.
                        gray = (gray + dy * step).clamp(0, 23);
                    }
                    232 + gray as u8
                } else {
                    let idx = idx.min(231);
                    let offset = (idx - 16) as i16;
                    let r = offset / 36;
                    let g = (offset % 36) / 6;
                    let b = offset % 6;
                    let mut cube_col = r * 6 + b; // 0..35
                    let mut cube_row = g; // 0..5
                    let step_x = if shift { 6 } else { 1 };
                    let step_y = if shift { 2 } else { 1 };
                    cube_col = (cube_col + dx * step_x).clamp(0, 35);
                    cube_row = (cube_row + dy * step_y).clamp(0, 5);
                    let r2 = cube_col / 6;
                    let b2 = cube_col % 6;
                    let idx = 16u16 + (r2 as u16 * 36) + (cube_row as u16 * 6) + b2 as u16;
                    idx as u8
                }
            }
        };

        let _ = self.dispatch_kernel(KernelAction::ThemeEditorSetAnsiIndex { index: next });
        self.apply_theme_editor_color();
    }
}
