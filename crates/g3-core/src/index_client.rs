//! Index client for managing codebase semantic search.
//!
//! This module provides a high-level client for codebase indexing
//! that wraps the g3-index library types and handles:
//! - Client initialization from config
//! - Connection to Qdrant
//! - State persistence (manifest + BM25 to .g3-index/)
//! - API key environment variable resolution

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use g3_config::IndexConfig;
use g3_index::{
    embeddings::OpenRouterEmbeddings,
    indexer::{Indexer, IndexerConfig, IndexStats},
    manifest::IndexManifest,
    qdrant::{QdrantClient, QdrantConfig, SearchFilter},
    search::{BM25Index, HybridSearcher, SearchConfig, SearchResult},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Resolve an optional API key value, expanding ${ENV_VAR} syntax.
/// Returns None if not configured, Some(key) if configured and resolved.
fn resolve_api_key_optional(api_key: &Option<String>) -> Option<String> {
    match api_key {
        Some(key) if key.starts_with("${") && key.ends_with("}") => {
            let var_name = &key[2..key.len() - 1];
            std::env::var(var_name).ok()
        }
        Some(key) if !key.is_empty() => Some(key.clone()),
        _ => None,
    }
}

/// State directory name within the working directory
const STATE_DIR_NAME: &str = ".g3-index";

/// Manifest file name
const MANIFEST_FILE: &str = "manifest.json";

/// BM25 index file name
const BM25_FILE: &str = "bm25_index.json";

/// Client for codebase indexing and semantic search.
///
/// Wraps the g3-index library types and provides a high-level API
/// for indexing and searching a codebase.
pub struct IndexClient {
    /// The indexer instance
    indexer: Arc<RwLock<Indexer<OpenRouterEmbeddings>>>,
    /// The hybrid searcher instance
    searcher: HybridSearcher<OpenRouterEmbeddings>,
    /// State directory for persistence
    state_dir: PathBuf,
    /// Working directory being indexed
    working_dir: PathBuf,
}

impl IndexClient {
    /// Create a new IndexClient from configuration.
    ///
    /// This initializes the embedding provider, connects to Qdrant,
    /// and loads any existing state from disk.
    ///
    /// # Arguments
    /// * `config` - Index configuration from g3-config
    /// * `working_dir` - The directory to index
    pub async fn new(config: &IndexConfig, working_dir: &Path) -> Result<Self> {
        info!("Initializing IndexClient for {:?}", working_dir);

        // Resolve API key
        let api_key = resolve_api_key(&config.embeddings.api_key)
            .context("Failed to resolve embedding API key")?;

        // Create embeddings provider
        let embeddings = Arc::new(OpenRouterEmbeddings::new(
            api_key,
            Some(config.embeddings.model.clone()),
            Some(config.embeddings.dimensions),
        ));

        // Create Qdrant config and connect
        let qdrant_config = QdrantConfig {
            url: config.qdrant_url.clone(),
            api_key: resolve_api_key_optional(&config.qdrant_api_key),
            collection_name: config.collection_name.clone(),
            dimensions: config.embeddings.dimensions,
        };

// Connect to Qdrant with retry logic (create two clients - one for indexer, one for searcher)
        // Retry with exponential backoff: 100ms, 200ms, 400ms (3 attempts total)
        const MAX_RETRIES: u32 = 3;
        const INITIAL_DELAY_MS: u64 = 100;

        let qdrant_for_indexer = {
            let mut last_error = None;
            let client = 'outer: {
                for attempt in 1..=MAX_RETRIES {
                    match QdrantClient::from_config(&qdrant_config).await {
                        Ok(c) => {
                            info!("Connected to Qdrant for indexer on attempt {}/{}", attempt, MAX_RETRIES);
                            break 'outer c;
                        }
                        Err(e) => {
                            last_error = Some(format!("{}", e));
                            if attempt < MAX_RETRIES {
                                let delay_ms = INITIAL_DELAY_MS * (1 << (attempt - 1)); // 100, 200, 400
                                warn!(
                                    "Qdrant connection attempt {}/{} failed (retrying in {}ms): {}",
                                    attempt, MAX_RETRIES, delay_ms, e
                                );
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!("Qdrant connection failed after {} attempts: {}", MAX_RETRIES, last_error.unwrap()));
            };
            client
        };

        let qdrant_for_searcher = {
            let mut last_error = None;
            let client = 'outer: {
                for attempt in 1..=MAX_RETRIES {
                    match QdrantClient::from_config(&qdrant_config).await {
                        Ok(c) => {
                            info!("Connected to Qdrant for searcher on attempt {}/{}", attempt, MAX_RETRIES);
                            break 'outer c;
                        }
                        Err(e) => {
                            last_error = Some(format!("{}", e));
                            if attempt < MAX_RETRIES {
                                let delay_ms = INITIAL_DELAY_MS * (1 << (attempt - 1)); // 100, 200, 400
                                warn!(
                                    "Qdrant connection attempt {}/{} failed (retrying in {}ms): {}",
                                    attempt, MAX_RETRIES, delay_ms, e
                                );
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            }
                        }
                    }
                }
                return Err(anyhow::anyhow!("Qdrant connection failed after {} attempts: {}", MAX_RETRIES, last_error.unwrap()));
            };
            client
        };

        // Set up state directory
        let state_dir = working_dir.join(STATE_DIR_NAME);
        if !state_dir.exists() {
            std::fs::create_dir_all(&state_dir)
                .context("Failed to create state directory")?;
            debug!("Created state directory: {:?}", state_dir);
        }

        // Load existing state or create new
        let manifest_path = state_dir.join(MANIFEST_FILE);
        let bm25_path = state_dir.join(BM25_FILE);

        let manifest = if manifest_path.exists() {
            match IndexManifest::load(&manifest_path) {
                Ok(m) => {
                    info!("Loaded existing manifest with {} files", m.files.len());
                    m
                }
                Err(e) => {
                    warn!("Failed to load manifest, starting fresh: {}", e);
                    IndexManifest::new()
                }
            }
        } else {
            debug!("No existing manifest found, starting fresh");
            IndexManifest::new()
        };

        let bm25_index = if bm25_path.exists() {
            match BM25Index::load(&bm25_path) {
                Ok(idx) => {
                    info!("Loaded existing BM25 index with {} documents", idx.len());
                    idx
                }
                Err(e) => {
                    warn!("Failed to load BM25 index, starting fresh: {}", e);
                    BM25Index::new()
                }
            }
        } else {
            debug!("No existing BM25 index found, starting fresh");
            BM25Index::new()
        };

        // Convert language names to extensions
        let extensions: Vec<String> = config
            .chunking
            .languages
            .iter()
            .flat_map(|lang| language_to_extensions(lang))
            .collect();

        // Create indexer config
        let indexer_config = IndexerConfig {
            root_path: working_dir.to_path_buf(),
            collection_name: config.collection_name.clone(),
            embedding_batch_size: 32,
            respect_gitignore: true,
            extensions,
            max_chunk_tokens: config.chunking.max_chunk_tokens,
            include_context: config.chunking.include_context,
        };

        // Create indexer with existing state
        let indexer = Indexer::with_state(
            indexer_config,
            embeddings.clone(),
            qdrant_for_indexer,
            manifest,
            bm25_index,
        )
        .context("Failed to create indexer")?;

        // Create search config
        let search_config = SearchConfig {
            limit: 10, // Default, can be overridden per search
            vector_weight: config.search.vector_weight,
            bm25_weight: config.search.bm25_weight,
            min_score: 0.0,
            hybrid: config.search.hybrid,
            rrf_k: 60.0,
        };

        // Create searcher sharing the BM25 index with indexer
        let searcher = HybridSearcher::new(
            search_config,
            embeddings,
            qdrant_for_searcher,
            indexer.bm25_index().clone(),
        );

        Ok(Self {
            indexer: Arc::new(RwLock::new(indexer)),
            searcher,
            state_dir,
            working_dir: working_dir.to_path_buf(),
        })
    }

    /// Index the codebase.
    ///
    /// # Arguments
    /// * `force` - If true, rebuild the entire index from scratch.
    ///             If false, perform incremental indexing.
    ///
    /// # Returns
    /// Statistics about the indexing operation.
    pub async fn index(&self, force: bool) -> Result<IndexStats> {
        info!(
            "Indexing codebase at {:?} (force={})",
            self.working_dir, force
        );

        let stats = {
            let mut indexer = self.indexer.write().await;
            if force {
                indexer.index_all(true).await?
            } else {
                indexer.index_incremental().await?
            }
        };

        // Save state after indexing
        self.save_state().await?;

        info!(
            "Indexing complete: {} files, {} chunks in {}ms",
            stats.files_processed, stats.chunks_created, stats.duration_ms
        );

        Ok(stats)
    }

    /// Search the codebase.
    ///
    /// # Arguments
    /// * `query` - Natural language or code query
    /// * `limit` - Maximum number of results
    /// * `file_filter` - Optional file path prefix filter
    ///
    /// # Returns
    /// A vector of search results sorted by relevance.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        file_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        debug!(
            "Searching for '{}' (limit={}, filter={:?})",
            query, limit, file_filter
        );

        // Build filter if provided
        let filter = file_filter.map(|prefix| {
            SearchFilter::new().with_path_prefix(prefix.to_string())
        });

        // Create a new searcher with the specified limit
        // Note: We can't easily change the limit on the existing searcher,
        // so we use the existing one and let the search method handle it
        let results = self.searcher.search(query, filter).await?;

        // Truncate to requested limit (searcher may return more for RRF fusion)
        let results: Vec<SearchResult> = results.into_iter().take(limit).collect();

        debug!("Found {} search results", results.len());
        Ok(results)
    }

    /// Get current index statistics.
    pub async fn get_stats(&self) -> IndexStats {
        let indexer = self.indexer.read().await;
        indexer.get_stats().await
    }

    /// Get the working directory being indexed.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Save manifest and BM25 index to disk.
    async fn save_state(&self) -> Result<()> {
        let indexer = self.indexer.read().await;

        // Save manifest
        let manifest = indexer.manifest().await;
        let manifest_path = self.state_dir.join(MANIFEST_FILE);
        manifest
            .save(&manifest_path)
            .context("Failed to save manifest")?;
        debug!("Saved manifest to {:?}", manifest_path);

        // Save BM25 index
        let bm25 = indexer.bm25_index().read().await;
        let bm25_path = self.state_dir.join(BM25_FILE);
        bm25.save(&bm25_path)
            .context("Failed to save BM25 index")?;
        debug!("Saved BM25 index to {:?}", bm25_path);

        Ok(())
    }

    // ========================================================================
    // Knowledge Graph Queries
    // ========================================================================

    /// Check if the knowledge graph is enabled.
    pub async fn has_graph(&self) -> bool {
        let indexer = self.indexer.read().await;
        indexer.has_graph()
    }

    /// Find all symbols with a given name.
    ///
    /// Returns symbols that match the exact name across all files.
    pub async fn find_symbols_by_name(&self, name: &str) -> Result<Vec<SymbolInfo>> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(Vec::new());
        };

        let gb_read = gb.read().await;
        let symbols: Vec<SymbolInfo> = gb_read
            .find_symbols_by_name(name)
            .into_iter()
            .map(SymbolInfo::from)
            .collect();

        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// Get all symbols in a file.
    pub async fn get_file_symbols(&self, file_path: &str) -> Result<Vec<SymbolInfo>> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(Vec::new());
        };

        let gb_read = gb.read().await;
        let symbols: Vec<SymbolInfo> = gb_read
            .symbols_in_file(file_path)
            .into_iter()
            .map(SymbolInfo::from)
            .collect();

        debug!("Found {} symbols in file '{}'", symbols.len(), file_path);
        Ok(symbols)
    }

    /// Find all callers of a symbol.
    ///
    /// Returns the IDs of symbols that call the given symbol.
    pub async fn find_callers(&self, symbol_id: &str) -> Result<Vec<String>> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(Vec::new());
        };

        let gb_read = gb.read().await;
        let callers = gb_read.find_callers(symbol_id);

        debug!("Found {} callers for symbol '{}'", callers.len(), symbol_id);
        Ok(callers)
    }

    /// Find all callees of a symbol.
    ///
    /// Returns the IDs of symbols called by the given symbol.
    pub async fn find_callees(&self, symbol_id: &str) -> Result<Vec<String>> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(Vec::new());
        };

        let gb_read = gb.read().await;
        let callees = gb_read.find_callees(symbol_id);

        debug!("Found {} callees for symbol '{}'", callees.len(), symbol_id);
        Ok(callees)
    }

    /// Find all references to a symbol.
    pub async fn find_references(&self, symbol_id: &str) -> Result<Vec<ReferenceInfo>> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(Vec::new());
        };

        let gb_read = gb.read().await;
        let refs: Vec<ReferenceInfo> = gb_read
            .find_references(symbol_id)
            .into_iter()
            .map(ReferenceInfo::from)
            .collect();

        debug!(
            "Found {} references for symbol '{}'",
            refs.len(),
            symbol_id
        );
        Ok(refs)
    }

    /// Get knowledge graph statistics.
    pub async fn get_graph_stats(&self) -> Result<GraphStats> {
        let indexer = self.indexer.read().await;
        let Some(gb) = indexer.graph_builder() else {
            return Ok(GraphStats::default());
        };

        let gb_read = gb.read().await;
        Ok(GraphStats {
            symbol_count: gb_read.symbol_count(),
            file_count: gb_read.file_count(),
        })
    }
}

