use super::super::theme::UiTheme;
use super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::{ConfigService, KeybindingContext, KeybindingService};
use crate::kernel::services::KernelMessage;
use crate::kernel::services::ports::{GlobalSearchMessage, SearchMessage};
use crate::kernel::{Action as KernelAction, EditorAction};
use std::sync::mpsc;
use std::time::Instant;

impl Workbench {
    /// 定时检查是否需要刷盘（由主循环调用）
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        changed |= self.poll_editor_search();
        changed |= self.poll_global_search();
        changed |= self.poll_kernel_bus();
        changed |= self.poll_logs();
        changed |= self.poll_settings();

        changed
    }

    fn poll_editor_search(&mut self) -> bool {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        self.editor_search_tasks.resize_with(panes, || None);
        self.editor_search_rx.resize_with(panes, || None);

        let mut changed = false;

        for pane in 0..panes {
            let Some(rx) = self.editor_search_rx[pane].take() else {
                continue;
            };

            let mut done = false;
            let mut disconnected = false;

            loop {
                match rx.try_recv() {
                    Ok(msg) => {
                        done = matches!(
                            msg,
                            SearchMessage::Complete { .. }
                                | SearchMessage::Cancelled { .. }
                                | SearchMessage::Error { .. }
                        );

                        changed |= self.dispatch_kernel(KernelAction::Editor(
                            EditorAction::SearchMessage { pane, message: msg },
                        ));

                        if done {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }

            if done || disconnected {
                self.editor_search_tasks[pane] = None;
            } else {
                self.editor_search_rx[pane] = Some(rx);
            }
        }

        changed
    }

    fn poll_global_search(&mut self) -> bool {
        let Some(rx) = self.global_search_rx.take() else {
            return false;
        };

        let mut changed = false;
        let mut done = false;
        let mut disconnected = false;

        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    done = matches!(
                        msg,
                        GlobalSearchMessage::Complete { .. }
                            | GlobalSearchMessage::Cancelled { .. }
                            | GlobalSearchMessage::Error { .. }
                    );

                    changed |= self.dispatch_kernel(KernelAction::SearchMessage(msg));

                    if done {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if done || disconnected {
            self.global_search_task = None;
        } else {
            self.global_search_rx = Some(rx);
        }

        changed
    }

    fn poll_kernel_bus(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.kernel_services.try_recv() {
                Ok(msg) => match msg {
                    KernelMessage::Action(action) => {
                        changed |= self.dispatch_kernel(action);
                    }
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    fn poll_logs(&mut self) -> bool {
        let Some(rx) = self.log_rx.take() else {
            return false;
        };

        let mut changed = false;
        let mut drained = 0usize;
        let mut disconnected = false;

        loop {
            match rx.try_recv() {
                Ok(line) => {
                    changed = true;
                    drained += 1;
                    self.logs.push_back(line);
                    while self.logs.len() > super::LOG_BUFFER_CAP {
                        self.logs.pop_front();
                    }
                    if drained >= super::MAX_LOG_DRAIN_PER_TICK {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if !disconnected {
            self.log_rx = Some(rx);
        }

        changed
    }

    fn poll_settings(&mut self) -> bool {
        if cfg!(test) {
            return false;
        }

        let Some(path) = self.settings_path.as_ref() else {
            return false;
        };

        if self.last_settings_check.elapsed() < super::SETTINGS_CHECK_INTERVAL {
            return false;
        }
        self.last_settings_check = Instant::now();

        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified != self.last_settings_modified {
            self.last_settings_modified = modified;
            return self.reload_settings();
        }

        false
    }

    pub(super) fn reload_settings(&mut self) -> bool {
        if cfg!(test) {
            return false;
        }

        let Some(settings) = crate::kernel::services::adapters::settings::load_settings() else {
            return false;
        };

        let editor_config = settings.editor.clone();
        let mut keybindings = KeybindingService::new();
        for rule in settings.keybindings {
            if let Some(key) =
                crate::kernel::services::adapters::settings::parse_keybinding(&rule.key)
            {
                let context = rule
                    .context
                    .as_deref()
                    .and_then(KeybindingContext::parse)
                    .unwrap_or(KeybindingContext::Global);
                if rule.command.trim().is_empty() {
                    let _ = keybindings.unbind(context, &key);
                } else {
                    keybindings.bind(context, key, Command::from_name(&rule.command));
                }
            }
        }

        let mut theme = UiTheme::default();
        theme.apply_settings(&settings.theme);

        let _ = self.store.dispatch(KernelAction::EditorConfigUpdated {
            config: editor_config.clone(),
        });

        if let Some(service) = self.kernel_services.get_mut::<KeybindingService>() {
            *service = keybindings;
        } else {
            let _ = self.kernel_services.register(keybindings);
        }
        if let Some(service) = self.kernel_services.get_mut::<ConfigService>() {
            *service.editor_mut() = editor_config.clone();
        } else {
            let _ = self
                .kernel_services
                .register(ConfigService::with_editor_config(editor_config));
        }
        self.theme = theme;
        self.last_settings_modified = self
            .settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        true
    }
}
