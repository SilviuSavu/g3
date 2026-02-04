//! File watcher for automatic re-indexing.
//!
//! This module provides background file watching that triggers
//! re-indexing when source files change. It includes debouncing
//! to batch rapid file changes and avoid excessive re-indexing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};

use crate::embeddings::EmbeddingProvider;
use crate::indexer::Indexer;

/// Errors that can occur in the file watcher.
#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Failed to create watcher: {0}")]
    CreateError(String),

    #[error("Failed to watch path: {0}")]
    WatchError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),
}

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// A file was created
    Created(PathBuf),

    /// A file was modified
    Modified(PathBuf),

    /// A file was deleted
    Deleted(PathBuf),

    /// A file was renamed (old path, new path)
    Renamed(PathBuf, PathBuf),
}

/// Configuration for the file watcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce duration for file events
    pub debounce: Duration,

    /// File extensions to watch
    pub extensions: Vec<String>,

    /// Whether to respect .gitignore
    pub respect_gitignore: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(500),
            extensions: vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "tsx".to_string(),
                "jsx".to_string(),
                "go".to_string(),
            ],
            respect_gitignore: true,
        }
    }
}

/// Pending event state for debouncing.
#[derive(Debug, Clone)]
struct PendingEvent {
    /// The most recent event type for this path
    event: FileEvent,
    /// When the most recent event was received
    last_seen: Instant,
}

/// File watcher that monitors a directory for changes.
pub struct FileWatcher {
    config: WatcherConfig,
    root_path: PathBuf,
    // The watcher needs to be kept alive
    pub(crate) _watcher: Option<RecommendedWatcher>,
}

impl FileWatcher {
    /// Create a new file watcher.
    pub fn new(root_path: PathBuf, config: WatcherConfig) -> Self {
        Self {
            config,
            root_path,
            _watcher: None,
        }
    }

