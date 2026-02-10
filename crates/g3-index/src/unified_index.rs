//! Unified Index API combining vector, lexical, and graph search.
//!
//! This module provides a single interface for codebase search capabilities:
//! - Semantic search via vector embeddings
//! - Lexical search via BM25
//! - AST-aware code search
//! - Knowledge graph queries (dependencies, callers, callees)
//!
//! # Architecture
//!
//! The UnifiedIndex provides a single interface for:
//!
//! - **Vector Layer** - Qdrant + Qwen3-Embedding-8B for semantic search
//! - **Lexical Layer** - BM25 index for keyword search
//! - **Graph Layer** - CodeGraph for dependency queries
//!
//! # Example
//!
//! ```ignore
//! use g3_index::{UnifiedIndex, QueryPlanner, CodeGraph, EmbeddingProvider};
//! use g3_index::search::{BM25Index, HybridSearcher};
//! use g3_index::qdrant::QdrantClient;
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! // Create components
//! let embeddings = MyEmbeddingProvider::new();
//! let qdrant = QdrantClient::connect("http://localhost:6334", "g3-codebase", 4096).await?;
//! let bm25_index = Arc::new(RwLock::new(BM25Index::new()));
//! let graph = CodeGraph::new();
//! let planner = QueryPlanner::new();
//!
//! // Create unified index
//! let unified = UnifiedIndex::new(embeddings, qdrant, bm25_index, graph, planner);
//!
//! // Search semantically
//! let results = unified.search_semantic("find login function", None).await?;
//!
//! // Query graph for dependencies
//! let callers = unified.query_graph("my_function", "callers", 2).await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

use crate::embeddings::EmbeddingProvider;
use crate::graph::{CodeGraph, EdgeKind};
use crate::search::{BM25Index, HybridSearcher, SearchConfig};
use crate::qdrant::QdrantClient;

/// Unified search result with common fields across all search types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSearchResult {
    /// Unique identifier for this result
    pub id: String,
    /// File path where the match was found
    pub file_path: String,
    /// Start line of the matched code (1-indexed)
    pub start_line: usize,
    /// End line of the matched code (1-indexed)
    pub end_line: usize,
    /// The matching code content (truncated for large results)
    pub content: String,
    /// Kind of code element (function, class, etc.)
    pub kind: String,
    /// Name of the code element (if available)
    pub name: Option<String>,
    /// Signature (if available)
    pub signature: Option<String>,
    /// Enclosing scope (e.g., "impl Foo")
    pub scope: Option<String>,
    /// Combined relevance score (higher is better)
    pub score: f32,
    /// Source of this result (semantic, lexical, ast, graph)
    pub source: UnifiedSearchSource,
    /// Additional metadata specific to the source
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Source of a unified search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnifiedSearchSource {
    /// Vector-based semantic search
    Semantic,
    /// BM25 keyword search
    Lexical,
    /// AST pattern matching
    Ast,
    /// Knowledge graph query
    Graph,
    /// LSP protocol query (requires external integration)
    Lsp,
}

impl UnifiedSearchResult {
    /// Create a result from vector search data.
    pub fn from_vector(
        id: impl Into<String>,
        file_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
        content: impl Into<String>,
        kind: impl Into<String>,
        name: Option<String>,
        signature: Option<String>,
        scope: Option<String>,
        score: f32,
    ) -> Self {
        Self {
            id: id.into(),
            file_path: file_path.into(),
            start_line,
            end_line,
            content: content.into(),
            kind: kind.into(),
            name,
            signature,
            scope,
            score,
            source: UnifiedSearchSource::Semantic,
            metadata: HashMap::new(),
        }
    }

    /// Create a result from BM25/lexical search data.
    pub fn from_lexical(
        id: impl Into<String>,
        file_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
        content: impl Into<String>,
        kind: impl Into<String>,
        name: Option<String>,
        signature: Option<String>,
        scope: Option<String>,
        score: f32,
    ) -> Self {
        Self {
            id: id.into(),
            file_path: file_path.into(),
            start_line,
            end_line,
            content: content.into(),
            kind: kind.into(),
            name,
            signature,
            scope,
            score,
            source: UnifiedSearchSource::Lexical,
            metadata: HashMap::new(),
        }
    }

