//! Index manifest for tracking indexed files and their state.
//!
//! The manifest persists information about what has been indexed,
//! enabling incremental updates and consistency checking.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Manifest tracking the state of indexed files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexManifest {
    /// Version of the manifest format
    pub version: u32,

    /// When the manifest was last updated
    pub last_updated: Option<SystemTime>,

    /// Map of file path to file state
    pub files: HashMap<PathBuf, FileState>,

    /// Total number of chunks in the index
    pub total_chunks: usize,
}

/// State of an indexed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    /// SHA256 hash of the file content
    pub content_hash: String,

    /// When the file was last indexed
    pub indexed_at: SystemTime,

    /// Number of chunks this file was split into
    pub chunk_count: usize,

    /// IDs of chunks in Qdrant for this file
    pub chunk_ids: Vec<String>,
}

impl IndexManifest {
    /// Create a new empty manifest.
    pub fn new() -> Self {
        Self {
            version: 1,
            last_updated: None,
            files: HashMap::new(),
            total_chunks: 0,
        }
    }

    /// Load manifest from a file.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path).map_err(ManifestError::Io)?;
        serde_json::from_str(&content).map_err(ManifestError::Parse)
    }

    /// Save manifest to a file.
    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        let content = serde_json::to_string_pretty(self).map_err(ManifestError::Serialize)?;
        std::fs::write(path, content).map_err(ManifestError::Io)
    }

    /// Check if a file needs re-indexing.
    pub fn needs_update(&self, path: &Path, current_hash: &str) -> bool {
        match self.files.get(path) {
            Some(state) => state.content_hash != current_hash,
            None => true,
        }
    }

    /// Record that a file was indexed.
    pub fn record_indexed(
        &mut self,
        path: PathBuf,
        content_hash: String,
        chunk_ids: Vec<String>,
    ) {
        let chunk_count = chunk_ids.len();

        // Update total chunks (subtract old count if file was previously indexed)
        if let Some(old_state) = self.files.get(&path) {
            self.total_chunks -= old_state.chunk_count;
        }
        self.total_chunks += chunk_count;

        self.files.insert(
            path,
            FileState {
                content_hash,
                indexed_at: SystemTime::now(),
                chunk_count,
                chunk_ids,
            },
        );

        self.last_updated = Some(SystemTime::now());
    }

    /// Remove a file from the manifest.
    pub fn remove_file(&mut self, path: &Path) -> Option<FileState> {
        if let Some(state) = self.files.remove(path) {
            self.total_chunks -= state.chunk_count;
            self.last_updated = Some(SystemTime::now());
            Some(state)
        } else {
            None
        }
    }

    /// Clear the entire manifest.
    pub fn clear(&mut self) {
        self.files.clear();
        self.total_chunks = 0;
        self.last_updated = Some(SystemTime::now());
    }

    /// Get all files that are in the manifest but not in the given set.
    pub fn find_deleted_files(&self, current_files: &[PathBuf]) -> Vec<PathBuf> {
        let current_set: std::collections::HashSet<_> = current_files.iter().collect();
        self.files
            .keys()
            .filter(|p| !current_set.contains(p))
            .cloned()
            .collect()
    }
}

