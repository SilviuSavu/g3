//! Codebase indexing and semantic search for G3.
//!
//! This crate provides:
//! - AST-based code chunking using tree-sitter
//! - Embedding generation via OpenRouter (Qwen3-Embedding-8B)
//! - Vector storage in Qdrant (4096 dimensions)
//! - Hybrid search (vector + BM25)
//! - Background file watching for auto-indexing
//! - Knowledge graph for code symbols, files, and cross-references
//! - Persistence layer with incremental updates

pub mod chunker;
pub mod embeddings;
pub mod graph;
pub mod graph_builder;
pub mod indexer;
pub mod manifest;
pub mod qdrant;
pub mod search;
pub mod storage;
pub mod watcher;

// Re-exports
pub use chunker::{Chunk, ChunkMetadata, CodeChunker};
pub use embeddings::EmbeddingProvider;
pub use graph::{CodeGraph, Edge, EdgeKind, FileNode, GraphError, SymbolKind, SymbolNode};
pub use graph_builder::GraphBuilder;
pub use indexer::{Indexer, IndexerConfig, IndexStats};
pub use manifest::IndexManifest;
pub use search::{BM25Index, HybridSearcher, SearchConfig, SearchResult, reciprocal_rank_fusion};
pub use storage::{
    DEFAULT_GRAPH_DIR, FileIndex, FileIndexEntry, GraphStorage, ScannedFile, SnapshotMetadata,
    UpdateStats,
};

/// Default Qdrant collection name
pub const DEFAULT_COLLECTION: &str = "g3-codebase";

/// Default embedding dimensions (Qwen3-Embedding-8B)
pub const DEFAULT_DIMENSIONS: usize = 4096;