    /// Create a result from AST pattern match.
    pub fn from_ast(
        id: impl Into<String>,
        file_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
        content: impl Into<String>,
        kind: impl Into<String>,
        name: Option<String>,
        signature: Option<String>,
        scope: Option<String>,
        score: f32,
    ) -> Self {
        Self {
            id: id.into(),
            file_path: file_path.into(),
            start_line,
            end_line,
            content: content.into(),
            kind: kind.into(),
            name,
            signature,
            scope,
            score,
            source: UnifiedSearchSource::Ast,
            metadata: HashMap::new(),
        }
    }

    /// Create a result from graph query.
    pub fn from_graph(
        id: impl Into<String>,
        file_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
        content: impl Into<String>,
        kind: impl Into<String>,
        name: Option<String>,
        signature: Option<String>,
        scope: Option<String>,
        score: f32,
        edge_kind: Option<EdgeKind>,
        source_symbol: Option<String>,
    ) -> Self {
        let mut metadata = HashMap::new();
        if let Some(kind) = edge_kind {
            metadata.insert(
                "edge_kind".to_string(),
                serde_json::to_value(format!("{:?}", kind)).unwrap_or_default(),
            );
        }
        if let Some(src) = source_symbol {
            metadata.insert("source_symbol".to_string(), serde_json::to_value(src).unwrap_or_default());
        }
        Self {
            id: id.into(),
            file_path: file_path.into(),
            start_line,
            end_line,
            content: content.into(),
            kind: kind.into(),
            name,
            signature,
            scope,
            score,
            source: UnifiedSearchSource::Graph,
            metadata,
        }
    }

    /// Truncate content to maximum length.
    pub fn with_truncated_content(mut self, max_length: usize) -> Self {
        if self.content.len() > max_length {
            let truncate_at = max_length.saturating_sub(3);
            // Safe UTF-8 truncation
            let chars: Vec<char> = self.content.chars().collect();
            if chars.len() > truncate_at {
                self.content = format!("{}...", chars.iter().take(truncate_at).collect::<String>());
            }
        }
        self
    }

    /// Get a display string for the source.
    pub fn source_label(&self) -> &'static str {
        match self.source {
            UnifiedSearchSource::Semantic => "semantic",
            UnifiedSearchSource::Lexical => "lexical",
            UnifiedSearchSource::Ast => "ast",
            UnifiedSearchSource::Graph => "graph",
            UnifiedSearchSource::Lsp => "lsp",
        }
    }
}

/// Query planner that selects optimal search strategy based on query characteristics.
#[derive(Debug, Clone)]
pub struct QueryPlanner {
    /// Default search configuration
    search_config: SearchConfig,
    /// Minimum score threshold for results
    min_score: f32,
    /// Maximum results to return from each search type
    max_results_per_source: usize,
    /// Whether to enable hybrid search (vector + lexical)
    hybrid_enabled: bool,
}

impl Default for QueryPlanner {
    fn default() -> Self {
        Self {
            search_config: SearchConfig::default(),
            min_score: 0.0,
            max_results_per_source: 20,
            hybrid_enabled: true,
        }
    }
}