/// Information about a code symbol.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_id: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
}

impl From<&g3_index::SymbolNode> for SymbolInfo {
    fn from(node: &g3_index::SymbolNode) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            kind: node.kind.label().to_string(),
            file_id: node.file_id.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            signature: node.signature.clone(),
        }
    }
}

/// Information about a symbol reference.
#[derive(Debug, Clone)]
pub struct ReferenceInfo {
    pub source: String,
    pub target: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

impl From<g3_index::Edge> for ReferenceInfo {
    fn from(edge: g3_index::Edge) -> Self {
        Self {
            source: edge.source,
            target: edge.target,
            file: edge.location_file,
            line: edge.location_line,
        }
    }
}

/// Knowledge graph statistics.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    pub symbol_count: usize,
    pub file_count: usize,
}

/// Resolve an API key value, expanding ${ENV_VAR} syntax.
///
/// # Arguments
/// * `api_key` - Optional API key value from config
///
/// # Returns
/// The resolved API key string
fn resolve_api_key(api_key: &Option<String>) -> Result<String> {
    match api_key {
        Some(key) if key.starts_with("${") && key.ends_with("}") => {
            // Extract environment variable name
            let var_name = &key[2..key.len() - 1];
            std::env::var(var_name).with_context(|| {
                format!(
                    "Environment variable '{}' not set (from config value '{}')",
                    var_name, key
                )
            })
        }
        Some(key) if !key.is_empty() => Ok(key.clone()),
        _ => {
            // Try common environment variables as fallback
            if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
                return Ok(key);
            }
            anyhow::bail!(
                "No API key configured. Set index.embeddings.api_key in config or OPENROUTER_API_KEY environment variable"
            )
        }
    }
}

