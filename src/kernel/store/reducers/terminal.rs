impl super::Store {
    pub(super) fn ensure_terminal_session(&mut self) -> (bool, Vec<crate::kernel::Effect>) {
        let cwd = self.state.workspace_root.clone();
        let cols = 80;
        let rows = 24;
        let Some(id) = self.state.terminal.ensure_session(cwd.clone(), cols, rows) else {
            return (false, Vec::new());
        };

        let args = if cfg!(windows) {
            Vec::new()
        } else {
            vec!["-l".to_string()]
        };

        (
            true,
            vec![crate::kernel::Effect::TerminalSpawn {
                id,
                cwd,
                shell: None,
                args,
                cols,
                rows,
            }],
        )
    }

    pub(super) fn reduce_terminal_action(
        &mut self,
        action: crate::kernel::Action,
    ) -> super::DispatchResult {
        use crate::kernel::Action;

        match action {
            Action::TerminalWrite { id, bytes } => {
                if self.state.terminal.session_mut(id).is_none() {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                super::DispatchResult {
                    effects: vec![crate::kernel::Effect::TerminalWrite { id, bytes }],
                    state_changed: false,
                }
            }
            Action::TerminalResize { id, cols, rows } => {
                let Some(session) = self.state.terminal.session_mut(id) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                let changed = session.resize(cols, rows);
                super::DispatchResult {
                    effects: if changed {
                        vec![crate::kernel::Effect::TerminalResize { id, cols, rows }]
                    } else {
                        Vec::new()
                    },
                    state_changed: changed,
                }
            }
            Action::TerminalScroll { id, delta } => {
                let Some(session) = self.state.terminal.session_mut(id) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: session.scroll(delta),
                }
            }
            Action::TerminalSpawned { id, title } => {
                let Some(session) = self.state.terminal.session_mut(id) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let title_changed = if session.title != title {
                    session.title = title;
                    true
                } else {
                    false
                };
                session.exited = false;
                session.exit_code = None;

                super::DispatchResult {
                    effects: vec![crate::kernel::Effect::TerminalResize {
                        id,
                        cols: session.cols,
                        rows: session.rows,
                    }],
                    state_changed: title_changed,
                }
            }
            Action::TerminalOutput { id, bytes } => {
                let Some(session) = self.state.terminal.session_mut(id) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: session.process_output(&bytes),
                }
            }
            Action::TerminalExited { id, code } => {
                let Some(session) = self.state.terminal.session_mut(id) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                session.exited = true;
                session.exit_code = code;
                super::DispatchResult {
                    effects: vec![crate::kernel::Effect::TerminalKill { id }],
                    state_changed: true,
                }
            }
            _ => unreachable!("non-terminal action passed to reduce_terminal_action"),
        }
    }
}