impl QueryPlanner {
    /// Create a new query planner with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure minimum score threshold.
    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }

    /// Configure maximum results per source.
    pub fn with_max_results_per_source(mut self, max: usize) -> Self {
        self.max_results_per_source = max;
        self
    }

    /// Enable or disable hybrid search.
    pub fn with_hybrid_enabled(mut self, enabled: bool) -> Self {
        self.hybrid_enabled = enabled;
        self
    }

    /// Analyze a query and determine the optimal search strategy with adaptive weights.
    pub fn plan_query(&self, query: &str) -> QueryPlan {
        let query_lower = query.to_lowercase();

        // Check for graph-style queries (callers, callees, dependencies)
        if Self::is_graph_query(&query_lower) {
            return QueryPlan {
                strategy: QueryStrategy::GraphOnly,
                vector_weight: 0.0,
                bm25_weight: 0.0,
            };
        }

        // Check for AST pattern queries (code snippets, syntax patterns)
        if Self::is_ast_query(&query_lower) {
            return QueryPlan {
                strategy: QueryStrategy::AstOnly,
                vector_weight: 0.0,
                bm25_weight: 0.0,
            };
        }

        // Default to hybrid search with adaptive weights
        if self.hybrid_enabled {
            let (vector_weight, bm25_weight) = Self::classify_query_weights(query);
            QueryPlan {
                strategy: QueryStrategy::Hybrid,
                vector_weight,
                bm25_weight,
            }
        } else {
            QueryPlan {
                strategy: QueryStrategy::VectorOnly,
                vector_weight: 1.0,
                bm25_weight: 0.0,
            }
        }
    }

    /// Classify a query and return adaptive (vector_weight, bm25_weight) based on characteristics.
    ///
    /// - **Identifier-like** (contains `::`, `_` mid-word, camelCase, single token) → favor BM25
    /// - **Natural language** (multiple words, question words, conceptual) → favor vector
    /// - **Mixed/default** → balanced with slight vector preference
    fn classify_query_weights(query: &str) -> (f32, f32) {
        let trimmed = query.trim();

        // Identifier-like: contains ::, has snake_case, camelCase, or is a single token
        if Self::is_identifier_like(trimmed) {
            debug!(query = trimmed, "Query classified as identifier-like (bm25=0.7, vector=0.3)");
            return (0.3, 0.7);
        }

        // Natural language: multiple words with question/conceptual patterns
        if Self::is_natural_language(trimmed) {
            debug!(query = trimmed, "Query classified as natural language (bm25=0.2, vector=0.8)");
            return (0.8, 0.2);
        }

        // Default/mixed
        debug!(query = trimmed, "Query classified as mixed (bm25=0.3, vector=0.7)");
        (0.7, 0.3)
    }

    /// Check if query looks like a code identifier.
    fn is_identifier_like(query: &str) -> bool {
        // Contains path separator (e.g., "std::collections::HashMap")
        if query.contains("::") {
            return true;
        }

        // Single token (no spaces) with underscore in middle (snake_case)
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.len() == 1 {
            let word = words[0];
            // snake_case: has underscore not at start/end
            if word.contains('_') && !word.starts_with('_') && !word.ends_with('_') {
                return true;
            }
            // camelCase or PascalCase: has lowercase followed by uppercase
            let chars: Vec<char> = word.chars().collect();
            for i in 1..chars.len() {
                if chars[i - 1].is_lowercase() && chars[i].is_uppercase() {
                    return true;
                }
            }
            // Single identifier-like token (all alphanumeric, no spaces)
            if word.len() > 1 && word.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return true;
            }
        }

        // Two tokens where one contains :: or _ (e.g., "impl Foo_bar")
        if words.len() == 2 {
            if words.iter().any(|w| w.contains("::") || (w.contains('_') && w.len() > 2)) {
                return true;
            }
        }

        false
    }

    /// Check if query looks like natural language.
    fn is_natural_language(query: &str) -> bool {
        let lower = query.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // Must have multiple words
        if words.len() < 3 {
            return false;
        }

        // Question words or conceptual patterns
        let question_words = ["how", "what", "where", "find", "show", "list", "which", "why", "when", "explain", "describe"];
        if let Some(&first_word) = words.first() {
            if question_words.contains(&first_word) {
                return true;
            }
        }

        // Conceptual phrases with common natural language verbs/prepositions
        let nl_indicators = ["does", "the", "is", "are", "for", "with", "that", "this", "implement", "handle", "manage", "work"];
        let nl_count = words.iter().filter(|w| nl_indicators.contains(w)).count();
        if nl_count >= 2 {
            return true;
        }

        false
    }

    /// Check if query is likely a graph-style query.
    pub fn is_graph_query(query: &str) -> bool {
        let graph_keywords = ["depend", "caller", "callee", "depend on", "use by", "uses", "call chain", "call path"];
        graph_keywords.iter().any(|k| query.contains(k))
    }

    /// Check if query is likely an AST pattern query.
    pub fn is_ast_query(query: &str) -> bool {
        // Look for code-like patterns
        query.contains("fn ") || query.contains("func") || query.contains("class ") || query.contains("impl ") || query.contains("trait ")
    }

    /// Get search configuration.
    pub fn search_config(&self) -> &SearchConfig {
        &self.search_config
    }

    /// Get minimum score threshold.
    pub fn min_score(&self) -> f32 {
        self.min_score
    }

    /// Get maximum results per source.
    pub fn max_results_per_source(&self) -> usize {
        self.max_results_per_source
    }

    /// Check if hybrid search is enabled.
    pub fn hybrid_enabled(&self) -> bool {
        self.hybrid_enabled
    }
}

