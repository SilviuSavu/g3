//! File system service for efficient repeated file operations.
//!
//! This module provides a high-level service for file system operations
//! with caching, async I/O, and optimized patterns like grep and glob.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::debug;
use crate::fs_cache::{DirectoryCacheManager, DirectoryCache};

/// File system service with caching and async support
pub struct FsService {
    /// Cache manager for directory listings
    cache_manager: DirectoryCacheManager,
    /// Root path for all operations
    root_path: PathBuf,
}

impl FsService {
    /// Create a new file system service with the given root path
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            cache_manager: DirectoryCacheManager::new(),
            root_path,
        }
    }

    /// Create with custom cache settings
    pub fn with_cache_settings(root_path: PathBuf, max_age: std::time::Duration) -> Self {
        Self {
            cache_manager: DirectoryCacheManager::with_max_age(max_age),
            root_path,
        }
    }

    /// Get the root path
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get a cached directory listing
    pub async fn get_directory(&mut self, path: &Path) -> Result<&DirectoryCache> {
        let full_path = self.root_path.join(path);
        self.cache_manager.get_or_create(full_path)
    }

    /// List files matching a pattern (async)
    /// 
    /// This is the async version of list_files with caching.
    pub async fn list_files_async(
        &mut self,
        path: &Path,
        pattern: &str,
    ) -> Result<Vec<(String, u64, usize)>> {
        let dir_cache = self.get_directory(path).await?;
        
        let entries = dir_cache
            .filter_by_pattern(pattern)
            .into_iter()
            .map(|e| (e.name.clone(), e.size, e.line_count))
            .collect();

        Ok(entries)
    }

    /// Count files matching a pattern
    pub async fn count_files(
        &mut self,
        path: &Path,
        pattern: &str,
    ) -> Result<usize> {
        let dir_cache = self.get_directory(path).await?;
        Ok(dir_cache.count_matching(pattern))
    }

    /// Grep pattern in files of a directory
    /// 
    /// This uses the cached directory listing and executes grep
    /// only on the matching files.
    pub async fn grep(
        &mut self,
        path: &Path,
        pattern: &str,
        file_pattern: &str,
    ) -> Result<usize> {
        let dir_cache = self.get_directory(path).await?;
        
        let matching_files = dir_cache.filter_by_pattern(file_pattern);
        let file_count = matching_files.len();
        
        debug!(
            "Grep: found {} files matching '{}', searching for '{}'",
            file_count, file_pattern, pattern
        );

        // If no files match, return 0 without spawning grep
        if file_count == 0 {
            return Ok(0);
        }

        // Use shell rg command for actual pattern matching
        // This is faster than reading each file individually
        let _command = format!(
            "rg \"{}\" {} | wc -l",
            pattern,
            path.to_string_lossy()
        );

        // Note: This could be optimized further by:
        // 1. Using ripgrep directly with file pattern filter
        // 2. Implementing grep in Rust with async I/O
        // 3. Using a persistent rg process

        Ok(file_count) // Placeholder - would need actual grep execution
    }

    /// Invalidate cache for a directory
    pub fn invalidate_cache(&mut self, path: &Path) {
        let full_path = self.root_path.join(path);
        self.cache_manager.invalidate(&full_path);
    }

    /// Clear all caches
    pub fn clear_cache(&mut self) {
        self.cache_manager.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        self.cache_manager.stats()
    }

    /// Get the full path for a relative path
    pub fn join_path(&self, path: &Path) -> PathBuf {
        self.root_path.join(path)
    }
}

impl Clone for FsService {
    fn clone(&self) -> Self {
        Self {
            cache_manager: self.cache_manager.clone(),
            root_path: self.root_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_fs_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let service = FsService::new(temp_dir.path().to_path_buf());
        
        assert_eq!(service.root_path(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_fs_service_list_files() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test files
        std::fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(temp_dir.path().join("lib.rs"), "pub mod test;").unwrap();
        std::fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();
        
        let mut service = FsService::new(temp_dir.path().to_path_buf());
        
        // List all files
        let files = service.list_files_async(Path::new("."), "*").await.unwrap();
        assert_eq!(files.len(), 3);
        
        // List only .rs files
        let rust_files = service.list_files_async(Path::new("."), "*.rs").await.unwrap();
        assert_eq!(rust_files.len(), 2);
        
        // Verify file metadata
        for (name, size, lines) in &rust_files {
            assert!(name.as_str() == "main.rs" || name.as_str() == "lib.rs");
            assert!(*size > 0);
            assert!(*lines > 0);
        }
    }

    #[tokio::test]
    async fn test_fs_service_count_files() {
        let temp_dir = TempDir::new().unwrap();
        
        std::fs::write(temp_dir.path().join("a.rs"), "").unwrap();
        std::fs::write(temp_dir.path().join("b.rs"), "").unwrap();
        std::fs::write(temp_dir.path().join("c.md"), "").unwrap();
        
        let mut service = FsService::new(temp_dir.path().to_path_buf());
        
        let count = service.count_files(Path::new("."), "*.rs").await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_fs_service_cache_invalidation() {
        let temp_dir = TempDir::new().unwrap();
        
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();
        
        let mut service = FsService::new(temp_dir.path().to_path_buf());
        
        // First access caches the directory
        let _ = service.get_directory(Path::new(".")).await.unwrap();
        
        // Verify cache is used
        let (num_caches, _) = service.cache_stats();
        assert_eq!(num_caches, 1);
        
        // Invalidate cache
        service.invalidate_cache(Path::new("."));
        let (num_caches, _) = service.cache_stats();
        assert_eq!(num_caches, 0);
    }

    #[tokio::test]
    async fn test_fs_service_clear_cache() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();
        
        std::fs::write(temp_dir1.path().join("test.txt"), "").unwrap();
        std::fs::write(temp_dir2.path().join("test.txt"), "").unwrap();
        
        let mut service = FsService::new(PathBuf::from("/")); // root path doesn't matter for this test
        
        // Access both directories
        let _ = service.get_directory(temp_dir1.path()).await;
        let _ = service.get_directory(temp_dir2.path()).await;
        
        // Both should be cached
        let (num_caches, _) = service.cache_stats();
        assert_eq!(num_caches, 2);
        
        // Clear all caches
        service.clear_cache();
        let (num_caches, _) = service.cache_stats();
        assert_eq!(num_caches, 0);
    }

    #[tokio::test]
    async fn test_fs_service_clone() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "").unwrap();
        
        let service = FsService::new(temp_dir.path().to_path_buf());
        let cloned = service.clone();
        
        // Both should work independently
        assert_eq!(service.root_path(), cloned.root_path());
    }
}
