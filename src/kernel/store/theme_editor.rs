use crate::kernel::state::{ThemeEditorFocus, ThemeEditorToken};
use crate::kernel::{Action, FocusTarget};

impl super::Store {
    pub(super) fn reduce_theme_editor_action(
        &mut self,
        action: Action,
    ) -> super::super::DispatchResult {
        match action {
            Action::ThemeEditorOpen => {
                self.state.ui.theme_editor.visible = true;
                self.state.ui.theme_editor.focus = ThemeEditorFocus::TokenList;
                self.state.ui.focus = FocusTarget::ThemeEditor;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorClose => {
                self.state.ui.theme_editor.visible = false;
                self.state.ui.focus = FocusTarget::Editor;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorMoveTokenSelection { delta } => {
                if !self.state.ui.theme_editor.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                let count = ThemeEditorToken::ALL.len() as isize;
                let cur = self.state.ui.theme_editor.selected_token.index() as isize;
                let next = ((cur + delta).rem_euclid(count)) as usize;
                self.state.ui.theme_editor.selected_token = ThemeEditorToken::from_index(next);
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorSetFocus { focus } => {
                self.state.ui.theme_editor.focus = focus;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorAdjustHue { delta } => {
                let cur = self.state.ui.theme_editor.hue as i32;
                let next = ((cur + delta as i32).rem_euclid(360)) as u16;
                self.state.ui.theme_editor.hue = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorSetHue { hue } => {
                self.state.ui.theme_editor.hue = hue % 360;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorAdjustSaturation { delta } => {
                let cur = self.state.ui.theme_editor.saturation as i16;
                let next = (cur + delta as i16).clamp(0, 100) as u8;
                self.state.ui.theme_editor.saturation = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorAdjustLightness { delta } => {
                let cur = self.state.ui.theme_editor.lightness as i16;
                let next = (cur + delta as i16).clamp(0, 100) as u8;
                self.state.ui.theme_editor.lightness = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorSetSaturationLightness {
                saturation,
                lightness,
            } => {
                self.state.ui.theme_editor.saturation = saturation.min(100);
                self.state.ui.theme_editor.lightness = lightness.min(100);
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorSetAnsiIndex { index } => {
                self.state.ui.theme_editor.ansi_index = index;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorCycleLanguage => {
                self.state.ui.theme_editor.preview_language =
                    self.state.ui.theme_editor.preview_language.next();
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorSetLanguage { language } => {
                self.state.ui.theme_editor.preview_language = language;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ThemeEditorResetToken => super::DispatchResult {
                effects: Vec::new(),
                state_changed: true,
            },
            _ => unreachable!("non-theme-editor action passed to reduce_theme_editor_action"),
        }
    }
}
