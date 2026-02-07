//! Cross-index connector for linking LSP symbols with indexed chunks.
//!
//! This module provides integration between:
//! - LSP language server protocol results
//! - Vector index (Qdrant) semantic search
//! - Lexical index (BM25) keyword search
//! - Knowledge graph (CodeGraph) structure
//!
//! The `IndexConnector` bridges these layers to provide:
//! - LSP symbol locations linked to indexed chunks
//! - Enriched search results with cross-index data
//! - Unified result formatting across all sources

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::graph::{CodeGraph, EdgeKind};
use crate::unified_index::UnifiedSearchResult;

/// Result enrichment configuration.
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Whether to fetch symbol details from LSP
    pub fetch_lsp_details: bool,
    /// Whether to enrich with graph context
    pub fetch_graph_context: bool,
    /// Maximum depth for graph context traversal
    pub graph_context_depth: usize,
    /// Whether to include code snippet in enriched results
    pub include_code_snippet: bool,
    /// Maximum snippet length
    pub max_snippet_length: usize,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            fetch_lsp_details: true,
            fetch_graph_context: true,
            graph_context_depth: 2,
            include_code_snippet: true,
            max_snippet_length: 500,
        }
    }
}

impl EnrichmentConfig {
    /// Create a new enrichment configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable LSP details fetching.
    pub fn with_lsp_details(mut self, enabled: bool) -> Self {
        self.fetch_lsp_details = enabled;
        self
    }

    /// Enable or disable graph context fetching.
    pub fn with_graph_context(mut self, enabled: bool) -> Self {
        self.fetch_graph_context = enabled;
        self
    }

    /// Set graph context traversal depth.
    pub fn with_graph_depth(mut self, depth: usize) -> Self {
        self.graph_context_depth = depth;
        self
    }

    /// Enable or disable code snippet inclusion.
    pub fn with_snippet(mut self, enabled: bool, max_length: usize) -> Self {
        self.include_code_snippet = enabled;
        self.max_snippet_length = max_length;
        self
    }
}

/// connector between LSP results and indexed chunks.
#[derive(Debug, Clone)]
pub struct IndexConnector {
    /// Configuration for result enrichment
    config: EnrichmentConfig,
    /// Knowledge graph for context lookup
    graph: CodeGraph,
}

impl IndexConnector {
    /// Create a new index connector.
    ///
    /// # Arguments
    /// * `config` - Enrichment configuration
    /// * `graph` - Knowledge graph for context lookup
    pub fn new(config: EnrichmentConfig, graph: CodeGraph) -> Self {
        Self { config, graph }
    }

    /// Create a new index connector with default configuration.
    pub fn with_graph(graph: CodeGraph) -> Self {
        Self {
            config: EnrichmentConfig::default(),
            graph,
        }
    }

    /// Set the configuration.
    pub fn with_config(mut self, config: EnrichmentConfig) -> Self {
        self.config = config;
        self
    }

    /// Link an LSP symbol location to indexed chunks.
    ///
    /// # Arguments
    /// * `lsp_file` - File path from LSP result
    /// * `lsp_line` - Line number from LSP result
    /// * `chunks` - Available indexed chunks
    ///
    /// # Returns
    /// Optional chunk ID that contains this location.
    pub fn link_lsp_to_chunk(
        &self,
        lsp_file: &str,
        lsp_line: usize,
        chunks: &[crate::chunker::Chunk],
    ) -> Option<String> {
        debug!(
            file = lsp_file,
            line = lsp_line,
            "Linking LSP location to chunk"
        );

        for chunk in chunks {
            let metadata = &chunk.metadata;
            if chunk.file_path == lsp_file
                && metadata.line_start <= lsp_line
                && metadata.line_end >= lsp_line
            {
                return Some(format!("{}:{}-{}", chunk.file_path, metadata.line_start, metadata.line_end));
            }
        }

        None
    }

    /// Find chunks associated with a symbol.
    ///
    /// # Arguments
    /// * `symbol_name` - Name of the symbol
    /// * `file_path` - File path where symbol is defined
    ///
    /// # Returns
    /// Vector of chunk IDs containing this symbol.
    pub fn find_chunks_for_symbol(
        &self,
        symbol_name: &str,
        file_path: &str,
    ) -> Vec<String> {
        debug!(
            symbol = symbol_name,
            file = file_path,
            "Finding chunks for symbol"
        );

        // Find the symbol in the graph
        let symbol_nodes = self.graph.find_symbols_by_name(symbol_name);

        let mut chunk_ids = Vec::new();

        for symbol in symbol_nodes {
            if symbol.file_id == file_path {
                // For now, return the file path as chunk identifier
                // In a full implementation, this would map to actual chunk IDs
                chunk_ids.push(format!("{}::{}", symbol.file_id, symbol.id));
            }
        }

        chunk_ids
    }