    /// Start watching and return a channel of file events.
    ///
    /// Events are debounced according to the configuration. Multiple rapid
    /// changes to the same file will be batched into a single event emitted
    /// after the debounce period.
    pub fn start(&mut self) -> Result<mpsc::Receiver<FileEvent>, WatcherError> {
        let (raw_tx, mut raw_rx) = mpsc::channel::<FileEvent>(100);
        let (debounced_tx, debounced_rx) = mpsc::channel(100);
        let extensions = self.config.extensions.clone();
        let debounce_duration = self.config.debounce;

        // Create the notify watcher that sends raw events
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let file_events = Self::convert_event(event, &extensions);
                for file_event in file_events {
                    // Best effort send
                    let _ = raw_tx.blocking_send(file_event);
                }
            }
        })
        .map_err(|e| WatcherError::CreateError(e.to_string()))?;

        self._watcher = Some(watcher);

        // Start watching the root path
        if let Some(ref mut watcher) = self._watcher {
            watcher
                .watch(&self.root_path, RecursiveMode::Recursive)
                .map_err(|e| WatcherError::WatchError(e.to_string()))?;
        }

        // Spawn the debouncing task
        tokio::spawn(async move {
            Self::debounce_events(&mut raw_rx, debounced_tx, debounce_duration).await;
        });

        tracing::info!("Started watching {:?} with {:?} debounce", self.root_path, debounce_duration);
        Ok(debounced_rx)
    }

    /// Debounce events by batching rapid changes to the same file.
    async fn debounce_events(
        raw_rx: &mut mpsc::Receiver<FileEvent>,
        debounced_tx: mpsc::Sender<FileEvent>,
        debounce_duration: Duration,
    ) {
        let mut pending: HashMap<PathBuf, PendingEvent> = HashMap::new();
        let tick_interval = Duration::from_millis(50); // Check for expired events every 50ms

        loop {
            // Use a timeout to periodically flush expired events
            match tokio::time::timeout(tick_interval, raw_rx.recv()).await {
                Ok(Some(event)) => {
                    let path = Self::event_path(&event);
                    let now = Instant::now();

                    pending
                        .entry(path)
                        .and_modify(|p| {
                            // Update to the latest event type and timestamp
                            p.event = Self::merge_events(&p.event, &event);
                            p.last_seen = now;
                        })
                        .or_insert(PendingEvent {
                            event,
                            last_seen: now,
                        });
                }
                Ok(None) => {
                    // Channel closed, flush remaining events and exit
                    for (_, pending_event) in pending.drain() {
                        let _ = debounced_tx.send(pending_event.event).await;
                    }
                    break;
                }
                Err(_) => {
                    // Timeout - check for expired events
                }
            }

            // Flush events that have exceeded the debounce duration
            let now = Instant::now();
            let mut to_emit = Vec::new();

            pending.retain(|path, pending_event| {
                if now.duration_since(pending_event.last_seen) >= debounce_duration {
                    to_emit.push((path.clone(), pending_event.event.clone()));
                    false // Remove from pending
                } else {
                    true // Keep in pending
                }
            });

            // Emit the debounced events
            for (_, event) in to_emit {
                if debounced_tx.send(event).await.is_err() {
                    // Receiver dropped, exit
                    return;
                }
            }
        }
    }

    /// Extract the primary path from an event.
    fn event_path(event: &FileEvent) -> PathBuf {
        match event {
            FileEvent::Created(p) => p.clone(),
            FileEvent::Modified(p) => p.clone(),
            FileEvent::Deleted(p) => p.clone(),
            FileEvent::Renamed(_, new) => new.clone(),
        }
    }

    /// Merge two events for the same file, keeping the most significant state.
    ///
    /// Priority: Deleted > Created > Modified
    /// - If a file is deleted, that's the final state
    /// - If a file is created then modified, it's effectively just created
    /// - Multiple modifications collapse to one
    fn merge_events(existing: &FileEvent, new: &FileEvent) -> FileEvent {
        match (existing, new) {
            // Delete always wins
            (_, FileEvent::Deleted(p)) => FileEvent::Deleted(p.clone()),
            (FileEvent::Deleted(p), _) => FileEvent::Deleted(p.clone()),

            // Create + Modify = Create (file is new)
            (FileEvent::Created(p), FileEvent::Modified(_)) => FileEvent::Created(p.clone()),

            // Modify + Create is unusual but treat as Create
            (FileEvent::Modified(_), FileEvent::Created(p)) => FileEvent::Created(p.clone()),

            // Rename handling - keep the most recent rename destination
            (_, FileEvent::Renamed(old, new)) => FileEvent::Renamed(old.clone(), new.clone()),
            (FileEvent::Renamed(old, new), _) => FileEvent::Renamed(old.clone(), new.clone()),

            // Multiple creates - keep as create (use first path)
            (FileEvent::Created(p), FileEvent::Created(_)) => FileEvent::Created(p.clone()),

            // Multiple modifies - keep as modify (use new path)
            (FileEvent::Modified(_), FileEvent::Modified(p)) => FileEvent::Modified(p.clone()),
        }
    }

    /// Stop watching.
    pub fn stop(&mut self) {
        self._watcher = None;
        tracing::info!("Stopped watching {:?}", self.root_path);
    }

    /// Convert a notify event to our FileEvent type.
    fn convert_event(event: Event, extensions: &[String]) -> Vec<FileEvent> {
        let mut file_events = Vec::new();

        for path in event.paths {
            // Check if the file has a watched extension
            if !Self::should_watch(&path, extensions) {
                continue;
            }

            let file_event = match event.kind {
                notify::EventKind::Create(_) => Some(FileEvent::Created(path)),
                notify::EventKind::Modify(_) => Some(FileEvent::Modified(path)),
                notify::EventKind::Remove(_) => Some(FileEvent::Deleted(path)),
                _ => None,
            };

            if let Some(fe) = file_event {
                file_events.push(fe);
            }
        }

        file_events
    }

    /// Check if a path should be watched based on its extension.
    pub fn should_watch(path: &Path, extensions: &[String]) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| extensions.iter().any(|e| e == ext))
            .unwrap_or(false)
    }
}

/// Background indexing service that watches for file changes.
pub struct IndexingService<E: EmbeddingProvider + 'static> {
    watcher: FileWatcher,
    indexer: Arc<RwLock<Indexer<E>>>,
}

