use crate::kernel::{Action, Effect};
use rustc_hash::FxHashMap;

impl super::Store {
    pub(super) fn reduce_git_action(&mut self, action: Action) -> super::DispatchResult {
        match action {
            Action::GitInit => super::DispatchResult {
                effects: vec![Effect::GitDetectRepo {
                    workspace_root: self.state.workspace_root.clone(),
                }],
                state_changed: false,
            },
            Action::GitRepoDetected {
                repo_root,
                head,
                worktrees,
            } => {
                let repo_changed = self.state.git.repo_root.as_ref() != Some(&repo_root);
                let head_changed = self.state.git.head.as_ref() != Some(&head);
                let worktrees_changed = self.state.git.worktrees != worktrees;

                self.state.git.repo_root = Some(repo_root.clone());
                self.state.git.head = Some(head);
                self.state.git.worktrees = worktrees;

                if repo_changed {
                    self.state.git.file_status.clear();
                    self.state.git.branches.clear();
                    let _ = self
                        .state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status);
                    for pane in &mut self.state.editor.panes {
                        for tab in &mut pane.tabs {
                            tab.clear_git_gutter();
                        }
                    }
                }

                let mut effects = Vec::new();
                effects.push(Effect::GitRefreshStatus {
                    repo_root: repo_root.clone(),
                });
                effects.push(Effect::GitListWorktrees {
                    repo_root: repo_root.clone(),
                });
                effects.push(Effect::GitListBranches { repo_root });

                super::DispatchResult {
                    effects,
                    state_changed: repo_changed || head_changed || worktrees_changed,
                }
            }
            Action::GitRepoCleared => {
                let had_repo = self.state.git.repo_root.take().is_some();
                let had_head = self.state.git.head.take().is_some();
                let had_worktrees = !self.state.git.worktrees.is_empty();
                let had_branches = !self.state.git.branches.is_empty();
                let had_status = !self.state.git.file_status.is_empty();

                self.state.git.worktrees.clear();
                self.state.git.branches.clear();
                self.state.git.file_status.clear();
                let explorer_git_changed = self
                    .state
                    .explorer
                    .set_git_statuses(&self.state.git.file_status);
                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        tab.clear_git_gutter();
                    }
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: had_repo
                        || had_head
                        || had_worktrees
                        || had_branches
                        || had_status
                        || explorer_git_changed,
                }
            }
            Action::GitStatusUpdated { statuses } => {
                let mut map = FxHashMap::default();
                for (path, status) in statuses {
                    map.insert(path, status);
                }

                if self.state.git.file_status == map {
                    let explorer_git_changed = self
                        .state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status);
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: explorer_git_changed,
                    };
                }

                self.state.git.file_status = map;
                let _ = self
                    .state
                    .explorer
                    .set_git_statuses(&self.state.git.file_status);

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::GitDiffUpdated { path, marks } => {
                let mut changed = false;
                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        if tab.path.as_ref() == Some(&path) {
                            changed |= tab.set_git_gutter(Some(marks.clone()));
                        }
                    }
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::GitWorktreesUpdated { worktrees } => {
                if self.state.git.worktrees == worktrees {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.git.worktrees = worktrees;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::GitBranchesUpdated { branches } => {
                if self.state.git.branches == branches {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.git.branches = branches;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::GitWorktreeResolved { path } => super::DispatchResult {
                effects: vec![Effect::Restart { path, hard: false }],
                state_changed: false,
            },
            Action::GitCheckoutBranch { branch } => {
                let Some(repo_root) = self.state.git.repo_root.clone() else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let branch = branch.trim();
                if branch.is_empty() {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                if let Some(head) = self.state.git.head.as_ref() {
                    if !head.detached && head.branch.as_deref() == Some(branch) {
                        return super::DispatchResult {
                            effects: Vec::new(),
                            state_changed: false,
                        };
                    }
                }

                super::DispatchResult {
                    effects: vec![Effect::GitWorktreeAdd {
                        repo_root,
                        branch: branch.to_string(),
                    }],
                    state_changed: false,
                }
            }
            _ => super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            },
        }
    }
}