/// Convert a language name to file extensions.
///
/// # Arguments
/// * `language` - Language name (e.g., "rust", "python")
///
/// # Returns
/// A vector of file extensions for that language
fn language_to_extensions(language: &str) -> Vec<String> {
    match language.to_lowercase().as_str() {
        "rust" => vec!["rs".to_string()],
        "python" => vec!["py".to_string()],
        "javascript" => vec!["js".to_string(), "jsx".to_string()],
        "typescript" => vec!["ts".to_string(), "tsx".to_string()],
        "go" => vec!["go".to_string()],
        "java" => vec!["java".to_string()],
        "c" => vec!["c".to_string(), "h".to_string()],
        "cpp" | "c++" => vec!["cpp".to_string(), "hpp".to_string(), "cc".to_string(), "hh".to_string()],
        "ruby" => vec!["rb".to_string()],
        "php" => vec!["php".to_string()],
        "swift" => vec!["swift".to_string()],
        "kotlin" => vec!["kt".to_string(), "kts".to_string()],
        "scala" => vec!["scala".to_string()],
        _ => {
            warn!("Unknown language '{}', using as extension", language);
            vec![language.to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_api_key_direct() {
        let key = resolve_api_key(&Some("direct-key".to_string()));
        assert!(key.is_ok());
        assert_eq!(key.unwrap(), "direct-key");
    }

    #[test]
    fn test_resolve_api_key_env_var() {
        // SAFETY: This is a test that sets a unique env var, no other threads use it
        unsafe {
            std::env::set_var("TEST_API_KEY_12345", "from-env");
        }
        let key = resolve_api_key(&Some("${TEST_API_KEY_12345}".to_string()));
        assert!(key.is_ok());
        assert_eq!(key.unwrap(), "from-env");
        // SAFETY: Cleanup of test-only env var
        unsafe {
            std::env::remove_var("TEST_API_KEY_12345");
        }
    }

    #[test]
    fn test_resolve_api_key_missing_env() {
        let key = resolve_api_key(&Some("${NONEXISTENT_VAR_XYZ}".to_string()));
        assert!(key.is_err());
    }

    #[test]
    fn test_resolve_api_key_empty() {
        // With no OPENROUTER_API_KEY set, this should fail
        // SAFETY: This is a test that temporarily removes an env var
        unsafe {
            std::env::remove_var("OPENROUTER_API_KEY");
        }
        let key = resolve_api_key(&Some("".to_string()));
        assert!(key.is_err());
    }

    #[test]
    fn test_language_to_extensions_rust() {
        let exts = language_to_extensions("rust");
        assert_eq!(exts, vec!["rs".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_python() {
        let exts = language_to_extensions("python");
        assert_eq!(exts, vec!["py".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_javascript() {
        let exts = language_to_extensions("javascript");
        assert_eq!(exts, vec!["js".to_string(), "jsx".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_typescript() {
        let exts = language_to_extensions("typescript");
        assert_eq!(exts, vec!["ts".to_string(), "tsx".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_go() {
        let exts = language_to_extensions("go");
        assert_eq!(exts, vec!["go".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_case_insensitive() {
        let exts = language_to_extensions("RUST");
        assert_eq!(exts, vec!["rs".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_unknown() {
        let exts = language_to_extensions("xyz");
        assert_eq!(exts, vec!["xyz".to_string()]);
    }

    #[test]
    fn test_language_to_extensions_cpp() {
        let exts = language_to_extensions("cpp");
        assert!(exts.contains(&"cpp".to_string()));
        assert!(exts.contains(&"hpp".to_string()));
    }

    /// Integration test that requires Qdrant running and OPENROUTER_API_KEY set.
    /// Run with: cargo test -p g3-core test_index_client_integration -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_index_client_integration() {
        use g3_config::IndexConfig;
        use std::path::Path;

        // Create a minimal config
        let config = IndexConfig {
            enabled: true,
            qdrant_url: "http://localhost:6334".to_string(),
            qdrant_api_key: None,
            collection_name: "g3-test-collection".to_string(),
            embeddings: g3_config::EmbeddingsConfig {
                provider: "openrouter".to_string(),
                api_key: Some("${OPENROUTER_API_KEY}".to_string()),
                model: "qwen/qwen3-embedding-8b".to_string(),
                dimensions: 4096,
                base_url: None,
            },
            search: g3_config::SearchConfig::default(),
            chunking: g3_config::ChunkingConfig::default(),
            watcher: g3_config::WatcherConfig::default(),
        };

        // Try to create client
        let work_dir = Path::new(".");
        let client = IndexClient::new(&config, work_dir).await;

        assert!(client.is_ok(), "Failed to create IndexClient: {:?}", client.err());
        let client = client.unwrap();

        // Get stats (should work even with empty index)
        let stats = client.get_stats().await;
        println!("Stats: {} files, {} chunks", stats.files_processed, stats.chunks_created);

        // Try indexing (will index current directory)
        let result = client.index(false).await;
        assert!(result.is_ok(), "Indexing failed: {:?}", result.err());

        let stats = result.unwrap();
        println!("Indexed {} files, {} chunks in {}ms",
            stats.files_processed, stats.chunks_created, stats.duration_ms);

        // Try searching
        let results = client.search("IndexClient", 5, None).await;
        assert!(results.is_ok(), "Search failed: {:?}", results.err());

        let results = results.unwrap();
        println!("Found {} results for 'IndexClient'", results.len());
        for r in &results {
            println!("  - {} ({}:{}-{})", r.file_path, r.kind, r.start_line, r.end_line);
        }
    }
}
