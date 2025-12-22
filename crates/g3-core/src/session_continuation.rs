//! Session continuation support for long-running interactive sessions.
//!
//! This module provides functionality to save and restore session state,
//! allowing users to resume work across multiple g3 invocations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, error, warn};

/// Version of the session continuation format
const CONTINUATION_VERSION: &str = "1.0";

/// Session continuation artifact containing all information needed to resume a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContinuation {
    /// Version of the continuation format
    pub version: String,
    /// Timestamp when the continuation was saved
    pub created_at: String,
    /// Original session ID
    pub session_id: String,
    /// The last final_output summary
    pub final_output_summary: Option<String>,
    /// Path to the full session log (g3_session_*.json)
    pub session_log_path: String,
    /// Context window usage percentage when saved
    pub context_percentage: f32,
    /// Snapshot of the TODO list content
    pub todo_snapshot: Option<String>,
    /// Working directory where the session was running
    pub working_directory: String,
}

impl SessionContinuation {
    /// Create a new session continuation artifact
    pub fn new(
        session_id: String,
        final_output_summary: Option<String>,
        session_log_path: String,
        context_percentage: f32,
        todo_snapshot: Option<String>,
        working_directory: String,
    ) -> Self {
        Self {
            version: CONTINUATION_VERSION.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            session_id,
            final_output_summary,
            session_log_path,
            context_percentage,
            todo_snapshot,
            working_directory,
        }
    }

    /// Check if the context can be fully restored (< 80% used)
    pub fn can_restore_full_context(&self) -> bool {
        self.context_percentage < 80.0
    }
}

/// Get the path to the .g3/session directory
pub fn get_session_dir() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    current_dir.join(".g3").join("session")
}

/// Get the path to the latest.json continuation file
pub fn get_latest_continuation_path() -> PathBuf {
    get_session_dir().join("latest.json")
}

/// Ensure the .g3/session directory exists
pub fn ensure_session_dir() -> Result<PathBuf> {
    let session_dir = get_session_dir();
    if !session_dir.exists() {
        std::fs::create_dir_all(&session_dir)?;
        debug!("Created session directory: {:?}", session_dir);
    }
    Ok(session_dir)
}

/// Save a session continuation artifact
pub fn save_continuation(continuation: &SessionContinuation) -> Result<PathBuf> {
    let session_dir = ensure_session_dir()?;
    let latest_path = session_dir.join("latest.json");
    
    let json = serde_json::to_string_pretty(continuation)?;
    std::fs::write(&latest_path, &json)?;
    
    debug!("Saved session continuation to {:?}", latest_path);
    Ok(latest_path)
}

/// Load the latest session continuation artifact if it exists
pub fn load_continuation() -> Result<Option<SessionContinuation>> {
    let latest_path = get_latest_continuation_path();
    
    if !latest_path.exists() {
        debug!("No continuation file found at {:?}", latest_path);
        return Ok(None);
    }
    
    let json = std::fs::read_to_string(&latest_path)?;
    let continuation: SessionContinuation = serde_json::from_str(&json)?;
    
    // Validate version
    if continuation.version != CONTINUATION_VERSION {
        warn!(
            "Continuation version mismatch: expected {}, got {}",
            CONTINUATION_VERSION, continuation.version
        );
    }
    
    debug!("Loaded session continuation from {:?}", latest_path);
    Ok(Some(continuation))
}

/// Clear all session continuation artifacts (for /clear command)
pub fn clear_continuation() -> Result<()> {
    let session_dir = get_session_dir();
    
    if session_dir.exists() {
        // Remove all files in the session directory
        for entry in std::fs::read_dir(&session_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                std::fs::remove_file(&path)?;
                debug!("Removed session file: {:?}", path);
            }
        }
        debug!("Cleared session continuation artifacts");
    }
    
    Ok(())
}

/// Check if a continuation exists and is valid
pub fn has_valid_continuation() -> bool {
    match load_continuation() {
        Ok(Some(continuation)) => {
            // Check if the session log still exists
            let session_log_path = PathBuf::from(&continuation.session_log_path);
            if !session_log_path.exists() {
                warn!("Session log no longer exists: {:?}", session_log_path);
                return false;
            }
            
            // Check if we're in the same working directory
            let current_dir = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            
            if current_dir != continuation.working_directory {
                debug!(
                    "Working directory changed: {} -> {}",
                    continuation.working_directory, current_dir
                );
                // Still valid, but user should be aware
            }
            
            true
        }
        Ok(None) => false,
        Err(e) => {
            error!("Error checking continuation: {}", e);
            false
        }
    }
}

/// Load the full context window from a session log file
pub fn load_context_from_session_log(session_log_path: &Path) -> Result<Option<serde_json::Value>> {
    if !session_log_path.exists() {
        return Ok(None);
    }
    
    let json = std::fs::read_to_string(session_log_path)?;
    let session_data: serde_json::Value = serde_json::from_str(&json)?;
    
    Ok(Some(session_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_continuation_creation() {
        let continuation = SessionContinuation::new(
            "test_session_123".to_string(),
            Some("Task completed successfully".to_string()),
            "/path/to/session.json".to_string(),
            45.0,
            Some("- [x] Task 1\n- [ ] Task 2".to_string()),
            "/home/user/project".to_string(),
        );
        
        assert_eq!(continuation.version, CONTINUATION_VERSION);
        assert_eq!(continuation.session_id, "test_session_123");
        assert!(continuation.can_restore_full_context());
    }

    #[test]
    fn test_can_restore_full_context() {
        let mut continuation = SessionContinuation::new(
            "test".to_string(),
            None,
            "path".to_string(),
            50.0,
            None,
            ".".to_string(),
        );
        
        assert!(continuation.can_restore_full_context()); // 50% < 80%
        
        continuation.context_percentage = 80.0;
        assert!(!continuation.can_restore_full_context()); // 80% >= 80%
        
        continuation.context_percentage = 95.0;
        assert!(!continuation.can_restore_full_context()); // 95% >= 80%
    }
}