impl<E: EmbeddingProvider + 'static> IndexingService<E> {
    /// Create a new indexing service.
    pub fn new(watcher: FileWatcher, indexer: Arc<RwLock<Indexer<E>>>) -> Self {
        Self { watcher, indexer }
    }

    /// Run the indexing service in the background.
    pub async fn run(mut self) -> Result<(), WatcherError> {
        let mut rx = self.watcher.start()?;

        while let Some(event) = rx.recv().await {
            match event {
                FileEvent::Created(path) | FileEvent::Modified(path) => {
                    tracing::debug!("File changed: {:?}", path);
                    let mut indexer = self.indexer.write().await;
                    if let Err(e) = indexer.index_file(&path).await {
                        tracing::error!("Failed to index {:?}: {}", path, e);
                    }
                }
                FileEvent::Deleted(path) => {
                    tracing::debug!("File deleted: {:?}", path);
                    let indexer = self.indexer.read().await;
                    if let Err(e) = indexer.remove_file(&path).await {
                        tracing::error!("Failed to remove {:?} from index: {}", path, e);
                    }
                }
                FileEvent::Renamed(old_path, new_path) => {
                    tracing::debug!("File renamed: {:?} -> {:?}", old_path, new_path);
                    {
                        let indexer = self.indexer.read().await;
                        if let Err(e) = indexer.remove_file(&old_path).await {
                            tracing::error!("Failed to remove {:?} from index: {}", old_path, e);
                        }
                    }
                    {
                        let mut indexer = self.indexer.write().await;
                        if let Err(e) = indexer.index_file(&new_path).await {
                            tracing::error!("Failed to index {:?}: {}", new_path, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce, Duration::from_millis(500));
        assert!(config.extensions.contains(&"rs".to_string()));
        assert!(config.respect_gitignore);
    }

    #[test]
    fn test_should_watch() {
        let extensions = vec!["rs".to_string(), "py".to_string()];

        assert!(FileWatcher::should_watch(Path::new("foo.rs"), &extensions));
        assert!(FileWatcher::should_watch(Path::new("bar.py"), &extensions));
        assert!(!FileWatcher::should_watch(Path::new("baz.txt"), &extensions));
        assert!(!FileWatcher::should_watch(Path::new("noext"), &extensions));
    }

    #[test]
    fn test_file_event_types() {
        let path = PathBuf::from("/test/file.rs");

        let created = FileEvent::Created(path.clone());
        let modified = FileEvent::Modified(path.clone());
        let deleted = FileEvent::Deleted(path.clone());
        let renamed = FileEvent::Renamed(path.clone(), PathBuf::from("/test/new.rs"));

        // Just verify they can be constructed
        match created {
            FileEvent::Created(p) => assert_eq!(p, path),
            _ => panic!("Wrong variant"),
        }
        match modified {
            FileEvent::Modified(p) => assert_eq!(p, path),
            _ => panic!("Wrong variant"),
        }
        match deleted {
            FileEvent::Deleted(p) => assert_eq!(p, path),
            _ => panic!("Wrong variant"),
        }
        match renamed {
            FileEvent::Renamed(old, new) => {
                assert_eq!(old, path);
                assert_eq!(new, PathBuf::from("/test/new.rs"));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_file_watcher_creation() {
        let dir = tempdir().unwrap();
        let config = WatcherConfig::default();
        let watcher = FileWatcher::new(dir.path().to_path_buf(), config);

        // Watcher created but not started
        assert!(watcher._watcher.is_none());
    }

    #[tokio::test]
    async fn test_file_watcher_start_stop() {
        let dir = tempdir().unwrap();
        let config = WatcherConfig::default();
        let mut watcher = FileWatcher::new(dir.path().to_path_buf(), config);

        // Start watching
        let _rx = watcher.start().unwrap();
        assert!(watcher._watcher.is_some());

        // Stop watching
        watcher.stop();
        assert!(watcher._watcher.is_none());
    }

    #[test]
    fn test_merge_events_delete_wins() {
        let path = PathBuf::from("/test/file.rs");
        let created = FileEvent::Created(path.clone());
        let modified = FileEvent::Modified(path.clone());
        let deleted = FileEvent::Deleted(path.clone());

        // Delete should win over created
        let merged = FileWatcher::merge_events(&created, &deleted);
        assert!(matches!(merged, FileEvent::Deleted(_)));

        // Delete should win over modified
        let merged = FileWatcher::merge_events(&modified, &deleted);
        assert!(matches!(merged, FileEvent::Deleted(_)));

        // Delete should persist
        let merged = FileWatcher::merge_events(&deleted, &created);
        assert!(matches!(merged, FileEvent::Deleted(_)));
    }

    #[test]
    fn test_merge_events_create_modify() {
        let path = PathBuf::from("/test/file.rs");
        let created = FileEvent::Created(path.clone());
        let modified = FileEvent::Modified(path.clone());

        // Create + Modify = Create
        let merged = FileWatcher::merge_events(&created, &modified);
        assert!(matches!(merged, FileEvent::Created(_)));
    }

    #[test]
    fn test_event_path_extraction() {
        let path = PathBuf::from("/test/file.rs");
        let new_path = PathBuf::from("/test/new.rs");

        assert_eq!(
            FileWatcher::event_path(&FileEvent::Created(path.clone())),
            path
        );
        assert_eq!(
            FileWatcher::event_path(&FileEvent::Modified(path.clone())),
            path
        );
        assert_eq!(
            FileWatcher::event_path(&FileEvent::Deleted(path.clone())),
            path
        );
        assert_eq!(
            FileWatcher::event_path(&FileEvent::Renamed(path.clone(), new_path.clone())),
            new_path
        );
    }
}
