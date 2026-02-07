//! Hybrid search combining vector similarity and BM25.
//!
//! This module provides search functionality that combines
//! semantic vector search with keyword-based BM25 ranking
//! using Reciprocal Rank Fusion (RRF).

pub mod bm25;

pub use bm25::BM25Index;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::embeddings::EmbeddingProvider;
use crate::qdrant::{QdrantClient, SearchFilter, SearchHit};

/// A search result with relevance score and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique ID of the chunk
    pub id: String,
    /// File path where the match was found
    pub file_path: String,
    /// Start line of the matched code
    pub start_line: usize,
    /// End line of the matched code
    pub end_line: usize,
    /// The matching code content
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
    /// Vector similarity score component
    pub vector_score: Option<f32>,
    /// BM25 score component (if applicable)
    pub bm25_score: Option<f32>,
}

/// Configuration for hybrid search.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Maximum number of results to return
    pub limit: usize,
    /// Weight for vector similarity (0.0 to 1.0)
    pub vector_weight: f32,
    /// Weight for BM25 (0.0 to 1.0)
    pub bm25_weight: f32,
    /// Minimum score threshold
    pub min_score: f32,
    /// Enable hybrid search (vector + BM25)
    pub hybrid: bool,
    /// RRF k parameter (default 60)
    pub rrf_k: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            limit: 10,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            min_score: 0.0,
            hybrid: true,
            rrf_k: 60.0,
        }
    }
}

/// Reciprocal Rank Fusion (RRF) implementation.
///
/// Combines rankings from multiple sources into a single ranking.
/// RRF score = sum(1 / (k + rank_i)) for each ranking source
pub fn reciprocal_rank_fusion(
    vector_results: &[(String, f32)],  // (id, score)
    bm25_results: &[(String, f64)],     // (id, score)
    k: f32,
    vector_weight: f32,
    bm25_weight: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    // Add vector search contribution
    for (rank, (id, _original_score)) in vector_results.iter().enumerate() {
        let rrf_score = vector_weight / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_default() += rrf_score;
    }

    // Add BM25 contribution
    for (rank, (id, _original_score)) in bm25_results.iter().enumerate() {
        let rrf_score = bm25_weight / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_default() += rrf_score;
    }

    // Sort by combined score descending
    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    results
}

/// Hybrid searcher combining vector and BM25 search.
pub struct HybridSearcher<E: EmbeddingProvider + ?Sized> {
    config: SearchConfig,
    embeddings: Arc<E>,
    qdrant: QdrantClient,
    bm25_index: Arc<RwLock<BM25Index>>,
}

impl<E: EmbeddingProvider + ?Sized> HybridSearcher<E> {
    /// Create a new hybrid searcher.
    pub fn new(
        config: SearchConfig,
        embeddings: Arc<E>,
        qdrant: QdrantClient,
        bm25_index: Arc<RwLock<BM25Index>>,
    ) -> Self {
        Self {
            config,
            embeddings,
            qdrant,
            bm25_index,
        }
    }

    /// Create with a new empty BM25 index.
    pub fn new_with_empty_bm25(
        config: SearchConfig,
        embeddings: Arc<E>,
        qdrant: QdrantClient,
    ) -> Self {
        Self {
            config,
            embeddings,
            qdrant,
            bm25_index: Arc::new(RwLock::new(BM25Index::new())),
        }
    }