    /// Enrich a unified search result with cross-index data.
    ///
    /// # Arguments
    /// * `result` - Original search result
    /// * `lsp_available` - Whether LSP data is available
    ///
    /// # Returns
    /// Enriched result with additional metadata.
    pub fn enrich_result(
        &self,
        mut result: UnifiedSearchResult,
        lsp_available: bool,
    ) -> UnifiedSearchResult {
        debug!(id = %result.id, "Enriching result");

        // Get graph context if enabled
        if self.config.fetch_graph_context {
            if let Some(file_node) = self.graph.get_file(&result.file_path) {
                // Get file-level metadata
                result.metadata.insert(
                    "file_symbol_count".to_string(),
                    serde_json::to_value(file_node.symbol_count).unwrap_or_default(),
                );
                result.metadata.insert(
                    "file_loc".to_string(),
                    serde_json::to_value(file_node.loc).unwrap_or_default(),
                );
            }

            // Try to find symbol info from the result's scope or name
            if let Some(name) = &result.name {
                let symbols = self.graph.find_symbols_by_name(name);
                if !symbols.is_empty() {
                    let symbol = &symbols[0];
                    result.metadata.insert(
                        "symbol_kind".to_string(),
                        serde_json::to_value(symbol.kind.label()).unwrap_or_default(),
                    );
                    result.metadata.insert(
                        "symbol_signature".to_string(),
                        serde_json::to_value(symbol.signature.as_deref().unwrap_or("")).unwrap_or_default(),
                    );
                    result.metadata.insert(
                        "symbol_module".to_string(),
                        serde_json::to_value(symbol.module_path.as_deref().unwrap_or("")).unwrap_or_default(),
                    );

                    // Get callers and callees
                    let callers = self.graph.find_callers(&symbol.id);
                    let callees = self.graph.find_callees(&symbol.id);

                    result.metadata.insert(
                        "symbol_callers_count".to_string(),
                        serde_json::to_value(callers.len()).unwrap_or_default(),
                    );
                    result.metadata.insert(
                        "symbol_callees_count".to_string(),
                        serde_json::to_value(callees.len()).unwrap_or_default(),
                    );
                }
            }
        }

        // Include code snippet if enabled
        if self.config.include_code_snippet && result.content.is_empty() && result.start_line > 0 {
            // In a full implementation, we'd fetch the actual code snippet
            // For now, mark that snippet is available
            result.metadata.insert(
                "snippet_available".to_string(),
                serde_json::to_value(true).unwrap_or_default(),
            );
        }

        // Mark if LSP data is available
        result.metadata.insert(
            "lsp_available".to_string(),
            serde_json::to_value(lsp_available).unwrap_or_default(),
        );

        result
    }

    /// Link an LSP result to chunks and enrich it.
    ///
    /// # Arguments
    /// * `lsp_file` - File path from LSP
    /// * `lsp_line` - Line number from LSP
    /// * `chunks` - Available indexed chunks
    /// * `original_result` - Original unified search result
    ///
    /// # Returns
    /// Enriched result with chunk linkage information.
    pub fn link_and_enrich(
        &self,
        lsp_file: &str,
        lsp_line: usize,
        chunks: &[crate::chunker::Chunk],
        mut original_result: UnifiedSearchResult,
    ) -> UnifiedSearchResult {
        debug!(
            file = lsp_file,
            line = lsp_line,
            "Linking and enriching result"
        );

        // Link to chunk
        if let Some(chunk_id) = self.link_lsp_to_chunk(lsp_file, lsp_line, chunks) {
            original_result.metadata.insert(
                "chunk_id".to_string(),
                serde_json::to_value(chunk_id).unwrap_or_default(),
            );
            original_result.metadata.insert(
                "lsp_line_linked".to_string(),
                serde_json::to_value(true).unwrap_or_default(),
            );
        }

        // Enrich with cross-index data
        let lsp_available = true; // LSP data was used to get here
        self.enrich_result(original_result, lsp_available)
    }