/// Search strategy type for query execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStrategy {
    /// Use graph-only search
    GraphOnly,
    /// Use AST-only search
    AstOnly,
    /// Use hybrid search (vector + lexical)
    Hybrid,
    /// Use vector-only search
    VectorOnly,
    /// Execute all search types and fuse results
    All,
}

/// Plan for executing a query, including strategy and adaptive weights.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// The search strategy to use
    pub strategy: QueryStrategy,
    /// Weight for vector similarity in RRF fusion (0.0 to 1.0)
    pub vector_weight: f32,
    /// Weight for BM25 keyword matching in RRF fusion (0.0 to 1.0)
    pub bm25_weight: f32,
}

impl QueryPlan {
    /// Returns true if this plan includes graph search.
    pub fn includes_graph(&self) -> bool {
        matches!(self.strategy, QueryStrategy::GraphOnly | QueryStrategy::All)
    }

    /// Returns true if this plan includes AST search.
    pub fn includes_ast(&self) -> bool {
        matches!(self.strategy, QueryStrategy::AstOnly | QueryStrategy::All)
    }

    /// Returns true if this plan includes semantic (vector) search.
    pub fn includes_semantic(&self) -> bool {
        matches!(self.strategy, QueryStrategy::Hybrid | QueryStrategy::VectorOnly | QueryStrategy::All)
    }

    /// Returns true if this plan includes lexical (BM25) search.
    pub fn includes_lexical(&self) -> bool {
        matches!(self.strategy, QueryStrategy::Hybrid | QueryStrategy::All)
    }
}

/// Unified index providing a single interface for all search capabilities.
pub struct UnifiedIndex {
    /// Hybrid searcher for vector + lexical search
    hybrid_searcher: HybridSearcher<dyn EmbeddingProvider + Send + Sync>,
    /// Knowledge graph for dependency and call hierarchy queries
    graph: CodeGraph,
    /// BM25 index for keyword search
    bm25_index: Arc<RwLock<BM25Index>>,
    /// Query planner for automatic strategy selection
    planner: QueryPlanner,
}

impl std::fmt::Debug for UnifiedIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedIndex")
            .field("planner", &self.planner)
            .field("symbol_count", &self.graph.symbols.len())
            .field("file_count", &self.graph.files.len())
            .finish()
    }
}

impl UnifiedIndex {
    /// Create a new unified index.
    ///
    /// # Arguments
    /// * `embeddings` - Embedding provider for semantic search
    /// * `qdrant` - Qdrant client for vector storage
    /// * `bm25_index` - BM25 index for lexical search
    /// * `graph` - Knowledge graph for dependency queries
    /// * `planner` - Query planner for automatic strategy selection
    pub fn new(
        embeddings: Arc<dyn EmbeddingProvider + Send + Sync>,
        qdrant: QdrantClient,
        bm25_index: Arc<RwLock<BM25Index>>,
        graph: CodeGraph,
        planner: QueryPlanner,
    ) -> Self {
        let hybrid_searcher = HybridSearcher::new_with_empty_bm25(
            planner.search_config().clone(),
            embeddings,
            qdrant,
        );
        Self {
            hybrid_searcher,
            graph,
            bm25_index,
            planner,
        }
    }

