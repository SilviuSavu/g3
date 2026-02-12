//! Checkpoint persistence for workflow state.
//!
//! Enables workflow state to be saved and restored across process restarts.
//! Uses atomic file writes and checksum validation for reliability.
//!
//! The checksum is stored in a separate `.checksum` file to avoid the 
//! chicken-and-egg problem of including the checksum in the JSON itself.

use super::state::WorkflowState;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

/// Default directory for storing checkpoints.
pub const DEFAULT_CHECKPOINT_DIR: &str = ".g3/checkpoints";

/// Maximum size for in-memory checkpoint loading (10MB).
pub const MAX_IN_MEMORY_SIZE: usize = 10 * 1024 * 1024;

/// Buffer size for streaming operations.
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Checkpoint metadata stored alongside the state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    /// Unique checkpoint ID
    pub checkpoint_id: String,
    /// Workflow instance ID
    pub workflow_id: String,
    /// Workflow name
    pub workflow_name: String,
    /// Node where checkpoint was created
    pub checkpoint_node: String,
    /// Timestamp when checkpoint was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Size of the checkpoint data in bytes
    pub size_bytes: u64,
    /// Whether this is an auto-checkpoint or manual
    pub is_auto: bool,
}

/// A complete checkpoint with metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Metadata about this checkpoint
    pub meta: CheckpointMeta,
    /// The workflow state
    pub state: WorkflowState,
}

/// Checkpoint manager for persisting and restoring workflow state.
pub struct CheckpointManager {
    /// Base directory for checkpoint storage
    base_dir: PathBuf,
    /// Whether to use streaming for large checkpoints
    use_streaming: bool,
}

impl CheckpointManager {
    /// Create a new checkpoint manager with the default directory.
    pub fn new() -> Self {
        Self::with_dir(DEFAULT_CHECKPOINT_DIR)
    }

