//! Main indexer orchestrator.
//!
//! The Indexer coordinates between the chunker, embedding provider,
//! and Qdrant client to index and update a codebase.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::chunker::{Chunk, CodeChunker};
use crate::embeddings::EmbeddingProvider;
use crate::manifest::IndexManifest;
use crate::qdrant::{Point, PointPayload, QdrantClient};
use crate::search::BM25Index;

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Root directory to index
    pub root_path: PathBuf,
    /// Qdrant collection name
    pub collection_name: String,
    /// Batch size for embedding requests
    pub embedding_batch_size: usize,
    /// Whether to respect .gitignore
    pub respect_gitignore: bool,
    /// File extensions to index
    pub extensions: Vec<String>,
    /// Maximum chunk tokens
    pub max_chunk_tokens: usize,
    /// Include context in chunks
    pub include_context: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("."),
            collection_name: crate::DEFAULT_COLLECTION.to_string(),
            embedding_batch_size: 32,
            respect_gitignore: true,
            extensions: vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "go".to_string(),
            ],
            max_chunk_tokens: 500,
            include_context: true,
        }
    }
}

/// Statistics about an indexing operation.
#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    /// Number of files processed
    pub files_processed: usize,
    /// Number of chunks created
    pub chunks_created: usize,
    /// Number of chunks updated
    pub chunks_updated: usize,
    /// Number of chunks deleted
    pub chunks_deleted: usize,
    /// Number of files skipped (unsupported language)
    pub files_skipped: usize,
    /// Total time in milliseconds
    pub duration_ms: u64,
    /// Number of embedding API calls
    pub embedding_calls: usize,
}

/// Main indexer that orchestrates codebase indexing.
pub struct Indexer<E: EmbeddingProvider> {
    config: IndexerConfig,
    chunker: CodeChunker,
    embeddings: Arc<E>,
    qdrant: QdrantClient,
    manifest: Arc<RwLock<IndexManifest>>,
    bm25_index: Arc<RwLock<BM25Index>>,
}

impl<E: EmbeddingProvider> Indexer<E> {
    /// Create a new indexer with the given configuration.
    pub fn new(config: IndexerConfig, embeddings: Arc<E>, qdrant: QdrantClient) -> Result<Self> {
        let chunker = CodeChunker::new(config.max_chunk_tokens, config.include_context)?;

        Ok(Self {
            config,
            chunker,
            embeddings,
            qdrant,
            manifest: Arc::new(RwLock::new(IndexManifest::new())),
            bm25_index: Arc::new(RwLock::new(BM25Index::new())),
        })
    }

    /// Create indexer with existing manifest and BM25 index.
    pub fn with_state(
        config: IndexerConfig,
        embeddings: Arc<E>,
        qdrant: QdrantClient,
        manifest: IndexManifest,
        bm25_index: BM25Index,
    ) -> Result<Self> {
        let chunker = CodeChunker::new(config.max_chunk_tokens, config.include_context)?;

        Ok(Self {
            config,
            chunker,
            embeddings,
            qdrant,
            manifest: Arc::new(RwLock::new(manifest)),
            bm25_index: Arc::new(RwLock::new(bm25_index)),
        })
    }

    /// Index the entire codebase from scratch.
    pub async fn index_all(&mut self, force: bool) -> Result<IndexStats> {
        let start = Instant::now();
        info!("Starting full codebase index of {:?}", self.config.root_path);

        let mut stats = IndexStats::default();

        // Clear existing index if force
        if force {
            self.qdrant.delete_collection().await.ok();
            self.qdrant.ensure_collection().await?;
            self.manifest.write().await.clear();
            self.bm25_index.write().await.clear();
        } else {
            self.qdrant.ensure_collection().await?;
        }

        // Collect all files to index
        let files = self.collect_files()?;
        info!("Found {} files to index", files.len());

        // Process files in batches
        let mut all_chunks: Vec<(Chunk, String)> = Vec::new(); // (chunk, file_hash)

        for file_path in &files {
            match self.process_file(file_path).await {
                Ok((chunks, hash)) => {
                    stats.files_processed += 1;
                    for chunk in chunks {
                        all_chunks.push((chunk, hash.clone()));
                    }
                }
                Err(e) => {
                    debug!("Skipping file {:?}: {}", file_path, e);
                    stats.files_skipped += 1;
                }
            }
        }

        // Generate embeddings in batches and upsert
        stats.chunks_created = all_chunks.len();
        self.embed_and_upsert(&all_chunks, &mut stats).await?;

        stats.duration_ms = start.elapsed().as_millis() as u64;
        info!("Indexing complete: {:?}", stats);
        Ok(stats)
    }

