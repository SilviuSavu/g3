//! Directory caching module for efficient repeated file system operations.
//!
//! This module provides caching for directory listings to avoid
//! expensive filesystem operations on repeated calls.

use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::debug;

/// Cache entry for a directory listing
#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub line_count: usize,
    pub modified: SystemTime,
}

/// A cached directory listing
#[derive(Clone, Debug)]
pub struct DirectoryCache {
    /// Directory path being cached
    dir_path: PathBuf,
    /// Cached entries
    entries: Vec<DirectoryEntry>,
    /// When the cache was created
    created_at: SystemTime,
    /// Maximum age of cache before invalidation
    max_age: Duration,
}

impl DirectoryCache {
    /// Create a new directory cache for the given path
    pub fn new(dir_path: PathBuf, max_age: Duration) -> Result<Self> {
        let entries = read_directory_entries(&dir_path)?;
        let created_at = SystemTime::now();

        debug!(
            "Created cache for {} with {} entries",
            dir_path.display(),
            entries.len()
        );

        Ok(Self {
            dir_path,
            entries,
            created_at,
            max_age,
        })
    }

    /// Check if the cache has expired
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().map(|e| e > self.max_age).unwrap_or(true)
    }

    /// Get the cached entries, returning None if expired
    pub fn get_entries(&self) -> Option<&[DirectoryEntry]> {
        if self.is_expired() {
            None
        } else {
            Some(&self.entries)
        }
    }

    /// Invalidate the cache
    pub fn invalidate(&mut self) {
        debug!("Invalidating cache for {}", self.dir_path.display());
        self.entries.clear();
    }

    /// Get directory path
    pub fn dir_path(&self) -> &Path {
        &self.dir_path
    }

    /// Filter entries by pattern
    pub fn filter_by_pattern(&self, pattern: &str) -> Vec<&DirectoryEntry> {
        self.entries
            .iter()
            .filter(|entry| matches_pattern(&entry.name, pattern))
            .collect()
    }

    /// Count matching entries
    pub fn count_matching(&self, pattern: &str) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches_pattern(&entry.name, pattern))
            .count()
    }
}

/// Simple pattern matching for glob patterns
fn matches_pattern(name: &str, pattern: &str) -> bool {
    match pattern {
        "*" => true,
        ext if ext.starts_with('*') => {
            let ext_pattern = ext.strip_prefix('*').unwrap_or(ext);
            name.ends_with(ext_pattern)
        }
        _ => name == pattern,
    }
}

/// Read directory entries with metadata
fn read_directory_entries(dir_path: &Path) -> Result<Vec<DirectoryEntry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy().to_string();

        // Get metadata
        let metadata = entry.metadata()?;
        let size = metadata.len();

        // Count lines for text files
        let line_count = if file_type.is_file() {
            match fs::read_to_string(entry.path()) {
                Ok(content) => content.lines().count(),
                Err(_) => 0,
            }
        } else {
            0
        };

        let modified = metadata.modified()?;

        entries.push(DirectoryEntry {
            path: entry.path(),
            name: file_name_str,
            size,
            line_count,
            modified,
        });
    }

    Ok(entries)
}

/// Directory cache manager
#[derive(Clone, Debug)]
pub struct DirectoryCacheManager {
    /// Maximum age for cached directories
    max_age: Duration,
    /// Cache storage
    caches: HashMap<PathBuf, DirectoryCache>,
}

impl DirectoryCacheManager {
    /// Create a new cache manager with default settings
    pub fn new() -> Self {
        Self {
            max_age: Duration::from_secs(60), // 60 second max age
            caches: HashMap::new(),
        }
    }

    /// Create a cache manager with custom settings
    pub fn with_max_age(max_age: Duration) -> Self {
        Self {
            max_age,
            caches: HashMap::new(),
        }
    }

