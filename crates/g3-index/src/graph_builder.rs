//! Graph builder for constructing CodeGraph from chunks.
//!
//! This module bridges the chunker output with the knowledge graph,
//! converting extracted chunks into graph nodes and edges.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::chunker::{Chunk, ChunkType};
use crate::graph::{CodeGraph, FileNode, SymbolKind, SymbolNode};
use crate::storage::{GraphStorage, DEFAULT_GRAPH_DIR};

/// Orchestrates graph building from indexed chunks.
///
/// GraphBuilder maintains a CodeGraph and GraphStorage, providing methods
/// to add files and symbols during indexing, and persist the graph to disk.
pub struct GraphBuilder {
    storage: GraphStorage,
    root_path: PathBuf,
}

impl GraphBuilder {
    /// Create a new GraphBuilder for the given workspace root.
    ///
    /// Initializes storage in `.g3-index/graph/` under the root path.
    pub fn new(root_path: &Path) -> Result<Self> {
        let graph_dir = root_path.join(DEFAULT_GRAPH_DIR);
        let storage = GraphStorage::init(&graph_dir)
            .with_context(|| format!("Failed to initialize graph storage at {:?}", graph_dir))?;

        info!(
            "GraphBuilder initialized: {} symbols, {} files",
            storage.graph().symbols.len(),
            storage.graph().files.len()
        );

        Ok(Self {
            storage,
            root_path: root_path.to_path_buf(),
        })
    }

    /// Add a file and its chunks to the graph.
    ///
    /// Converts chunks into symbols and adds appropriate edges.
    pub fn add_file(&mut self, file_path: &Path, chunks: &[Chunk]) -> Result<()> {
        let relative_path = file_path
            .strip_prefix(&self.root_path)
            .unwrap_or(file_path);
        let file_id = relative_path.to_string_lossy().to_string();

        // Detect language from extension
        let language = file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(extension_to_language)
            .unwrap_or("unknown");

        // Count lines of code
        let loc = chunks
            .iter()
            .map(|c| c.metadata.line_end.saturating_sub(c.metadata.line_start) + 1)
            .sum();

        // Remove existing file and its symbols if present
        if self.storage.graph().files.contains_key(&file_id) {
            let _ = self.storage.graph_mut().remove_file(&file_id);
        }

        // Add file node
        let file_node = FileNode::new(relative_path, language).with_loc(loc);
        self.storage.graph_mut().add_file(file_node);

        // Add symbols from chunks
        for chunk in chunks {
            let symbol = chunk_to_symbol(chunk, &file_id);
            self.storage.graph_mut().add_symbol(symbol);
        }

        debug!(
            "Added file to graph: {} ({} chunks)",
            file_id,
            chunks.len()
        );
        Ok(())
    }

    /// Remove a file and its symbols from the graph.
    pub fn remove_file(&mut self, file_path: &Path) -> Result<()> {
        let relative_path = file_path
            .strip_prefix(&self.root_path)
            .unwrap_or(file_path);
        let file_id = relative_path.to_string_lossy().to_string();

        self.storage.graph_mut().remove_file(&file_id)?;
        debug!("Removed file from graph: {}", file_id);
        Ok(())
    }

    /// Save the graph to disk.
    pub fn save(&mut self) -> Result<()> {
        self.storage.save()?;
        info!(
            "Saved graph: {} symbols, {} files, {} edges",
            self.storage.graph().symbols.len(),
            self.storage.graph().files.len(),
            self.storage.graph().edges.len()
        );
        Ok(())
    }

    /// Get a reference to the current graph.
    pub fn graph(&self) -> &CodeGraph {
        self.storage.graph()
    }

    /// Get the number of symbols in the graph.
    pub fn symbol_count(&self) -> usize {
        self.storage.graph().symbols.len()
    }

    /// Get the number of files in the graph.
    pub fn file_count(&self) -> usize {
        self.storage.graph().files.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.storage.graph().is_empty()
    }

    /// Clear the graph.
    pub fn clear(&mut self) -> Result<()> {
        self.storage.clear()
    }

    /// Find all symbols with the given name.
    pub fn find_symbols_by_name(&self, name: &str) -> Vec<&SymbolNode> {
        self.storage.graph().find_symbols_by_name(name)
    }