    /// Index only files that have changed since the last index.
    pub async fn index_incremental(&mut self) -> Result<IndexStats> {
        let start = Instant::now();
        info!(
            "Starting incremental index of {:?}",
            self.config.root_path
        );

        let mut stats = IndexStats::default();
        self.qdrant.ensure_collection().await?;

        // Get current files
        let current_files: HashSet<PathBuf> = self.collect_files()?.into_iter().collect();

        // Get previously indexed files
        let manifest = self.manifest.read().await;
        let indexed_files: HashSet<PathBuf> = manifest.files.keys().cloned().collect();
        drop(manifest);

        // Find deleted files
        let deleted: Vec<PathBuf> = indexed_files.difference(&current_files).cloned().collect();
        for path in &deleted {
            if let Err(e) = self.remove_file(path).await {
                warn!("Failed to remove deleted file {:?}: {}", path, e);
            }
            stats.chunks_deleted += 1;
        }

        // Find new or changed files
        let mut chunks_to_add: Vec<(Chunk, String)> = Vec::new();

        for file_path in &current_files {
            let current_hash = Self::compute_file_hash(file_path)?;

            let needs_update = {
                let manifest = self.manifest.read().await;
                manifest.needs_update(file_path, &current_hash)
            };

            if needs_update {
                // Remove old chunks for this file
                self.remove_file(file_path).await.ok();

                match self.process_file(file_path).await {
                    Ok((chunks, hash)) => {
                        stats.files_processed += 1;
                        for chunk in chunks {
                            chunks_to_add.push((chunk, hash.clone()));
                        }
                    }
                    Err(e) => {
                        debug!("Skipping file {:?}: {}", file_path, e);
                        stats.files_skipped += 1;
                    }
                }
            }
        }

        stats.chunks_created = chunks_to_add.len();
        self.embed_and_upsert(&chunks_to_add, &mut stats).await?;

        stats.duration_ms = start.elapsed().as_millis() as u64;
        info!("Incremental indexing complete: {:?}", stats);
        Ok(stats)
    }

    /// Index a single file.
    pub async fn index_file(&mut self, path: &Path) -> Result<usize> {
        let (chunks, hash) = self.process_file(path).await?;
        let chunk_count = chunks.len();

        let chunks_with_hash: Vec<(Chunk, String)> =
            chunks.into_iter().map(|c| (c, hash.clone())).collect();

        let mut stats = IndexStats::default();
        self.embed_and_upsert(&chunks_with_hash, &mut stats).await?;

        Ok(chunk_count)
    }