    /// Search for code similar to the query using hybrid search.
    ///
    /// # Arguments
    /// * `query` - Natural language or code query
    /// * `filter` - Optional filter conditions
    ///
    /// # Returns
    /// A vector of search results, sorted by relevance.
    pub async fn search(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
    ) -> Result<Vec<SearchResult>> {
        debug!("Searching for: {}", query);

        // Generate embedding for the query
        let query_vector = self.embeddings.embed(query).await?;

        // Fetch more results for fusion
        let fetch_limit = self.config.limit * 3;

        // Search Qdrant for vector similarity
        let vector_hits = self
            .qdrant
            .search(query_vector, fetch_limit, filter)
            .await?;

        // Convert to (id, score) pairs for RRF
        let vector_results: Vec<(String, f32)> = vector_hits
            .iter()
            .map(|hit| (hit.id.clone(), hit.score))
            .collect();

        // Build a map from ID to hit for later lookup
        let hits_map: HashMap<String, &SearchHit> = vector_hits
            .iter()
            .map(|hit| (hit.id.clone(), hit))
            .collect();

        // Get BM25 results if hybrid search is enabled
        let final_ranking = if self.config.hybrid && !self.bm25_index.read().await.is_empty() {
            let bm25_index = self.bm25_index.read().await;
            let bm25_results = bm25_index.search(query, fetch_limit);

            // Apply RRF fusion
            reciprocal_rank_fusion(
                &vector_results,
                &bm25_results,
                self.config.rrf_k,
                self.config.vector_weight,
                self.config.bm25_weight,
            )
        } else {
            // Vector-only search
            vector_results
        };

        // Convert ranking to SearchResults
        let mut results: Vec<SearchResult> = Vec::new();

        for (id, combined_score) in final_ranking.iter().take(self.config.limit) {
            if let Some(hit) = hits_map.get(id) {
                let result = SearchResult {
                    id: id.clone(),
                    file_path: hit.payload.file_path.clone(),
                    start_line: hit.payload.line_start,
                    end_line: hit.payload.line_end,
                    content: hit.payload.code.clone(),
                    kind: hit.payload.chunk_type.clone(),
                    name: if hit.payload.name.is_empty() {
                        None
                    } else {
                        Some(hit.payload.name.clone())
                    },
                    signature: hit.payload.signature.clone(),
                    scope: hit.payload.scope.clone(),
                    score: *combined_score,
                    vector_score: Some(hit.score),
                    bm25_score: None, // Could compute if needed
                };
                results.push(result);
            }
        }

        // Filter by minimum score
        results.retain(|r| r.score >= self.config.min_score);

        Ok(results)
    }

    /// Find code similar to the given code snippet.
    pub async fn find_similar(
        &self,
        code: &str,
        filter: Option<SearchFilter>,
    ) -> Result<Vec<SearchResult>> {
        self.search(code, filter).await
    }

    /// Add a document to the BM25 index.
    pub async fn add_to_bm25(&self, id: String, text: String) {
        let mut index = self.bm25_index.write().await;
        index.add_document(id, text);
    }

    /// Remove a document from the BM25 index.
    pub async fn remove_from_bm25(&self, id: &str) {
        let mut index = self.bm25_index.write().await;
        index.remove_document(id);
    }

