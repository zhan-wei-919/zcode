use super::Workbench;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, EditorAction, Effect as KernelEffect};
use crate::kernel::services::adapters::{ClipboardService, GlobalSearchService, SearchService};
use std::fs::File;
use std::io::BufWriter;
use std::sync::mpsc;

impl Workbench {
    pub(super) fn dispatch_kernel(&mut self, action: KernelAction) -> bool {
        let _scope = perf::scope("kernel.dispatch");
        let result = {
            let _scope = perf::scope("kernel.reduce");
            self.store.dispatch(action)
        };
        {
            let _scope = perf::scope("kernel.effects");
            for effect in result.effects {
                self.run_effect(effect);
            }
        }
        self.sync_editor_search_slots();
        result.state_changed
    }

    fn sync_editor_search_slots(&mut self) {
        let desired = self.store.state().ui.editor_layout.panes.max(1);

        if desired < self.editor_search_tasks.len() {
            for task in self.editor_search_tasks.iter().skip(desired).flatten() {
                task.cancel();
            }
        }

        self.editor_search_tasks.truncate(desired);
        self.editor_search_rx.truncate(desired);

        let current = self.editor_search_tasks.len();
        if desired > current {
            self.editor_search_tasks
                .extend(std::iter::repeat_with(|| None).take(desired - current));
            self.editor_search_rx
                .extend(std::iter::repeat_with(|| None).take(desired - current));
        }
    }

    fn run_effect(&mut self, effect: KernelEffect) {
        match effect {
            KernelEffect::LoadFile(path) => {
                let _scope = perf::scope("effect.load_file");
                self.runtime.load_file(path)
            }
            KernelEffect::LoadDir(path) => {
                let _scope = perf::scope("effect.load_dir");
                self.runtime.load_dir(path)
            }
            KernelEffect::CreateFile(path) => {
                let _scope = perf::scope("effect.create_file");
                self.runtime.create_file(path)
            }
            KernelEffect::CreateDir(path) => {
                let _scope = perf::scope("effect.create_dir");
                self.runtime.create_dir(path)
            }
            KernelEffect::DeletePath { path, is_dir } => {
                let _scope = perf::scope("effect.delete_path");
                self.runtime.delete_path(path, is_dir)
            }
            KernelEffect::ReloadSettings => {
                let _scope = perf::scope("effect.reload_settings");
                self.reload_settings();
            }
            KernelEffect::OpenSettings => {
                let _scope = perf::scope("effect.open_settings");
                self.open_settings();
            }
            KernelEffect::StartGlobalSearch {
                root,
                pattern,
                case_sensitive,
                use_regex,
            } => {
                let _scope = perf::scope("effect.global_search");
                if let Some(task) = self.global_search_task.take() {
                    task.cancel();
                }

                let (tx, rx) = mpsc::channel();
                self.global_search_rx = Some(rx);

                if let Some(service) = self.kernel_services.get::<GlobalSearchService>() {
                    let task = service.search_in_dir(
                        root,
                        pattern,
                        case_sensitive,
                        use_regex,
                        tx,
                    );
                    let search_id = task.id();
                    self.global_search_task = Some(task);
                    let _ = self.dispatch_kernel(KernelAction::SearchStarted { search_id });
                }
            }
            KernelEffect::StartEditorSearch {
                pane,
                rope,
                pattern,
                case_sensitive,
                use_regex,
            } => {
                let _scope = perf::scope("effect.editor_search");
                self.sync_editor_search_slots();
                if pane >= self.editor_search_tasks.len() {
                    return;
                }

                if let Some(task) = self.editor_search_tasks[pane].take() {
                    task.cancel();
                }
                self.editor_search_rx[pane] = None;

                let (tx, rx) = mpsc::channel();
                self.editor_search_rx[pane] = Some(rx);

                if let Some(service) = self.kernel_services.get::<SearchService>() {
                    let task = service.search_in_rope(
                        rope,
                        pattern,
                        case_sensitive,
                        use_regex,
                        tx,
                    );
                    let search_id = task.id();
                    self.editor_search_tasks[pane] = Some(task);
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::SearchStarted {
                        pane,
                        search_id,
                    }));
                }
            }
            KernelEffect::CancelEditorSearch { pane } => {
                let _scope = perf::scope("effect.cancel_search");
                self.sync_editor_search_slots();
                if pane >= self.editor_search_tasks.len() {
                    return;
                }
                if let Some(task) = self.editor_search_tasks[pane].take() {
                    task.cancel();
                }
                self.editor_search_rx[pane] = None;
            }
            KernelEffect::WriteFile { pane, path } => {
                let _scope = perf::scope("effect.write_file");
                let success = write_file_from_state(&self.store, pane, &path);
                if !success {
                    tracing::error!(path = %path.display(), "write_file failed");
                }
                if success
                    && self
                        .settings_path
                        .as_ref()
                        .is_some_and(|settings_path| settings_path.as_path() == path.as_path())
                {
                    self.reload_settings();
                }
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Saved {
                    pane,
                    path,
                    success,
                }));
            }
            KernelEffect::SetClipboardText(text) => {
                let _scope = perf::scope("effect.clipboard_set");
                if let Some(service) = self.kernel_services.get_mut::<ClipboardService>() {
                    if let Err(e) = service.set_text(&text) {
                        tracing::warn!(error = %e, "clipboard.set_text failed");
                    }
                }
            }
            KernelEffect::RequestClipboardText { pane } => {
                let _scope = perf::scope("effect.clipboard_get");
                if let Some(service) = self.kernel_services.get_mut::<ClipboardService>() {
                    match service.get_text() {
                        Ok(text) if !text.is_empty() => {
                            let _ = self.dispatch_kernel(KernelAction::Editor(
                                EditorAction::InsertText { pane, text },
                            ));
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!(error = %e, "clipboard.get_text failed"),
                    }
                }
            }
        }
    }
}

fn write_file_from_state(
    store: &crate::kernel::Store,
    pane: usize,
    path: &std::path::Path,
) -> bool {
    let Some(pane_state) = store.state().editor.pane(pane) else {
        return false;
    };

    let Some(tab) = pane_state
        .tabs
        .iter()
        .find(|t| t.path.as_deref() == Some(path))
    else {
        return false;
    };

    let Ok(file) = File::create(path) else {
        return false;
    };
    let mut writer = BufWriter::new(file);
    tab.buffer.write_to(&mut writer).is_ok()
}