    /// Get callers of a symbol as enriched results.
    ///
    /// # Arguments
    /// * `symbol_name` - Name of the symbol
    /// * `symbol_file` - File path where symbol is defined
    /// * `max_results` - Maximum number of callers to return
    ///
    /// # Returns
    /// Vector of enriched results for callers.
    pub fn get_callers_as_results(
        &self,
        symbol_name: &str,
        symbol_file: &str,
        max_results: usize,
    ) -> Vec<UnifiedSearchResult> {
        debug!(
            symbol = symbol_name,
            file = symbol_file,
            "Getting callers as results"
        );

        let mut results = Vec::new();
        let symbols = self.graph.find_symbols_by_name(symbol_name);

        for symbol in symbols {
            if symbol.file_id != symbol_file {
                continue;
            }

            let callers = self.graph.find_callers(&symbol.id);

            for caller_id in callers.iter().take(max_results) {
                if let Some(caller) = self.graph.get_symbol(caller_id) {
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

        results
    }

    /// Get callees of a symbol as enriched results.
    ///
    /// # Arguments
    /// * `symbol_name` - Name of the symbol
    /// * `symbol_file` - File path where symbol is defined
    /// * `max_results` - Maximum number of callees to return
    ///
    /// # Returns
    /// Vector of enriched results for callees.
    pub fn get_callees_as_results(
        &self,
        symbol_name: &str,
        symbol_file: &str,
        max_results: usize,
    ) -> Vec<UnifiedSearchResult> {
        debug!(
            symbol = symbol_name,
            file = symbol_file,
            "Getting callees as results"
        );

        let mut results = Vec::new();
        let symbols = self.graph.find_symbols_by_name(symbol_name);

        for symbol in symbols {
            if symbol.file_id != symbol_file {
                continue;
            }

            let callees = self.graph.find_callees(&symbol.id);

            for callee_id in callees.iter().take(max_results) {
                if let Some(callee) = self.graph.get_symbol(callee_id) {
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

        results
    }

    /// Get the knowledge graph reference.
    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }

    /// Get the configuration.
    pub fn config(&self) -> &EnrichmentConfig {
        &self.config
    }
}

/// Query that executes across all indexes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossIndexQuery {
    /// Natural language query
    pub query: String,
    /// Search strategies to use
    pub strategies: Vec<CrossIndexStrategy>,
    /// Maximum results per strategy
    pub max_results: usize,
    /// Weight for each strategy in result fusion
    pub strategy_weights: HashMap<String, f32>,
}

/// Search strategy for cross-index query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrossIndexStrategy {
    /// LSP-based semantic search
    Lsp,
    /// Vector-based semantic search
    Semantic,
    /// BM25 lexical search
    Lexical,
    /// AST pattern matching
    Ast,
    /// Graph-based traversal
    Graph,
}

impl CrossIndexQuery {
    /// Create a new cross-index query.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            strategies: vec![
                CrossIndexStrategy::Semantic,
                CrossIndexStrategy::Lexical,
            ],
            max_results: 20,
            strategy_weights: HashMap::new(),
        }
    }

    /// Set search strategies.
    pub fn with_strategies(mut self, strategies: Vec<CrossIndexStrategy>) -> Self {
        self.strategies = strategies;
        self
    }

    /// Set maximum results.
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    /// Set strategy weights.
    pub fn with_weights(mut self, weights: HashMap<String, f32>) -> Self {
        self.strategy_weights = weights;
        self
    }

    /// Get the default weight for a strategy.
    pub fn get_strategy_weight(&self, strategy: &CrossIndexStrategy) -> f32 {
        let key = format!("{:?}", strategy).to_lowercase();
        *self.strategy_weights.get(&key).unwrap_or(&1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_config_default() {
        let config = EnrichmentConfig::default();
        assert!(config.fetch_lsp_details);
        assert!(config.fetch_graph_context);
        assert_eq!(config.graph_context_depth, 2);
        assert!(config.include_code_snippet);
        assert_eq!(config.max_snippet_length, 500);
    }

    #[test]
    fn test_enrichment_config_builder() {
        let config = EnrichmentConfig::new()
            .with_lsp_details(false)
            .with_graph_depth(5)
            .with_snippet(true, 1000);

        assert!(!config.fetch_lsp_details);
        assert_eq!(config.graph_context_depth, 5);
        assert!(config.include_code_snippet);
        assert_eq!(config.max_snippet_length, 1000);
    }

    #[test]
    fn test_index_connector_new() {
        let graph = CodeGraph::new();
        let connector = IndexConnector::new(EnrichmentConfig::default(), graph);

        assert_eq!(connector.graph().symbols.len(), 0);
        assert_eq!(connector.graph().files.len(), 0);
    }

    #[test]
    fn test_cross_index_query_new() {
        let query = CrossIndexQuery::new("find login function");

        assert_eq!(query.query, "find login function");
        assert_eq!(query.strategies.len(), 2);
        assert_eq!(query.max_results, 20);
    }

    #[test]
    fn test_cross_index_query_builder() {
        let query = CrossIndexQuery::new("test")
            .with_strategies(vec![
                CrossIndexStrategy::Semantic,
                CrossIndexStrategy::Graph,
            ])
            .with_max_results(50);

        assert_eq!(query.strategies.len(), 2);
        assert_eq!(query.max_results, 50);
    }

    #[test]
    fn test_cross_index_strategy_serialization() {
        let strategies = vec![
            CrossIndexStrategy::Lsp,
            CrossIndexStrategy::Semantic,
            CrossIndexStrategy::Lexical,
            CrossIndexStrategy::Ast,
            CrossIndexStrategy::Graph,
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let parsed: CrossIndexStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy, parsed);
        }
    }
}