    /// Get or create a cache for a directory
    pub fn get_or_create(&mut self, dir_path: PathBuf) -> Result<&DirectoryCache> {
        // Use Entry::or_insert_with - this is the idiomatic pattern
        
        
        // Clone path for lookup before modifying
        let path_clone = dir_path.clone();
        
        // First check if cache exists (in its own scope to release borrow)
        let is_valid = {
            let cache = self.caches.get(&path_clone);
            cache.map(|c| !c.is_expired()).unwrap_or(false)
        };
        
        if is_valid {
            return Ok(self.caches.get(&path_clone).unwrap());
        }
        
        // Create and insert new cache
        let cache = DirectoryCache::new(dir_path.clone(), self.max_age)?;
        self.caches.insert(dir_path, cache);
        
        Ok(self.caches.get(&path_clone).unwrap())
    }

    /// Invalidate cache for a specific directory
    pub fn invalidate(&mut self, dir_path: &Path) {
        self.caches.remove(dir_path);
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        self.caches.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, usize) {
        let total_entries: usize = self.caches.values().map(|c| c.entries.len()).sum();
        (self.caches.len(), total_entries)
    }
}

impl Default for DirectoryCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_directory_cache_creation() {
        let dir = TempDir::new().unwrap();
        let cache = DirectoryCache::new(dir.path().to_path_buf(), Duration::from_secs(60)).unwrap();
        
        assert!(cache.get_entries().is_some());
    }

    #[test]
    fn test_directory_cache_pattern_matching() {
        let dir = TempDir::new().unwrap();
        
        // Create some test files
        File::create(dir.path().join("main.rs")).unwrap();
        File::create(dir.path().join("lib.rs")).unwrap();
        File::create(dir.path().join("README.md")).unwrap();
        
        let cache = DirectoryCache::new(dir.path().to_path_buf(), Duration::from_secs(60)).unwrap();
        
        // Test pattern matching
        let rust_files = cache.filter_by_pattern("*.rs");
        assert_eq!(rust_files.len(), 2);
        
        let md_files = cache.filter_by_pattern("*.md");
        assert_eq!(md_files.len(), 1);
        
        let all_files = cache.filter_by_pattern("*");
        assert_eq!(all_files.len(), 3);
    }

    #[test]
    fn test_directory_cache_manager() {
        let mut manager = DirectoryCacheManager::new();
        
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("test.txt")).unwrap();
        
        // First call creates cache
        let cache1 = manager.get_or_create(dir.path().to_path_buf()).unwrap();
        assert_eq!(cache1.count_matching("*"), 1);
        
        // Second call should return cached version
        let cache2 = manager.get_or_create(dir.path().to_path_buf()).unwrap();
        assert_eq!(cache2.count_matching("*"), 1);
        
        // Check stats
        let (num_caches, total_entries) = manager.stats();
        assert_eq!(num_caches, 1);
        assert_eq!(total_entries, 1);
    }

    #[test]
    fn test_directory_cache_invalidates_on_expiry() {
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("test.txt")).unwrap();
        
        // Create cache with very short expiry
        let cache = DirectoryCache::new(dir.path().to_path_buf(), Duration::from_nanos(1)).unwrap();
        
        // Initially valid
        
        // Wait for expiry
        std::thread::sleep(Duration::from_millis(1));
        
        // Should be expired
        
        // After expiry, get_entries() should return None
        assert!(cache.get_entries().is_none());
        assert!(cache.is_expired());
    }

    #[test]
    fn test_directory_cache_manager_recreate_expired() {
        let mut manager = DirectoryCacheManager::with_max_age(Duration::from_nanos(1));
        
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("test.txt")).unwrap();
        
        // First call
        let _ = manager.get_or_create(dir.path().to_path_buf()).unwrap();
        
        // Wait for expiry
        std::thread::sleep(Duration::from_millis(1));
        
        // Second call should recreate cache
        let cache = manager.get_or_create(dir.path().to_path_buf()).unwrap();
        assert_eq!(cache.count_matching("*"), 1);
    }
}
