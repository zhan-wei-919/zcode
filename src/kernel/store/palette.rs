use crate::kernel::{Action, FocusTarget};

impl super::Store {
    pub(super) fn reduce_palette_action(&mut self, action: Action) -> super::super::DispatchResult {
        match action {
            Action::PaletteAppend(ch) => {
                if !self.state.ui.command_palette.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                self.state.ui.command_palette.query.push(ch);
                self.state.ui.command_palette.selected = 0;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::PaletteBackspace => {
                if !self.state.ui.command_palette.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let removed = self.state.ui.command_palette.query.pop().is_some();
                if removed {
                    self.state.ui.command_palette.selected = 0;
                }
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: removed,
                }
            }
            Action::PaletteMoveSelection(delta) => {
                if !self.state.ui.command_palette.visible || delta == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let selected = &mut self.state.ui.command_palette.selected;
                if delta > 0 {
                    *selected = selected.saturating_add(delta as usize);
                } else {
                    *selected = selected.saturating_sub((-delta) as usize);
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::PaletteClose => {
                if !self.state.ui.command_palette.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                self.state.ui.command_palette.reset();
                if self.state.ui.focus == FocusTarget::CommandPalette {
                    self.state.ui.focus = FocusTarget::Editor;
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            _ => unreachable!("non-palette action passed to reduce_palette_action"),
        }
    }
}