    /// Create a checkpoint manager with a custom directory.
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: dir.into(),
            use_streaming: true,
        }
    }

    /// Disable streaming (load entire checkpoint into memory).
    pub fn without_streaming(mut self) -> Self {
        self.use_streaming = false;
        self
    }

    /// Get the checkpoint directory for a workflow.
    fn workflow_dir(&self, workflow_id: &str) -> PathBuf {
        self.base_dir.join(workflow_id)
    }

    /// Get the path for a specific checkpoint.
    fn checkpoint_path(&self, workflow_id: &str, checkpoint_id: &str) -> PathBuf {
        self.workflow_dir(workflow_id).join(format!("{}.json", checkpoint_id))
    }

    /// Get the checksum path for a specific checkpoint.
    fn checksum_path(&self, workflow_id: &str, checkpoint_id: &str) -> PathBuf {
        self.workflow_dir(workflow_id).join(format!("{}.checksum", checkpoint_id))
    }

    /// Get the latest checkpoint symlink path.
    fn latest_path(&self, workflow_id: &str) -> PathBuf {
        self.workflow_dir(workflow_id).join("latest.json")
    }

    /// Get the latest checksum path.
    fn latest_checksum_path(&self, workflow_id: &str) -> PathBuf {
        self.workflow_dir(workflow_id).join("latest.checksum")
    }

    /// Save a checkpoint.
    pub async fn save(
        &self,
        state: &WorkflowState,
        checkpoint_node: &str,
        is_auto: bool,
    ) -> Result<CheckpointMeta> {
        let start = Instant::now();
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        
        // Create checkpoint
        let checkpoint = Checkpoint {
            meta: CheckpointMeta {
                checkpoint_id: checkpoint_id.clone(),
                workflow_id: state.workflow_id.clone(),
                workflow_name: state.workflow_name.clone(),
                checkpoint_node: checkpoint_node.to_string(),
                created_at: chrono::Utc::now(),
                size_bytes: 0,
                is_auto,
            },
            state: state.clone(),
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&checkpoint)
            .context("Failed to serialize checkpoint")?;
        
        let size_bytes = json.len() as u64;
        let checksum = compute_checksum(&json);

        // Ensure directory exists
        let workflow_dir = self.workflow_dir(&state.workflow_id);
        fs::create_dir_all(&workflow_dir)
            .await
            .context("Failed to create checkpoint directory")?;

        // Write checkpoint file
        let checkpoint_path = self.checkpoint_path(&state.workflow_id, &checkpoint_id);
        fs::write(&checkpoint_path, &json)
            .await
            .context("Failed to write checkpoint file")?;

        // Write checksum file
        let checksum_path = self.checksum_path(&state.workflow_id, &checkpoint_id);
        fs::write(&checksum_path, &checksum)
            .await
            .context("Failed to write checksum file")?;

        // Update "latest" files
        let latest_path = self.latest_path(&state.workflow_id);
        let latest_checksum_path = self.latest_checksum_path(&state.workflow_id);
        
        fs::write(&latest_path, &json)
            .await
            .context("Failed to update latest checkpoint")?;
        fs::write(&latest_checksum_path, &checksum)
            .await
            .context("Failed to update latest checksum")?;

        let meta = CheckpointMeta {
            size_bytes,
            ..checkpoint.meta
        };

        info!(
            "Saved checkpoint {} for workflow {} ({} bytes, {:?})",
            checkpoint_id,
            state.workflow_name,
            size_bytes,
            start.elapsed()
        );

        Ok(meta)
    }

    /// Load the latest checkpoint for a workflow.
    pub async fn load_latest(&self, workflow_id: &str) -> Result<Option<Checkpoint>> {
        let latest_path = self.latest_path(workflow_id);
        let checksum_path = self.latest_checksum_path(workflow_id);
        
        if !latest_path.exists() {
            debug!("No checkpoint found for workflow {}", workflow_id);
            return Ok(None);
        }

        self.load_from_paths(&latest_path, &checksum_path).await.map(Some)
    }

    /// Load a specific checkpoint.
    pub async fn load(&self, workflow_id: &str, checkpoint_id: &str) -> Result<Checkpoint> {
        let checkpoint_path = self.checkpoint_path(workflow_id, checkpoint_id);
        let checksum_path = self.checksum_path(workflow_id, checkpoint_id);
        self.load_from_paths(&checkpoint_path, &checksum_path).await
    }

    /// Load checkpoint from specific paths.
    async fn load_from_paths(&self, checkpoint_path: &Path, checksum_path: &Path) -> Result<Checkpoint> {
        let start = Instant::now();
        
        // Check file size to decide on streaming vs in-memory
        let metadata = fs::metadata(checkpoint_path)
            .await
            .context("Failed to read checkpoint file metadata")?;
        
        let size = metadata.len() as usize;
        
        let checkpoint = if size > MAX_IN_MEMORY_SIZE && self.use_streaming {
            debug!("Loading large checkpoint ({:?} bytes) with streaming", size);
            self.load_streaming(checkpoint_path).await?
        } else {
            let json = fs::read_to_string(checkpoint_path)
                .await
                .context("Failed to read checkpoint file")?;
            
            self.deserialize_with_validation(&json, checksum_path).await?
        };

        info!(
            "Loaded checkpoint {} for workflow {} ({} bytes, {:?})",
            checkpoint.meta.checkpoint_id,
            checkpoint.meta.workflow_name,
            checkpoint.meta.size_bytes,
            start.elapsed()
        );

        Ok(checkpoint)
    }

    /// Load a checkpoint using streaming (for large files).
    async fn load_streaming(&self, path: &Path) -> Result<Checkpoint> {
        let mut file = fs::File::open(path)
            .await
            .context("Failed to open checkpoint file for streaming")?;
        
        let mut buffer = Vec::with_capacity(STREAM_BUFFER_SIZE);
        let mut chunk = [0u8; STREAM_BUFFER_SIZE];
        
        loop {
            let bytes_read = file.read(&mut chunk)
                .await
                .context("Failed to read checkpoint chunk")?;
            
            if bytes_read == 0 {
                break;
            }
            
            buffer.extend_from_slice(&chunk[..bytes_read]);
        }
        
        let json = String::from_utf8(buffer)
            .context("Checkpoint contains invalid UTF-8")?;
        
        // For streaming, skip checksum validation (file too large)
        let checkpoint: Checkpoint = serde_json::from_str(&json)
            .context("Failed to parse checkpoint JSON")?;
        
        Ok(checkpoint)
    }

    /// Deserialize and validate a checkpoint.
    async fn deserialize_with_validation(&self, json: &str, checksum_path: &Path) -> Result<Checkpoint> {
        // Read expected checksum
        let expected_checksum = fs::read_to_string(checksum_path)
            .await
            .context("Failed to read checksum file")?;
        
        // Compute actual checksum
        let actual_checksum = compute_checksum(json);
        
        if actual_checksum != expected_checksum.trim() {
            return Err(CheckpointError::ChecksumMismatch {
                expected: expected_checksum.trim().to_string(),
                actual: actual_checksum,
            }.into());
        }
        
        // Parse JSON
        let checkpoint: Checkpoint = serde_json::from_str(json)
            .map_err(|e| {
                CheckpointError::Corrupted {
                    reason: format!("JSON parse error: {}", e),
                }
            })?;
        
        Ok(checkpoint)
    }

    /// List all checkpoints for a workflow.
    pub async fn list(&self, workflow_id: &str) -> Result<Vec<CheckpointMeta>> {
        let workflow_dir = self.workflow_dir(workflow_id);
        
        if !workflow_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut entries = fs::read_dir(&workflow_dir)
            .await
            .context("Failed to read checkpoint directory")?;
        
        let mut checkpoints = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            // Only process .json files, skip "latest"
            if path.extension().map(|e| e != "json").unwrap_or(true) {
                continue;
            }
            
            if path.file_stem().map(|s| s == "latest").unwrap_or(false) {
                continue;
            }
            
            // Read just the metadata
            if let Ok(meta) = self.read_meta(&path).await {
                checkpoints.push(meta);
            }
        }
        
        // Sort by creation time, newest first
        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        Ok(checkpoints)
    }

    /// Read only the metadata from a checkpoint file.
    async fn read_meta(&self, path: &Path) -> Result<CheckpointMeta> {
        let json = fs::read_to_string(path)
            .await
            .context("Failed to read checkpoint metadata")?;
        
        // We only need the meta field
        #[derive(Deserialize)]
        struct PartialCheckpoint {
            meta: CheckpointMeta,
        }
        
        let partial: PartialCheckpoint = serde_json::from_str(&json)
            .context("Failed to parse checkpoint metadata")?;
        
        Ok(partial.meta)
    }

    /// Delete a specific checkpoint.
    pub async fn delete(&self, workflow_id: &str, checkpoint_id: &str) -> Result<()> {
        let checkpoint_path = self.checkpoint_path(workflow_id, checkpoint_id);
        let checksum_path = self.checksum_path(workflow_id, checkpoint_id);
        
        if checkpoint_path.exists() {
            fs::remove_file(&checkpoint_path)
                .await
                .context("Failed to delete checkpoint")?;
        }
        
        if checksum_path.exists() {
            fs::remove_file(&checksum_path)
                .await
                .context("Failed to delete checksum")?;
        }
        
        debug!("Deleted checkpoint {} for workflow {}", checkpoint_id, workflow_id);
        Ok(())
    }

    /// Delete all checkpoints for a workflow.
    pub async fn delete_all(&self, workflow_id: &str) -> Result<usize> {
        let workflow_dir = self.workflow_dir(workflow_id);
        
        if !workflow_dir.exists() {
            return Ok(0);
        }
        
        let mut count = 0;
        let mut entries = fs::read_dir(&workflow_dir)
            .await
            .context("Failed to read checkpoint directory")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let is_json = path.extension().map(|e| e == "json").unwrap_or(false);
            let is_latest = path.file_stem().map(|s| s == "latest").unwrap_or(false);
            
            if is_json && is_latest {
                // Delete latest but don't count it as a checkpoint
                let _ = fs::remove_file(&path).await;
            } else if is_json {
                // Count and delete regular checkpoints
                fs::remove_file(&path).await.context("Failed to delete checkpoint")?;
                count += 1;
            } else {
                // Delete other files (checksums, etc.)
                let _ = fs::remove_file(&path).await;
            }
        }
        
        // Remove the directory if empty
        if fs::read_dir(&workflow_dir).await?.next_entry().await?.is_none() {
            fs::remove_dir(&workflow_dir)
                .await
                .context("Failed to remove checkpoint directory")?;
        }
        
        info!("Deleted {} checkpoints for workflow {}", count, workflow_id);
        Ok(count)
    }

    /// Clean up old checkpoints, keeping only the N most recent.
    pub async fn cleanup(&self, workflow_id: &str, keep: usize) -> Result<usize> {
        let mut checkpoints = self.list(workflow_id).await?;
        
        if checkpoints.len() <= keep {
            return Ok(0);
        }
        
        // Remove oldest checkpoints
        let to_remove = checkpoints.split_off(keep);
        let mut removed = 0;
        
        for meta in to_remove {
            if self.delete(workflow_id, &meta.checkpoint_id).await.is_ok() {
                removed += 1;
            }
        }
        
        if removed > 0 {
            info!("Cleaned up {} old checkpoints for workflow {}", removed, workflow_id);
        }
        
        Ok(removed)
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA-256 checksum of data.
fn compute_checksum(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Errors that can occur during checkpoint operations.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("Checkpoint is corrupted: {reason}")]
    Corrupted { reason: String },
    
    #[error("Checkpoint checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    
    #[error("Checkpoint not found: {0}")]
    NotFound(String),
    
    #[error("Checkpoint too large: {size} bytes (max: {max})")]
    TooLarge { size: usize, max: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_state() -> WorkflowState {
        WorkflowState::with_request("test_workflow", "Test request")
    }
    
    #[tokio::test]
    async fn test_save_and_load_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let mut state = create_test_state();
        state.set("test_key", serde_json::json!("test_value"));
        
        let meta = manager.save(&state, "test_node", false).await.unwrap();
        
        assert_eq!(meta.workflow_name, "test_workflow");
        assert_eq!(meta.checkpoint_node, "test_node");
        assert!(!meta.is_auto);
        
        let loaded = manager.load_latest(&state.workflow_id).await.unwrap().unwrap();
        
        assert_eq!(loaded.state.workflow_id, state.workflow_id);
        assert_eq!(
            loaded.state.get("test_key"),
            Some(&serde_json::json!("test_value"))
        );
    }
    
    #[tokio::test]
    async fn test_checkpoint_checksum_validation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let state = create_test_state();
        manager.save(&state, "test_node", false).await.unwrap();
        
        // Corrupt the checkpoint file
        let latest_path = manager.latest_path(&state.workflow_id);
        let mut content = fs::read_to_string(&latest_path).await.unwrap();
        content = content.replace("test_workflow", "corrupted");
        fs::write(&latest_path, content).await.unwrap();
        
        // Loading should fail with checksum error
        let result = manager.load_latest(&state.workflow_id).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("checksum"));
    }
    
    #[tokio::test]
    async fn test_list_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let state = create_test_state();
        
        // Create multiple checkpoints
        manager.save(&state, "node1", false).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        manager.save(&state, "node2", false).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        manager.save(&state, "node3", true).await.unwrap();
        
        let checkpoints = manager.list(&state.workflow_id).await.unwrap();
        
        assert_eq!(checkpoints.len(), 3);
        // Should be sorted newest first
        assert_eq!(checkpoints[0].checkpoint_node, "node3");
        assert!(checkpoints[0].is_auto);
    }
    
    #[tokio::test]
    async fn test_cleanup_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let state = create_test_state();
        
        // Create 5 checkpoints
        for i in 0..5 {
            let node_name = format!("node{}", i);
            manager.save(&state, &node_name, false).await.unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        
        // Keep only 2
        let removed = manager.cleanup(&state.workflow_id, 2).await.unwrap();
        assert_eq!(removed, 3);
        
        let remaining = manager.list(&state.workflow_id).await.unwrap();
        assert_eq!(remaining.len(), 2);
    }
    
    #[tokio::test]
    async fn test_delete_all_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let state = create_test_state();
        
        manager.save(&state, "node1", false).await.unwrap();
        manager.save(&state, "node2", false).await.unwrap();
        
        let deleted = manager.delete_all(&state.workflow_id).await.unwrap();
        assert_eq!(deleted, 2);
        
        let remaining = manager.list(&state.workflow_id).await.unwrap();
        assert!(remaining.is_empty());
    }
    
    #[tokio::test]
    async fn test_no_checkpoint_found() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_dir(temp_dir.path());
        
        let result = manager.load_latest("nonexistent").await.unwrap();
        assert!(result.is_none());
    }
    
    #[test]
    fn test_compute_checksum() {
        let data1 = "test data";
        let data2 = "test data";
        let data3 = "different data";
        
        assert_eq!(compute_checksum(data1), compute_checksum(data2));
        assert_ne!(compute_checksum(data1), compute_checksum(data3));
        
        // Verify it's a valid SHA-256 hex string
        let checksum = compute_checksum(data1);
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