    /// Get a reference to the BM25 index.
    pub fn bm25_index(&self) -> &Arc<RwLock<BM25Index>> {
        &self.bm25_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.limit, 10);
        assert!((config.vector_weight - 0.7).abs() < f32::EPSILON);
        assert!((config.bm25_weight - 0.3).abs() < f32::EPSILON);
        assert!(config.hybrid);
    }

    #[test]
    fn test_rrf_fusion() {
        // Vector results: doc1 (rank 0), doc2 (rank 1), doc3 (rank 2)
        let vector_results = vec![
            ("doc1".to_string(), 0.9f32),
            ("doc2".to_string(), 0.8f32),
            ("doc3".to_string(), 0.7f32),
        ];

        // BM25 results: doc2 (rank 0), doc1 (rank 1), doc4 (rank 2)
        let bm25_results = vec![
            ("doc2".to_string(), 5.0f64),
            ("doc1".to_string(), 4.0f64),
            ("doc4".to_string(), 3.0f64),
        ];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

        // doc1 and doc2 should be top since they appear in both
        assert!(!fused.is_empty());
        let top_two: Vec<&str> = fused.iter().take(2).map(|(id, _)| id.as_str()).collect();
        assert!(top_two.contains(&"doc1") || top_two.contains(&"doc2"));
    }

    #[test]
    fn test_rrf_single_source() {
        let vector_results = vec![
            ("doc1".to_string(), 0.9f32),
            ("doc2".to_string(), 0.8f32),
        ];
        let bm25_results: Vec<(String, f64)> = vec![];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, "doc1"); // Should maintain order
    }

    #[test]
    fn test_rrf_bm25_only() {
        let vector_results: Vec<(String, f32)> = vec![];
        let bm25_results = vec![
            ("doc1".to_string(), 5.0f64),
            ("doc2".to_string(), 4.0f64),
        ];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, "doc1"); // Should maintain BM25 order
    }

    #[test]
    fn test_rrf_empty_inputs() {
        let vector_results: Vec<(String, f32)> = vec![];
        let bm25_results: Vec<(String, f64)> = vec![];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_rrf_disjoint_results() {
        // Vector and BM25 return completely different documents
        let vector_results = vec![
            ("doc1".to_string(), 0.9f32),
            ("doc2".to_string(), 0.8f32),
        ];
        let bm25_results = vec![
            ("doc3".to_string(), 5.0f64),
            ("doc4".to_string(), 4.0f64),
        ];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

        // Should contain all 4 documents
        assert_eq!(fused.len(), 4);

        // Vector results should rank higher due to 0.7 weight vs 0.3
        let top_two: Vec<&str> = fused.iter().take(2).map(|(id, _)| id.as_str()).collect();
        assert!(top_two.contains(&"doc1"));
    }

    #[test]
    fn test_rrf_equal_weights() {
        let vector_results = vec![("doc1".to_string(), 0.9f32)];
        let bm25_results = vec![("doc2".to_string(), 5.0f64)];

        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.5, 0.5);

        // Both should have equal scores since they're at same rank with equal weights
        assert_eq!(fused.len(), 2);
        assert!((fused[0].1 - fused[1].1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rrf_k_parameter_effect() {
        let vector_results = vec![
            ("doc1".to_string(), 0.9f32),
            ("doc2".to_string(), 0.8f32),
        ];
        let bm25_results: Vec<(String, f64)> = vec![];

        // Lower k should give higher scores to top-ranked docs
        let fused_low_k = reciprocal_rank_fusion(&vector_results, &bm25_results, 10.0, 1.0, 0.0);
        let fused_high_k = reciprocal_rank_fusion(&vector_results, &bm25_results, 100.0, 1.0, 0.0);

        // With lower k, the difference between ranks is more pronounced
        let diff_low_k = fused_low_k[0].1 - fused_low_k[1].1;
        let diff_high_k = fused_high_k[0].1 - fused_high_k[1].1;
        assert!(diff_low_k > diff_high_k);
    }

    #[test]
    fn test_search_config_custom() {
        let config = SearchConfig {
            limit: 20,
            vector_weight: 0.6,
            bm25_weight: 0.4,
            min_score: 0.5,
            hybrid: false,
            rrf_k: 30.0,
        };

        assert_eq!(config.limit, 20);
        assert!((config.vector_weight - 0.6).abs() < f32::EPSILON);
        assert!((config.bm25_weight - 0.4).abs() < f32::EPSILON);
        assert!((config.min_score - 0.5).abs() < f32::EPSILON);
        assert!(!config.hybrid);
        assert!((config.rrf_k - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_result_struct() {
        let result = SearchResult {
            id: "chunk-123".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 10,
            end_line: 25,
            content: "fn hello() {}".to_string(),
            kind: "function".to_string(),
            name: Some("hello".to_string()),
            signature: Some("fn hello()".to_string()),
            scope: Some("impl Foo".to_string()),
            score: 0.85,
            vector_score: Some(0.9),
            bm25_score: Some(0.75),
        };

        assert_eq!(result.id, "chunk-123");
        assert_eq!(result.file_path, "src/lib.rs");
        assert_eq!(result.start_line, 10);
        assert_eq!(result.end_line, 25);
        assert_eq!(result.name, Some("hello".to_string()));
        assert!((result.score - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_result_minimal() {
        let result = SearchResult {
            id: "chunk-456".to_string(),
            file_path: "test.py".to_string(),
            start_line: 1,
            end_line: 5,
            content: "def test(): pass".to_string(),
            kind: "function".to_string(),
            name: None,
            signature: None,
            scope: None,
            score: 0.5,
            vector_score: None,
            bm25_score: None,
        };

        assert!(result.name.is_none());
        assert!(result.signature.is_none());
        assert!(result.scope.is_none());
        assert!(result.vector_score.is_none());
        assert!(result.bm25_score.is_none());
    }

    #[test]
    fn test_rrf_ordering_stability() {
        // Test that RRF produces consistent ordering
        let vector_results = vec![
            ("a".to_string(), 0.9f32),
            ("b".to_string(), 0.8f32),
            ("c".to_string(), 0.7f32),
        ];
        let bm25_results = vec![
            ("c".to_string(), 10.0f64),
            ("b".to_string(), 8.0f64),
            ("a".to_string(), 6.0f64),
        ];

        let fused1 = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);
        let fused2 = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

        // Results should be identical
        assert_eq!(fused1.len(), fused2.len());
        for (r1, r2) in fused1.iter().zip(fused2.iter()) {
            assert_eq!(r1.0, r2.0);
            assert!((r1.1 - r2.1).abs() < f32::EPSILON);
        }
    }
}