    /// Collect all files to index.
    fn collect_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let walker = WalkBuilder::new(&self.config.root_path)
            .hidden(true)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Check extension
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if self.config.extensions.contains(&ext.to_string()) {
                    // Skip common build/vendor directories
                    let path_str = path.to_string_lossy();
                    if !path_str.contains("/target/")
                        && !path_str.contains("/node_modules/")
                        && !path_str.contains("/.git/")
                        && !path_str.contains("/vendor/")
                        && !path_str.contains("/__pycache__/")
                    {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        Ok(files)
    }

    /// Process a single file: chunk it and compute hash.
    async fn process_file(&mut self, path: &Path) -> Result<(Vec<Chunk>, String)> {
        let hash = Self::compute_file_hash(path)?;
        let chunks = self.chunker.chunk_file(path)?;

        Ok((chunks, hash))
    }

    /// Compute SHA256 hash of a file.
    fn compute_file_hash(path: &Path) -> Result<String> {
        let content = std::fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Generate embeddings for chunks and upsert to Qdrant.
    async fn embed_and_upsert(
        &self,
        chunks: &[(Chunk, String)],
        stats: &mut IndexStats,
    ) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let batch_size = self.embeddings.max_batch_size();

        // Group chunks by file for manifest updates
        let mut file_chunks: std::collections::HashMap<PathBuf, Vec<String>> =
            std::collections::HashMap::new();

        for batch in chunks.chunks(batch_size) {
            // Prepare texts for embedding
            let texts: Vec<String> = batch
                .iter()
                .map(|(chunk, _)| chunk.enriched_content.clone())
                .collect();

            // Generate embeddings
            let embeddings = self.embeddings.embed_batch(&texts).await?;
            stats.embedding_calls += 1;

            // Create points for Qdrant
            let mut points = Vec::new();
            let mut bm25_index = self.bm25_index.write().await;

            for ((chunk, hash), embedding) in batch.iter().zip(embeddings.into_iter()) {
                let id = Uuid::new_v4().to_string();

                let payload = PointPayload {
                    file_path: chunk.file_path.clone(),
                    chunk_type: chunk.metadata.chunk_type.as_str().to_string(),
                    name: chunk.metadata.name.clone(),
                    signature: chunk.metadata.signature.clone(),
                    line_start: chunk.metadata.line_start,
                    line_end: chunk.metadata.line_end,
                    module: chunk.metadata.module.clone(),
                    scope: chunk.metadata.scope.clone(),
                    code: chunk.content.clone(),
                };

                points.push(Point {
                    id: id.clone(),
                    vector: embedding,
                    payload,
                });

                // Add to BM25 index
                bm25_index.add_document(id.clone(), chunk.enriched_content.clone());

                // Track chunk IDs per file
                let file_path = PathBuf::from(&chunk.file_path);
                file_chunks.entry(file_path).or_default().push(id);

                // Store hash for manifest update
                let file_path = PathBuf::from(&chunk.file_path);
                if !file_chunks.contains_key(&file_path) {
                    // First chunk for this file, store the hash
                    let mut manifest = self.manifest.write().await;
                    manifest.record_indexed(file_path.clone(), hash.clone(), vec![]);
                }
            }

            // Upsert to Qdrant
            self.qdrant.upsert_points(points).await?;
        }

        // Update manifest with all chunk IDs
        let mut manifest = self.manifest.write().await;
        for (file_path, chunk_ids) in file_chunks {
            if let Some(entry) = manifest.files.get_mut(&file_path) {
                entry.chunk_ids = chunk_ids;
                entry.chunk_count = entry.chunk_ids.len();
            }
        }

        Ok(())
    }

    /// Remove a file from the index.
    pub async fn remove_file(&self, path: &Path) -> Result<()> {
        debug!("Removing file from index: {:?}", path);

        let file_state = {
            let mut manifest = self.manifest.write().await;
            manifest.remove_file(path)
        };

        if let Some(state) = file_state {
            // Remove from Qdrant
            self.qdrant.delete_points(state.chunk_ids.clone()).await?;

            // Remove from BM25
            let mut bm25 = self.bm25_index.write().await;
            for id in state.chunk_ids {
                bm25.remove_document(&id);
            }
        }

        Ok(())
    }

    /// Get the current index manifest.
    pub async fn manifest(&self) -> IndexManifest {
        self.manifest.read().await.clone()
    }

    /// Get the BM25 index.
    pub fn bm25_index(&self) -> &Arc<RwLock<BM25Index>> {
        &self.bm25_index
    }

    /// Get index statistics.
    pub async fn get_stats(&self) -> IndexStats {
        let manifest = self.manifest.read().await;

        let chunks_count: usize = manifest.files.values().map(|e| e.chunk_ids.len()).sum();

        IndexStats {
            files_processed: manifest.files.len(),
            chunks_created: chunks_count,
            chunks_updated: 0,
            chunks_deleted: 0,
            files_skipped: 0,
            duration_ms: 0,
            embedding_calls: 0,
        }
    }

    /// Get the indexer configuration.
    pub fn config(&self) -> &IndexerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexer_config_default() {
        let config = IndexerConfig::default();
        assert_eq!(config.root_path, PathBuf::from("."));
        assert_eq!(config.collection_name, crate::DEFAULT_COLLECTION);
        assert_eq!(config.embedding_batch_size, 32);
        assert!(config.respect_gitignore);
        assert_eq!(config.extensions.len(), 5);
    }

    #[test]
    fn test_index_stats_default() {
        let stats = IndexStats::default();
        assert_eq!(stats.files_processed, 0);
        assert_eq!(stats.chunks_created, 0);
        assert_eq!(stats.chunks_updated, 0);
        assert_eq!(stats.chunks_deleted, 0);
        assert_eq!(stats.files_skipped, 0);
        assert_eq!(stats.duration_ms, 0);
        assert_eq!(stats.embedding_calls, 0);
    }

    #[test]
    fn test_compute_file_hash() {
        // Create a temp file to hash
        use std::io::Write;
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        writeln!(temp, "test content").unwrap();

        let hash = Indexer::<MockEmbeddingProvider>::compute_file_hash(temp.path()).unwrap();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex is 64 chars
    }

    // Mock embedding provider for tests
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; 4096])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; 4096]).collect())
        }

        fn dimensions(&self) -> usize {
            4096
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }
}