    /// Search for semantic matches to a query.
    ///
    /// # Arguments
    /// * `query` - Natural language or code query
    /// * `filter` - Optional filter conditions
    ///
    /// # Returns
    /// A vector of unified search results, sorted by relevance.
    pub async fn search_semantic(
        &self,
        query: &str,
        filter: Option<crate::qdrant::SearchFilter>,
    ) -> Result<Vec<UnifiedSearchResult>> {
        debug!(query, "Searching semantic");

        let results = self
            .hybrid_searcher
            .search(query, filter)
            .await?
            .into_iter()
            .map(|r| UnifiedSearchResult {
                id: r.id,
                file_path: r.file_path,
                start_line: r.start_line,
                end_line: r.end_line,
                content: r.content,
                kind: r.kind,
                name: r.name,
                signature: r.signature,
                scope: r.scope,
                score: r.score,
                source: UnifiedSearchSource::Semantic,
                metadata: {
                    let mut m = HashMap::new();
                    if let Some(vector_score) = r.vector_score {
                        m.insert("vector_score".to_string(), serde_json::to_value(vector_score).unwrap_or_default());
                    }
                    if let Some(bm25_score) = r.bm25_score {
                        m.insert("bm25_score".to_string(), serde_json::to_value(bm25_score).unwrap_or_default());
                    }
                    m
                },
            })
            .collect();

        Ok(results)
    }

    /// Search using BM25 lexical search.
    ///
    /// # Arguments
    /// * `query` - Keyword or phrase to search for
    ///
    /// # Returns
    /// A vector of unified search results, sorted by BM25 score.
    pub async fn search_lexical(&self, query: &str) -> Result<Vec<UnifiedSearchResult>> {
        debug!(query, "Searching lexical");

        let index = self.bm25_index.read().await;
        let bm25_results = index.search(query, self.planner.max_results_per_source());

        let results: Vec<UnifiedSearchResult> = bm25_results
            .into_iter()
            .enumerate()
            .map(|(rank, (id, score))| {
                // Get chunk metadata from the index
                let id_str = id.clone();
                UnifiedSearchResult {
                    id: id_str,
                    file_path: format!("chunk-{}", id),
                    start_line: 1,
                    end_line: 1,
                    content: String::new(),
                    kind: "chunk".to_string(),
                    name: None,
                    signature: None,
                    scope: None,
                    score: score as f32,
                    source: UnifiedSearchSource::Lexical,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("bm25_rank".to_string(), serde_json::to_value(rank).unwrap_or_default());
                        m
                    },
                }
            })
            .collect();