/// Errors that can occur when working with manifests.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("IO error: {0}")]
    Io(#[source] std::io::Error),

    #[error("Failed to parse manifest: {0}")]
    Parse(#[source] serde_json::Error),

    #[error("Failed to serialize manifest: {0}")]
    Serialize(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_new() {
        let manifest = IndexManifest::new();
        assert_eq!(manifest.version, 1);
        assert!(manifest.files.is_empty());
        assert_eq!(manifest.total_chunks, 0);
    }

    #[test]
    fn test_needs_update() {
        let mut manifest = IndexManifest::new();
        let path = PathBuf::from("test.rs");

        // New file needs update
        assert!(manifest.needs_update(&path, "hash1"));

        // Record it
        manifest.record_indexed(path.clone(), "hash1".to_string(), vec!["c1".to_string()]);

        // Same hash doesn't need update
        assert!(!manifest.needs_update(&path, "hash1"));

        // Different hash needs update
        assert!(manifest.needs_update(&path, "hash2"));
    }

    #[test]
    fn test_record_indexed() {
        let mut manifest = IndexManifest::new();

        manifest.record_indexed(
            PathBuf::from("src/lib.rs"),
            "hash123".to_string(),
            vec!["chunk1".to_string(), "chunk2".to_string(), "chunk3".to_string()],
        );

        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.total_chunks, 3);

        let state = manifest.files.get(&PathBuf::from("src/lib.rs")).unwrap();
        assert_eq!(state.content_hash, "hash123");
        assert_eq!(state.chunk_count, 3);
        assert_eq!(state.chunk_ids.len(), 3);
    }

    #[test]
    fn test_record_indexed_updates_existing() {
        let mut manifest = IndexManifest::new();

        // First index
        manifest.record_indexed(
            PathBuf::from("test.rs"),
            "hash1".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        assert_eq!(manifest.total_chunks, 2);

        // Re-index same file with different chunks
        manifest.record_indexed(
            PathBuf::from("test.rs"),
            "hash2".to_string(),
            vec!["c3".to_string(), "c4".to_string(), "c5".to_string()],
        );

        // Should have updated total (removed 2 old, added 3 new)
        assert_eq!(manifest.total_chunks, 3);
        assert_eq!(manifest.files.len(), 1);

        let state = manifest.files.get(&PathBuf::from("test.rs")).unwrap();
        assert_eq!(state.content_hash, "hash2");
        assert_eq!(state.chunk_count, 3);
    }

    #[test]
    fn test_remove_file() {
        let mut manifest = IndexManifest::new();

        manifest.record_indexed(
            PathBuf::from("a.rs"),
            "hash_a".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        manifest.record_indexed(
            PathBuf::from("b.rs"),
            "hash_b".to_string(),
            vec!["c3".to_string()],
        );

        assert_eq!(manifest.total_chunks, 3);
        assert_eq!(manifest.files.len(), 2);

        // Remove file a
        let removed = manifest.remove_file(&PathBuf::from("a.rs"));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().chunk_count, 2);

        assert_eq!(manifest.total_chunks, 1);
        assert_eq!(manifest.files.len(), 1);
        assert!(!manifest.files.contains_key(&PathBuf::from("a.rs")));
    }

    #[test]
    fn test_remove_nonexistent_file() {
        let mut manifest = IndexManifest::new();
        let removed = manifest.remove_file(&PathBuf::from("nonexistent.rs"));
        assert!(removed.is_none());
    }

    #[test]
    fn test_find_deleted_files() {
        let mut manifest = IndexManifest::new();

        manifest.record_indexed(PathBuf::from("a.rs"), "h1".to_string(), vec![]);
        manifest.record_indexed(PathBuf::from("b.rs"), "h2".to_string(), vec![]);
        manifest.record_indexed(PathBuf::from("c.rs"), "h3".to_string(), vec![]);

        // Current files only include a.rs and c.rs (b.rs was deleted)
        let current_files = vec![PathBuf::from("a.rs"), PathBuf::from("c.rs")];
        let deleted = manifest.find_deleted_files(&current_files);

        assert_eq!(deleted.len(), 1);
        assert!(deleted.contains(&PathBuf::from("b.rs")));
    }

    #[test]
    fn test_find_deleted_files_empty() {
        let mut manifest = IndexManifest::new();
        manifest.record_indexed(PathBuf::from("a.rs"), "h1".to_string(), vec![]);

        // All files still exist
        let current_files = vec![PathBuf::from("a.rs")];
        let deleted = manifest.find_deleted_files(&current_files);
        assert!(deleted.is_empty());
    }

    #[test]
    fn test_manifest_save_load() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = IndexManifest::new();
        manifest.record_indexed(
            PathBuf::from("src/main.rs"),
            "abc123".to_string(),
            vec!["chunk1".to_string(), "chunk2".to_string()],
        );
        manifest.record_indexed(
            PathBuf::from("src/lib.rs"),
            "def456".to_string(),
            vec!["chunk3".to_string()],
        );

        // Save
        manifest.save(&path).unwrap();
        assert!(path.exists());

        // Load
        let loaded = IndexManifest::load(&path).unwrap();
        assert_eq!(loaded.version, manifest.version);
        assert_eq!(loaded.files.len(), 2);
        assert_eq!(loaded.total_chunks, 3);

        let main_state = loaded.files.get(&PathBuf::from("src/main.rs")).unwrap();
        assert_eq!(main_state.content_hash, "abc123");
        assert_eq!(main_state.chunk_ids.len(), 2);
    }

    #[test]
    fn test_manifest_load_nonexistent() {
        let result = IndexManifest::load(Path::new("/nonexistent/path/manifest.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_last_updated_tracking() {
        let mut manifest = IndexManifest::new();
        assert!(manifest.last_updated.is_none());

        manifest.record_indexed(PathBuf::from("test.rs"), "hash".to_string(), vec![]);
        assert!(manifest.last_updated.is_some());

        let first_update = manifest.last_updated;

        // Wait a tiny bit and update again
        std::thread::sleep(std::time::Duration::from_millis(10));
        manifest.record_indexed(PathBuf::from("test2.rs"), "hash2".to_string(), vec![]);

        // last_updated should be more recent
        assert!(manifest.last_updated > first_update);
    }

    #[test]
    fn test_multiple_files_total_chunks() {
        let mut manifest = IndexManifest::new();

        manifest.record_indexed(PathBuf::from("a.rs"), "h1".to_string(), vec!["1".to_string(), "2".to_string()]);
        manifest.record_indexed(PathBuf::from("b.rs"), "h2".to_string(), vec!["3".to_string(), "4".to_string(), "5".to_string()]);
        manifest.record_indexed(PathBuf::from("c.rs"), "h3".to_string(), vec!["6".to_string()]);

        assert_eq!(manifest.total_chunks, 6);

        // Remove one file
        manifest.remove_file(&PathBuf::from("b.rs"));
        assert_eq!(manifest.total_chunks, 3);
    }

    #[test]
    fn test_clear() {
        let mut manifest = IndexManifest::new();
        manifest.record_indexed(PathBuf::from("a.rs"), "h1".to_string(), vec!["1".to_string()]);
        manifest.record_indexed(PathBuf::from("b.rs"), "h2".to_string(), vec!["2".to_string()]);

        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.total_chunks, 2);

        manifest.clear();

        assert!(manifest.files.is_empty());
        assert_eq!(manifest.total_chunks, 0);
        assert!(manifest.last_updated.is_some());
    }
}
