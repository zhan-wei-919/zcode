use crate::models::should_ignore;
use notify::event::{CreateKind, ModifyKind, RenameMode};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

const WATCHER_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWatchEvent {
    EditorModified(PathBuf),
    EditorRemoved(PathBuf),
    WorkspaceCreated { path: PathBuf, is_dir: bool },
    WorkspaceDeleted { path: PathBuf },
    WorkspaceRenamed { from: PathBuf, to: PathBuf },
    WorkspaceDirChanged { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FsDelta {
    Created { path: PathBuf, is_dir: bool },
    Deleted { path: PathBuf },
    Renamed { from: PathBuf, to: PathBuf },
    Modified { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct DrainBuckets {
    workspace_renamed: FxHashSet<(PathBuf, PathBuf)>,
    workspace_deleted: FxHashSet<PathBuf>,
    workspace_created: FxHashMap<PathBuf, bool>,
    workspace_dirs_changed: FxHashSet<PathBuf>,
    editor_removed: FxHashSet<PathBuf>,
    editor_modified: FxHashSet<PathBuf>,
}

pub struct FileWatcherService {
    watcher: RecommendedWatcher,
    raw_event_rx: mpsc::Receiver<notify::Event>,
    workspace_root: PathBuf,
    open_files: FxHashSet<PathBuf>,
    open_file_keys: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    open_file_fingerprints: FxHashMap<PathBuf, FileFingerprint>,
}

impl FileWatcherService {
    pub fn new(workspace_root: &Path) -> Result<Self, notify::Error> {
        let workspace_root = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                let _ = tx.send(event);
            },
            Config::default().with_poll_interval(WATCHER_POLL_INTERVAL),
        )?;
        watcher.watch(&workspace_root, RecursiveMode::Recursive)?;
        Ok(Self {
            watcher,
            raw_event_rx: rx,
            workspace_root,
            open_files: FxHashSet::default(),
            open_file_keys: FxHashMap::default(),
            open_file_fingerprints: FxHashMap::default(),
        })
    }

    pub fn sync_open_files<'a, I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = &'a Path>,
    {
        let mut open_files = FxHashSet::default();
        let mut open_file_keys: FxHashMap<PathBuf, FxHashSet<PathBuf>> = FxHashMap::default();
        let mut open_file_fingerprints: FxHashMap<PathBuf, FileFingerprint> = FxHashMap::default();

        for path in paths {
            let path = path.to_path_buf();
            if !open_files.insert(path.clone()) {
                continue;
            }

            for key in path_identity_keys(path.as_path(), self.workspace_root.as_path()) {
                open_file_keys.entry(key).or_default().insert(path.clone());
            }

            if let Some(existing) = self.open_file_fingerprints.get(&path).cloned() {
                open_file_fingerprints.insert(path.clone(), existing);
            } else if let Some(fingerprint) = file_fingerprint(path.as_path()) {
                open_file_fingerprints.insert(path, fingerprint);
            }
        }

        self.open_files = open_files;
        self.open_file_keys = open_file_keys;
        self.open_file_fingerprints = open_file_fingerprints;
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn open_files(&self) -> &FxHashSet<PathBuf> {
        &self.open_files
    }

    pub fn acknowledge_write(&mut self, path: &Path) {
        for tab_path in self.match_open_paths(path) {
            if let Some(fingerprint) = file_fingerprint(tab_path.as_path()) {
                self.open_file_fingerprints.insert(tab_path, fingerprint);
            } else {
                self.open_file_fingerprints.remove(&tab_path);
            }
        }
    }

    pub fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        let _watcher_guard = &self.watcher;

        let mut buckets = DrainBuckets::default();

        while let Ok(event) = self.raw_event_rx.try_recv() {
            for delta in normalize_notify_event(event) {
                self.route_delta(delta, &mut buckets);
            }
        }

        let mut events = Vec::new();

        let DrainBuckets {
            workspace_renamed,
            workspace_deleted,
            workspace_created,
            workspace_dirs_changed,
            editor_removed,
            editor_modified,
        } = buckets;

        let mut renamed = workspace_renamed.into_iter().collect::<Vec<_>>();
        renamed.sort_unstable_by(|(a_from, a_to), (b_from, b_to)| {
            a_from.cmp(b_from).then_with(|| a_to.cmp(b_to))
        });
        for (from, to) in renamed {
            events.push(FileWatchEvent::WorkspaceRenamed { from, to });
        }

        let mut deleted = workspace_deleted.into_iter().collect::<Vec<_>>();
        deleted.sort_unstable();
        for path in deleted {
            events.push(FileWatchEvent::WorkspaceDeleted { path });
        }

        let mut editor_removed_paths = editor_removed.into_iter().collect::<Vec<_>>();
        editor_removed_paths.sort_unstable();
        for path in editor_removed_paths {
            events.push(FileWatchEvent::EditorRemoved(path));
        }

        let mut created = workspace_created.into_iter().collect::<Vec<_>>();
        created.sort_unstable_by(|(a_path, _), (b_path, _)| a_path.cmp(b_path));
        for (path, is_dir) in created {
            events.push(FileWatchEvent::WorkspaceCreated { path, is_dir });
        }

        let mut changed_dirs = workspace_dirs_changed.into_iter().collect::<Vec<_>>();
        changed_dirs.sort_unstable();
        for path in changed_dirs {
            events.push(FileWatchEvent::WorkspaceDirChanged { path });
        }

        let mut editor_modified_paths = editor_modified.into_iter().collect::<Vec<_>>();
        editor_modified_paths.sort_unstable();
        for path in editor_modified_paths {
            events.push(FileWatchEvent::EditorModified(path));
        }

        events
    }

    fn route_delta(&mut self, delta: FsDelta, buckets: &mut DrainBuckets) {
        match delta {
            FsDelta::Modified { path } => {
                let matched_open_paths = self.match_open_paths(path.as_path());
                let is_open_path = !matched_open_paths.is_empty();

                for tab_path in matched_open_paths {
                    if self.refresh_open_file_fingerprint(tab_path.as_path()) {
                        buckets.editor_modified.insert(tab_path);
                    }
                }

                if !is_open_path {
                    self.maybe_mark_workspace_dir_changed(
                        path.as_path(),
                        &mut buckets.workspace_dirs_changed,
                    );
                }
            }
            FsDelta::Deleted { path } => {
                if let Some(path) = self.to_workspace_path(path.as_path()) {
                    buckets.workspace_deleted.insert(path);
                }
                for tab_path in self.match_open_paths(path.as_path()) {
                    self.open_file_fingerprints.remove(&tab_path);
                    buckets.editor_removed.insert(tab_path);
                }
            }
            FsDelta::Created { path, is_dir } => {
                if path.exists() {
                    if let Some(path) = self.to_workspace_path(path.as_path()) {
                        merge_created(&mut buckets.workspace_created, path, is_dir);
                    }
                }
            }
            FsDelta::Renamed { from, to } => {
                for tab_path in self.match_open_paths(from.as_path()) {
                    self.open_file_fingerprints.remove(&tab_path);
                    buckets.editor_removed.insert(tab_path);
                }
                for tab_path in self.match_open_paths(to.as_path()) {
                    let _ = self.refresh_open_file_fingerprint(tab_path.as_path());
                    buckets.editor_modified.insert(tab_path);
                }

                let from_in_workspace = self.to_workspace_path(from.as_path());
                let to_in_workspace = self.to_workspace_path(to.as_path());
                let to_exists = to.exists();

                match (from_in_workspace, to_in_workspace) {
                    (Some(from), Some(to)) => {
                        buckets.workspace_renamed.insert((from, to));
                    }
                    (Some(from), None) => {
                        buckets.workspace_deleted.insert(from);
                    }
                    (None, Some(to)) => {
                        if to_exists {
                            let is_dir = infer_is_dir(to.as_path(), None);
                            merge_created(&mut buckets.workspace_created, to, is_dir);
                        }
                    }
                    (None, None) => {}
                }
            }
        }
    }

    fn maybe_mark_workspace_dir_changed(
        &self,
        path: &Path,
        workspace_dirs_changed: &mut FxHashSet<PathBuf>,
    ) {
        let raw = raw_absolute_path(path, self.workspace_root.as_path());
        let candidate_dir = match std::fs::metadata(&raw) {
            Ok(meta) if meta.is_dir() => Some(raw.as_path()),
            _ => raw.parent(),
        };

        let Some(dir) = candidate_dir.and_then(|p| self.to_workspace_path(p)) else {
            return;
        };

        workspace_dirs_changed.insert(dir);
    }

    fn refresh_open_file_fingerprint(&mut self, path: &Path) -> bool {
        let new_fingerprint = file_fingerprint(path);
        match new_fingerprint {
            Some(new_fingerprint) => {
                if let Some(previous) = self
                    .open_file_fingerprints
                    .insert(path.to_path_buf(), new_fingerprint.clone())
                {
                    previous != new_fingerprint
                } else {
                    true
                }
            }
            None => self.open_file_fingerprints.remove(path).is_some(),
        }
    }

    fn to_workspace_path(&self, path: &Path) -> Option<PathBuf> {
        let raw = raw_absolute_path(path, self.workspace_root.as_path());
        let workspace = self.workspace_root.as_path();

        let resolved = if raw.starts_with(workspace) {
            Some(raw)
        } else {
            raw.canonicalize()
                .ok()
                .filter(|canonical| canonical.starts_with(workspace))
        }?;

        if contains_ignored_component(resolved.as_path(), workspace) {
            return None;
        }
        Some(resolved)
    }

    fn match_open_paths(&self, path: &Path) -> FxHashSet<PathBuf> {
        let mut matched = FxHashSet::default();
        for key in path_identity_keys(path, self.workspace_root.as_path()) {
            if let Some(paths) = self.open_file_keys.get(&key) {
                matched.extend(paths.iter().cloned());
            }
        }
        matched
    }
}

fn merge_created(target: &mut FxHashMap<PathBuf, bool>, path: PathBuf, is_dir: bool) {
    if let Some(existing) = target.get_mut(&path) {
        *existing |= is_dir;
    } else {
        target.insert(path, is_dir);
    }
}

fn raw_absolute_path(path: &Path, workspace_root: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

fn path_identity_keys(path: &Path, workspace_root: &Path) -> Vec<PathBuf> {
    let raw = raw_absolute_path(path, workspace_root);
    let mut keys = vec![raw.clone()];
    if let Ok(canonical) = raw.canonicalize() {
        if canonical != raw {
            keys.push(canonical);
        }
    }
    keys
}

fn contains_ignored_component(path: &Path, workspace_root: &Path) -> bool {
    let relative = path.strip_prefix(workspace_root).unwrap_or(path);
    relative.components().any(|component| {
        if let Component::Normal(name) = component {
            should_ignore(&name.to_string_lossy())
        } else {
            false
        }
    })
}

fn infer_is_dir(path: &Path, create_kind: Option<CreateKind>) -> bool {
    match create_kind {
        Some(CreateKind::Folder) => true,
        Some(CreateKind::File) => false,
        _ => std::fs::metadata(path)
            .map(|meta| meta.is_dir())
            .unwrap_or(false),
    }
}

fn file_fingerprint(path: &Path) -> Option<FileFingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }

    Some(FileFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn normalize_notify_event(event: notify::Event) -> Vec<FsDelta> {
    match event.kind {
        EventKind::Create(create_kind) => event
            .paths
            .into_iter()
            .map(|path| FsDelta::Created {
                is_dir: infer_is_dir(path.as_path(), Some(create_kind)),
                path,
            })
            .collect(),
        EventKind::Remove(_) => event
            .paths
            .into_iter()
            .map(|path| FsDelta::Deleted { path })
            .collect(),
        EventKind::Modify(kind) => normalize_modify_event(kind, event.paths),
        _ => Vec::new(),
    }
}

fn normalize_modify_event(kind: ModifyKind, paths: Vec<PathBuf>) -> Vec<FsDelta> {
    match kind {
        ModifyKind::Name(RenameMode::Both) => {
            if paths.len() >= 2 {
                vec![FsDelta::Renamed {
                    from: paths[0].clone(),
                    to: paths[1].clone(),
                }]
            } else {
                paths
                    .into_iter()
                    .map(|path| FsDelta::Modified { path })
                    .collect()
            }
        }
        ModifyKind::Name(RenameMode::From) => paths
            .into_iter()
            .map(|path| FsDelta::Deleted { path })
            .collect(),
        ModifyKind::Name(RenameMode::To) => paths
            .into_iter()
            .map(|path| FsDelta::Created {
                is_dir: infer_is_dir(path.as_path(), None),
                path,
            })
            .collect(),
        ModifyKind::Data(_)
        | ModifyKind::Any
        | ModifyKind::Other
        | ModifyKind::Metadata(_)
        | ModifyKind::Name(_) => paths
            .into_iter()
            .map(|path| FsDelta::Modified { path })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_service_with_raw_channel(
        workspace_root: &Path,
    ) -> (FileWatcherService, std::sync::mpsc::Sender<notify::Event>) {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            |_| {},
            Config::default().with_poll_interval(WATCHER_POLL_INTERVAL),
        )
        .expect("create watcher");
        (
            FileWatcherService {
                watcher,
                raw_event_rx: rx,
                workspace_root: workspace_root.to_path_buf(),
                open_files: FxHashSet::default(),
                open_file_keys: FxHashMap::default(),
                open_file_fingerprints: FxHashMap::default(),
            },
            tx,
        )
    }

    #[test]
    fn modify_name_events_are_normalized_as_modified() {
        let path = PathBuf::from("/tmp/zcode-file-watcher-name-modified.rs");
        let event = notify::Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };

        let deltas = normalize_notify_event(event);
        assert_eq!(deltas, vec![FsDelta::Modified { path }]);
    }

    #[test]
    fn atomic_save_rename_like_event_routes_to_editor_modified_target() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let target_path = workspace_root.join("a.rs");
        std::fs::write(&target_path, "fn main() {}\n").expect("write target");
        let tmp_path = workspace_root.join(".a.rs.tmp");
        std::fs::write(&tmp_path, "fn main() { println!(\"x\"); }\n").expect("write tmp");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        service.sync_open_files([target_path.as_path()]);

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            paths: vec![tmp_path.clone(), target_path.clone()],
            attrs: Default::default(),
        })
        .expect("send event");

        let events = service.drain_events();
        assert!(
            events.contains(&FileWatchEvent::EditorModified(target_path)),
            "atomic-save rename target should trigger editor modified"
        );
        assert!(
            events.contains(&FileWatchEvent::WorkspaceRenamed {
                from: tmp_path,
                to: workspace_root.join("a.rs"),
            }),
            "rename event should be routed for workspace tree"
        );
    }

    #[test]
    fn rename_from_to_incomplete_info_should_degrade_to_delete_and_create() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let from = workspace_root.join("old.rs");
        let to = workspace_root.join("new.rs");
        std::fs::write(&from, "old").expect("write old");
        std::fs::write(&to, "new").expect("write new");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            paths: vec![from.clone()],
            attrs: Default::default(),
        })
        .expect("send from");
        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            paths: vec![to.clone()],
            attrs: Default::default(),
        })
        .expect("send to");

        let events = service.drain_events();
        assert!(events.contains(&FileWatchEvent::WorkspaceDeleted { path: from }));
        assert!(events.contains(&FileWatchEvent::WorkspaceCreated {
            path: to,
            is_dir: false,
        }));
    }

    #[test]
    fn raw_and_canonical_keys_should_both_match_open_file_identity() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let real_path = workspace_root.join("real.rs");
        std::fs::write(&real_path, "fn main() {}\n").expect("write real");
        let alias_path = workspace_root.join("real_alias.rs");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_path, &alias_path).expect("create symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_path, &alias_path).expect("create symlink");

        let canonical = real_path.canonicalize().expect("canonicalize");
        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        service.sync_open_files([alias_path.as_path()]);
        std::fs::write(&real_path, "fn main() { println!(\"changed\"); }\n").expect("rewrite real");

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![canonical],
            attrs: Default::default(),
        })
        .expect("send event");

        let events = service.drain_events();
        assert!(events.contains(&FileWatchEvent::EditorModified(alias_path)));
    }

    #[test]
    fn watch_poll_interval_is_two_hundred_fifty_ms() {
        assert_eq!(WATCHER_POLL_INTERVAL, Duration::from_millis(250));
    }

    #[test]
    fn metadata_only_modified_event_with_unchanged_fingerprint_should_not_emit_editor_modified() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let path = workspace_root.join("a.rs");
        std::fs::write(&path, "fn main() {}\n").expect("write file");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        service.sync_open_files([path.as_path()]);

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Metadata(
                notify::event::MetadataKind::AccessTime,
            )),
            paths: vec![path.clone()],
            attrs: Default::default(),
        })
        .expect("send metadata event");

        let events = service.drain_events();
        assert!(
            !events.contains(&FileWatchEvent::EditorModified(path)),
            "metadata-only noise should not trigger editor reload"
        );
    }

    #[test]
    fn acknowledged_internal_write_should_not_emit_followup_editor_modified() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let path = workspace_root.join("a.rs");
        std::fs::write(&path, "fn main() {}\n").expect("write file");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        service.sync_open_files([path.as_path()]);

        std::fs::write(&path, "fn main() { println!(\"saved\"); }\n").expect("rewrite file");
        service.acknowledge_write(path.as_path());

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![path.clone()],
            attrs: Default::default(),
        })
        .expect("send content event");

        let events = service.drain_events();
        assert!(
            !events.contains(&FileWatchEvent::EditorModified(path)),
            "self-save followup event should not trigger external reload"
        );
    }

    #[test]
    fn directory_modify_event_should_emit_workspace_dir_changed_for_parent() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let docs_dir = workspace_root.join("docs");
        std::fs::create_dir_all(&docs_dir).expect("create docs dir");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![docs_dir.clone()],
            attrs: Default::default(),
        })
        .expect("send modify event");

        let events = service.drain_events();
        assert!(
            events.contains(&FileWatchEvent::WorkspaceDirChanged { path: docs_dir }),
            "directory modify should request dir reload"
        );
    }

    #[test]
    fn modify_event_for_open_file_should_not_emit_workspace_dir_changed() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let workspace_root = dir.path().to_path_buf();
        let path = workspace_root.join("a.rs");
        std::fs::write(&path, "fn main() {}\n").expect("write file");

        let (mut service, tx) = create_service_with_raw_channel(workspace_root.as_path());
        service.sync_open_files([path.as_path()]);

        tx.send(notify::Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![path.clone()],
            attrs: Default::default(),
        })
        .expect("send modify event");

        let events = service.drain_events();
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, FileWatchEvent::WorkspaceDirChanged { .. })),
            "open file modifications should not force explorer reload"
        );
    }
}