    /// Get all symbols in a file.
    pub fn symbols_in_file(&self, file_path: &str) -> Vec<&SymbolNode> {
        let file_id = file_path.to_string();
        self.storage.graph().symbols_in_file(&file_id)
    }

    /// Find all callers of a symbol.
    pub fn find_callers(&self, symbol_id: &str) -> Vec<String> {
        let id = symbol_id.to_string();
        self.storage.graph().find_callers(&id)
    }

    /// Find all callees of a symbol.
    pub fn find_callees(&self, symbol_id: &str) -> Vec<String> {
        let id = symbol_id.to_string();
        self.storage.graph().find_callees(&id)
    }

    /// Find all references to a symbol.
    pub fn find_references(&self, symbol_id: &str) -> Vec<crate::graph::Edge> {
        let id = symbol_id.to_string();
        self.storage.graph().find_references(&id)
    }
}

/// Convert a chunk type to a symbol kind.
fn chunk_type_to_symbol_kind(chunk_type: &ChunkType) -> SymbolKind {
    match chunk_type {
        ChunkType::Function => SymbolKind::Function,
        ChunkType::Method => SymbolKind::Method,
        ChunkType::Struct => SymbolKind::Struct,
        ChunkType::Enum => SymbolKind::Enum,
        ChunkType::Trait => SymbolKind::Trait,
        ChunkType::Impl => SymbolKind::Other, // impl blocks are containers
        ChunkType::Const => SymbolKind::Constant,
        ChunkType::Module => SymbolKind::Module,
        ChunkType::Class => SymbolKind::Struct, // Treat class as struct
        ChunkType::Interface => SymbolKind::Interface,
    }
}

/// Convert a chunk to a symbol node.
fn chunk_to_symbol(chunk: &Chunk, file_id: &str) -> SymbolNode {
    let kind = chunk_type_to_symbol_kind(&chunk.metadata.chunk_type);

    let mut symbol = SymbolNode::new(
        &chunk.metadata.name,
        kind,
        file_id,
        chunk.metadata.line_start,
    )
    .with_range(chunk.metadata.line_start, chunk.metadata.line_end);

    if let Some(ref sig) = chunk.metadata.signature {
        symbol = symbol.with_signature(sig);
    }

    if let Some(ref scope) = chunk.metadata.scope {
        // Parse scope to find parent (e.g., "impl Foo" -> parent is Foo)
        symbol = symbol.with_module(scope);
    }

    symbol
}

/// Convert file extension to language name.
fn extension_to_language(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "hh" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::{ChunkMetadata, ChunkType};

    fn make_test_chunk(name: &str, chunk_type: ChunkType, line: usize) -> Chunk {
        Chunk {
            file_path: "test.rs".to_string(),
            content: "fn test() {}".to_string(),
            enriched_content: "fn test() {}".to_string(),
            metadata: ChunkMetadata {
                chunk_type,
                name: name.to_string(),
                signature: Some(format!("fn {}()", name)),
                line_start: line,
                line_end: line + 2,
                module: None,
                scope: None,
                content_hash: "abc123".to_string(),
                language: "rust".to_string(),
            },
        }
    }

    #[test]
    fn test_chunk_type_to_symbol_kind() {
        assert_eq!(
            chunk_type_to_symbol_kind(&ChunkType::Function),
            SymbolKind::Function
        );
        assert_eq!(
            chunk_type_to_symbol_kind(&ChunkType::Struct),
            SymbolKind::Struct
        );
        assert_eq!(
            chunk_type_to_symbol_kind(&ChunkType::Method),
            SymbolKind::Method
        );
        assert_eq!(
            chunk_type_to_symbol_kind(&ChunkType::Trait),
            SymbolKind::Trait
        );
    }

    #[test]
    fn test_chunk_to_symbol() {
        let chunk = make_test_chunk("my_function", ChunkType::Function, 10);
        let symbol = chunk_to_symbol(&chunk, "src/lib.rs");

        assert_eq!(symbol.name, "my_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.file_id, "src/lib.rs");
        assert_eq!(symbol.line_start, 10);
        assert!(symbol.signature.is_some());
    }

    #[test]
    fn test_extension_to_language() {
        assert_eq!(extension_to_language("rs"), "rust");
        assert_eq!(extension_to_language("py"), "python");
        assert_eq!(extension_to_language("ts"), "typescript");
        assert_eq!(extension_to_language("go"), "go");
        assert_eq!(extension_to_language("xyz"), "unknown");
    }
}
