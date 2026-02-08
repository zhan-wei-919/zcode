use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SUPPRESS_DURATION: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum FileWatchEvent {
    Modified(PathBuf),
    Removed(PathBuf),
}

pub struct FileWatcherService {
    watcher: RecommendedWatcher,
    event_rx: mpsc::Receiver<FileWatchEvent>,
    watched_paths: FxHashSet<PathBuf>,
    suppress_until: FxHashMap<PathBuf, Instant>,
}

impl FileWatcherService {
    pub fn new() -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                let is_modify = matches!(
                    event.kind,
                    EventKind::Modify(notify::event::ModifyKind::Data(_)) | EventKind::Create(_)
                );
                let is_remove = matches!(event.kind, EventKind::Remove(_));
                if is_modify {
                    for path in event.paths {
                        let _ = tx.send(FileWatchEvent::Modified(path));
                    }
                } else if is_remove {
                    for path in event.paths {
                        let _ = tx.send(FileWatchEvent::Removed(path));
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;
        Ok(Self {
            watcher,
            event_rx: rx,
            watched_paths: FxHashSet::default(),
            suppress_until: FxHashMap::default(),
        })
    }
    pub fn watch(&mut self, path: &Path) {
        if self.watched_paths.contains(path) {
            return;
        }
        if self
            .watcher
            .watch(path, RecursiveMode::NonRecursive)
            .is_ok()
        {
            self.watched_paths.insert(path.to_path_buf());
        }
    }

    pub fn unwatch(&mut self, path: &Path) {
        if self.watched_paths.remove(path) {
            let _ = self.watcher.unwatch(path);
        }
    }

    pub fn suppress_next(&mut self, path: &Path) {
        self.suppress_until
            .insert(path.to_path_buf(), Instant::now() + SUPPRESS_DURATION);
    }

    pub fn watched_paths(&self) -> &FxHashSet<PathBuf> {
        &self.watched_paths
    }

    pub fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        let now = Instant::now();
        self.suppress_until.retain(|_, deadline| *deadline > now);

        let mut events = Vec::new();
        let mut seen_modified = FxHashSet::default();
        let mut seen_removed = FxHashSet::default();

        while let Ok(event) = self.event_rx.try_recv() {
            let path = match &event {
                FileWatchEvent::Modified(p) | FileWatchEvent::Removed(p) => p,
            };
            if self.suppress_until.contains_key(path) {
                continue;
            }
            let inserted = match &event {
                FileWatchEvent::Modified(_) => seen_modified.insert(path.clone()),
                FileWatchEvent::Removed(_) => seen_removed.insert(path.clone()),
            };
            if !inserted {
                continue;
            }
            events.push(event);
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_service_with_channel(
    ) -> (FileWatcherService, std::sync::mpsc::Sender<FileWatchEvent>) {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            |_| {},
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .expect("create watcher");
        (
            FileWatcherService {
                watcher,
                event_rx: rx,
                watched_paths: FxHashSet::default(),
                suppress_until: FxHashMap::default(),
            },
            tx,
        )
    }

    #[test]
    fn drain_events_should_preserve_removed_when_modified_and_removed_arrive_together() {
        let (mut service, tx) = create_service_with_channel();
        let path = PathBuf::from("/tmp/zcode-file-watcher-dedup-test");

        tx.send(FileWatchEvent::Modified(path.clone()))
            .expect("send modified");
        tx.send(FileWatchEvent::Removed(path.clone()))
            .expect("send removed");

        let events = service.drain_events();

        assert!(
            events
                .iter()
                .any(|event| matches!(event, FileWatchEvent::Removed(p) if p == &path)),
            "removed event should not be dropped when modified and removed are in same drain cycle"
        );
    }
}