        Ok(results)
    }

    /// Search using AST pattern matching.
    ///
    /// # Arguments
    /// * `pattern` - Tree-sitter query pattern
    ///
    /// # Returns
    /// A vector of unified search results matching the pattern.
    pub async fn search_ast(&self, pattern: &str) -> Result<Vec<UnifiedSearchResult>> {
        debug!(pattern, "Searching AST patterns");

        // This would use tree-sitter to parse and match patterns
        // For now, return empty - actual implementation would depend on code_search crate
        Ok(Vec::new())
    }

    /// Query the knowledge graph for symbols and relationships.
    ///
    /// # Arguments
    /// * `symbol_name` - Name of the symbol to find
    /// * `query_type` - Type of query ("find", "callers", "callees", "references", "files", "types")
    /// * `depth` - Maximum depth for recursive queries (for traversals)
    ///
    /// # Returns
    /// A vector of unified search results.
    pub async fn query_graph(
        &self,
        symbol_name: &str,
        query_type: &str,
        depth: usize,
    ) -> Result<Vec<UnifiedSearchResult>> {
        debug!(
            symbol = symbol_name,
            query_type,
            depth,
            "Querying knowledge graph"
        );

        let mut results = Vec::new();

        // Find all symbols with this name
        let symbols = self.graph.find_symbols_by_name(symbol_name);

        match query_type {
            "find" => {
                for symbol in symbols {
                    results.push(UnifiedSearchResult::from_graph(
                        symbol.id.clone(),
                        symbol.file_id.clone(),
                        symbol.line_start,
                        symbol.line_end,
                        String::new(),
                        symbol.kind.label().to_string(),
                        Some(symbol.name.clone()),
                        None,
                        None,
                        1.0,
                        None,
                        None,
                    ));
                }
            }
            "callers" => {
                for symbol in symbols {
                    let callers = self.graph.find_callers(&symbol.id);
                    for caller_id in callers {
                        if let Some(caller) = self.graph.get_symbol(&caller_id) {
                            results.push(UnifiedSearchResult::from_graph(
                                format!("{}->{}", symbol.id, caller_id),
                                caller.file_id.clone(),
                                caller.line_start,
                                caller.line_end,
                                String::new(),
                                "caller".to_string(),
                                Some(caller.name.clone()),
                                None,
                                None,
                                0.9,
                                Some(EdgeKind::Calls),
                                Some(symbol.name.clone()),
                            ));
                        }
                    }
                }
            }
            "callees" => {
                for symbol in symbols {
                    let callees = self.graph.find_callees(&symbol.id);
                    for callee_id in callees {
                        if let Some(callee) = self.graph.get_symbol(&callee_id) {
                            results.push(UnifiedSearchResult::from_graph(
                                format!("{}->{}", symbol.id, callee_id),
                                callee.file_id.clone(),
                                callee.line_start,
                                callee.line_end,
                                String::new(),
                                "callee".to_string(),
                                Some(callee.name.clone()),
                                None,
                                None,
                                0.9,
                                Some(EdgeKind::Calls),
                                Some(symbol.name.clone()),
                            ));
                        }
                    }
                }
            }
            "references" => {
                for symbol in symbols {
                    let refs = self.graph.find_references(&symbol.id);
                    for edge in refs {
                        // Handle both symbol and file targets
                        let target_info = if edge.kind == EdgeKind::Defines {
                            // For Defines edges, source is a file
                            if let Some(file) = self.graph.get_file(&edge.source) {
                                (file.id.clone(), file.path.to_string_lossy().to_string(), "file".to_string())
                            } else {
                                continue;
                            }
                        } else {
                            // For other edges, source is typically a symbol
                            if let Some(symbol) = self.graph.get_symbol(&edge.source) {
                                (symbol.file_id.clone(), symbol.name.clone(), symbol.kind.label().to_string())
                            } else {
                                continue;
                            }
                        };

                        results.push(UnifiedSearchResult::from_graph(
                            edge.source.clone(),
                            target_info.0,
                            1, // Line info not available in edge
                            1,
                            String::new(),
                            format!("ref_{:?}", edge.kind),
                            Some(target_info.1),
                            None,
                            None,
                            0.8,
                            Some(edge.kind),
                            Some(symbol.name.clone()),
                        ));
                    }
                }
            }
            "files" => {
                // Find all files containing symbols with this name
                let mut seen_files = std::collections::HashSet::new();
                for symbol in symbols {
                    if seen_files.insert(&symbol.file_id) {
                        if let Some(file) = self.graph.get_file(&symbol.file_id) {
                            results.push(UnifiedSearchResult::from_graph(
                                symbol.file_id.clone(),
                                file.id.clone(),
                                1,
                                1,
                                String::new(),
                                "file".to_string(),
                                Some(file.path.to_string_lossy().to_string()),
                                None,
                                None,
                                1.0,
                                None,
                                None,
                            ));
                        }
                    }
                }
            }
            "types" => {
                // Find all types (structs, enums, traits, etc.) with this name
                let type_kinds = [
                    crate::graph::SymbolKind::Struct,
                    crate::graph::SymbolKind::Enum,
                    crate::graph::SymbolKind::Trait,
                    crate::graph::SymbolKind::Interface,
                    crate::graph::SymbolKind::TypeAlias,
                ];
                for symbol in symbols {
                    if type_kinds.contains(&symbol.kind) {
                        results.push(UnifiedSearchResult::from_graph(
                            symbol.id.clone(),
                            symbol.file_id.clone(),
                            symbol.line_start,
                            symbol.line_end,
                            String::new(),
                            symbol.kind.label().to_string(),
                            Some(symbol.name.clone()),
                            None,
                            None,
                            1.0,
                            None,
                            None,
                        ));
                    }
                }
            }
            "traverse" => {
                // BFS traversal up to depth
                let mut visited = std::collections::HashSet::new();
                let mut queue: Vec<(String, usize)> = symbols
                    .into_iter()
                    .map(|s| (s.id.clone(), 0))
                    .collect();

                while let Some((node_id, current_depth)) = queue.pop() {
                    if current_depth > depth {
                        continue;
                    }

                    if !visited.insert(node_id.clone()) {
                        continue;
                    }

                    // Get the node info - handle both SymbolNode and FileNode
                    let (file_path, name, kind) = if let Some(symbol) = self.graph.get_symbol(&node_id) {
                        (symbol.file_id.clone(), Some(symbol.name.clone()), symbol.kind.label().to_string())
                    } else if let Some(file) = self.graph.get_file(&node_id) {
                        (file.id.clone(), None, "file".to_string())
                    } else {
                        continue;
                    };

                    results.push(UnifiedSearchResult::from_graph(
                        node_id.clone(),
                        file_path,
                        1,
                        1,
                        String::new(),
                        kind,
                        name,
                        None,
                        None,
                        1.0 / (current_depth as f32 + 1.0), // Decay score with depth
                        None,
                        None,
                    ));

                    // Add neighbors to queue
                    let outgoing = self.graph.outgoing_edges(&node_id);
                    for edge in outgoing {
                        queue.push((edge.target.clone(), current_depth + 1));
                    }
                }
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown query type: {}. Supported: find, callers, callees, references, files, types, traverse",
                    query_type
                ));
            }
        }

        Ok(results)
    }

    /// Execute a unified search using the query planner.
    ///
    /// This method automatically selects the optimal search strategy
    /// based on the query characteristics.
    ///
    /// # Arguments
    /// * `query` - The search query
    ///
    /// # Returns
    /// A vector of unified search results, potentially from multiple sources.
    pub async fn unified_search(&self, query: &str) -> Result<Vec<UnifiedSearchResult>> {
        let plan = self.planner.plan_query(query);

        debug!(?plan, "Executing query with plan");

        let mut all_results = Vec::new();

        if plan.includes_semantic() {
            let semantic_results = self.search_semantic(query, None).await?;
            all_results.extend(semantic_results);
        }

        if plan.includes_lexical() {
            let lexical_results = self.search_lexical(query).await?;
            all_results.extend(lexical_results);
        }

        if plan.includes_ast() {
            let ast_results = self.search_ast(query).await?;
            all_results.extend(ast_results);
        }

        if plan.includes_graph() {
            // Graph queries require symbol name - try to extract from query
            // For now, return empty - would need more context
        }

        Ok(all_results)
    }

    /// Get the query planner.
    pub fn planner(&self) -> &QueryPlanner {
        &self.planner
    }

    /// Get a reference to the knowledge graph.
    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }

    /// Get a reference to the hybrid searcher.
    pub fn hybrid_searcher(&self) -> &HybridSearcher<dyn EmbeddingProvider + Send + Sync> {
        &self.hybrid_searcher
    }

    /// Get the number of symbols in the graph.
    pub fn symbol_count(&self) -> usize {
        self.graph.symbols.len()
    }

    /// Get the number of files in the graph.
    pub fn file_count(&self) -> usize {
        self.graph.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_planner_graph_detection() {
        assert!(QueryPlanner::is_graph_query("depend on foo"));
        assert!(QueryPlanner::is_graph_query("caller of foo"));
        assert!(QueryPlanner::is_graph_query("callees of bar"));
        assert!(QueryPlanner::is_graph_query("uses MyClass"));
    }

    #[test]
    fn test_query_planner_ast_detection() {
        assert!(QueryPlanner::is_ast_query("fn "));
        assert!(QueryPlanner::is_ast_query("func "));
        assert!(QueryPlanner::is_ast_query("class "));
        assert!(QueryPlanner::is_ast_query("impl "));
        assert!(QueryPlanner::is_ast_query("trait "));
    }

    #[test]
    fn test_query_planner_plan_query() {
        let planner = QueryPlanner::new();

        assert!(matches!(planner.plan_query("callers of foo").strategy, QueryStrategy::GraphOnly));
        assert!(matches!(planner.plan_query("fn hello() {}").strategy, QueryStrategy::AstOnly));
        assert!(matches!(planner.plan_query("find similar code").strategy, QueryStrategy::Hybrid));
    }

    #[test]
    fn test_query_classification_identifier() {
        let planner = QueryPlanner::new();

        // snake_case identifier
        let plan = planner.plan_query("parse_config");
        assert!((plan.bm25_weight - 0.7).abs() < f32::EPSILON);
        assert!((plan.vector_weight - 0.3).abs() < f32::EPSILON);

        // Path-style identifier
        let plan = planner.plan_query("std::collections::HashMap");
        assert!((plan.bm25_weight - 0.7).abs() < f32::EPSILON);

        // camelCase
        let plan = planner.plan_query("parseConfig");
        assert!((plan.bm25_weight - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_query_classification_natural_language() {
        let planner = QueryPlanner::new();

        // Question-style query
        let plan = planner.plan_query("how does authentication work in this codebase");
        assert!((plan.vector_weight - 0.8).abs() < f32::EPSILON);
        assert!((plan.bm25_weight - 0.2).abs() < f32::EPSILON);

        // Conceptual query with NL indicators
        let plan = planner.plan_query("where is the code that handles error logging");
        assert!((plan.vector_weight - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_query_classification_mixed() {
        let planner = QueryPlanner::new();

        // Short mixed query
        let plan = planner.plan_query("search results");
        assert!((plan.vector_weight - 0.7).abs() < f32::EPSILON);
        assert!((plan.bm25_weight - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_unified_search_result_sources() {
        let result = UnifiedSearchResult::from_vector(
            "test-1",
            "src/lib.rs",
            10,
            20,
            "fn test() {}",
            "function",
            Some("test".to_string()),
            Some("fn test()".to_string()),
            None,
            0.9,
        );

        assert_eq!(result.source, UnifiedSearchSource::Semantic);
        assert_eq!(result.source_label(), "semantic");
    }

    #[test]
    fn test_unified_search_result_truncation() {
        let long_content = "a".repeat(1000);
        let mut result = UnifiedSearchResult::from_vector(
            "test-1",
            "src/lib.rs",
            10,
            20,
            long_content,
            "function",
            None,
            None,
            None,
            0.9,
        );

        assert_eq!(result.content.len(), 1000);
        result = result.with_truncated_content(50);
        assert!(result.content.len() <= 50);
        assert!(result.content.ends_with("..."));
    }

    #[test]
    fn test_query_plan_includes_methods() {
        let plan = QueryPlan { strategy: QueryStrategy::All, vector_weight: 0.7, bm25_weight: 0.3 };

        assert!(plan.includes_graph());
        assert!(plan.includes_ast());
        assert!(plan.includes_semantic());
        assert!(plan.includes_lexical());

        let graph_plan = QueryPlan { strategy: QueryStrategy::GraphOnly, vector_weight: 0.0, bm25_weight: 0.0 };
        assert!(graph_plan.includes_graph());
        assert!(!graph_plan.includes_ast());
        assert!(!graph_plan.includes_semantic());
        assert!(!graph_plan.includes_lexical());
    }

    #[test]
    fn test_unified_search_result_from_graph() {
        let result = UnifiedSearchResult::from_graph(
            "test-1",
            "src/lib.rs",
            10,
            20,
            "fn test() {}",
            "function",
            Some("test".to_string()),
            None,
            None,
            0.8,
            Some(EdgeKind::Calls),
            Some("caller".to_string()),
        );

        assert_eq!(result.source, UnifiedSearchSource::Graph);
        assert!(result.metadata.contains_key("edge_kind"));
        assert!(result.metadata.contains_key("source_symbol"));
    }
}
