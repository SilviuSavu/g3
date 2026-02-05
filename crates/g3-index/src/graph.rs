//! Knowledge graph representing code symbols, files, and cross-references.
//!
//! This module provides a unified graph data model for representing codebase
//! structure as a directed graph with bidirectional references.
//!
//! # Architecture
//!
//! - **Symbol Nodes**: Functions, types, modules with metadata
//! - **File Nodes**: Document boundaries, language detection
//! - **Edges**: Definitions, references, call hierarchies, dependencies
//!
//! # Example
//!
//! ```no_run
//! use g3_index::graph::{CodeGraph, EdgeKind, SymbolKind, SymbolNode};
//!
//! let mut graph = CodeGraph::new();
//!
//! // Add a symbol
//! let symbol = SymbolNode::new(
//!     "my_function",
//!     SymbolKind::Function,
//!     "src/lib.rs",
//!     10,
//! );
//! graph.add_symbol(symbol);
//!
//! // Add reference from another location
//! graph.add_reference("src/main.rs", "my_function", EdgeKind::Calls, 5);
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unique identifier for a symbol in graph.
pub type SymbolId = String;

/// Unique identifier for a file in graph.
pub type FileId = String;

/// Type of code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    TypeAlias,
    Constant,
    Static,
    Module,
    Variable,
    Parameter,
    Generic,
    Macro,
    Other,
}

impl SymbolKind {
    /// Display label for symbol kind.
    pub fn label(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Method => "method",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Constant => "const",
            SymbolKind::Static => "static",
            SymbolKind::Module => "mod",
            SymbolKind::Variable => "var",
            SymbolKind::Parameter => "param",
            SymbolKind::Generic => "generic",
            SymbolKind::Macro => "macro",
            SymbolKind::Other => "symbol",
        }
    }
}

/// Edge type representing relationships between symbols and files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    /// Defines a symbol (file -> symbol)
    Defines,
    /// References a symbol (symbol -> symbol)
    References,
    /// Calls a function/method (symbol -> symbol)
    Calls,
    /// Inherits from a type (symbol -> symbol)
    Inherits,
    /// Implements a trait/interface (symbol -> symbol)
    Implements,
    /// Contains symbols (module -> symbol)
    Contains,
    /// Belongs to file (symbol -> file)
    BelongsTo,
    /// Imports a symbol (file -> symbol)
    Imports,
    /// Uses a type (symbol -> symbol)
    Uses,
    /// Overrides a method (symbol -> symbol)
    Overrides,
    /// Alias of another symbol (symbol -> symbol)
    AliasOf,
    /// Generic type parameter (symbol -> symbol)
    TypeParam,
}

/// Represents a code symbol node in graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Unique identifier (e.g., "src/lib.rs::my_function")
    pub id: SymbolId,
    /// Symbol name
    pub name: String,
    /// Type of symbol
    pub kind: SymbolKind,
    /// File where symbol is defined
    pub file_id: FileId,
    /// Start line (1-indexed)
    pub line_start: usize,
    /// End line (1-indexed)
    pub line_end: usize,
    /// Start column (1-indexed)
    pub column_start: usize,
    /// End column (1-indexed)
    pub column_end: usize,
    /// Optional signature
    pub signature: Option<String>,
    /// Optional doc comment
    pub documentation: Option<String>,
    /// Module path (e.g., "crate::module::submodule")
    pub module_path: Option<String>,
    /// Parent symbol (if nested)
    pub parent_id: Option<SymbolId>,
    /// Type information (if available)
    pub type_info: Option<String>,
    /// Generic parameters
    pub generic_params: Vec<String>,
    /// Visibility (public, private, etc.)
    pub visibility: Option<String>,
    /// Is this symbol deprecated?
    pub deprecated: bool,
    /// Language-specific metadata (JSON-serializable)
    pub metadata: Option<serde_json::Value>,
}

impl SymbolNode {
    /// Create a new symbol node.
    pub fn new(
        name: impl Into<String>,
        kind: SymbolKind,
        file_id: impl Into<String>,
        line: usize,
    ) -> Self {
        let name = name.into();
        let file_id = file_id.into();
        let id = Self::generate_id(&file_id, &name, line);

        Self {
            id,
            name,
            kind,
            file_id,
            line_start: line,
            line_end: line,
            column_start: 1,
            column_end: 1,
            signature: None,
            documentation: None,
            module_path: None,
            parent_id: None,
            type_info: None,
            generic_params: Vec::new(),
            visibility: None,
            deprecated: false,
            metadata: None,
        }
    }

    /// Generate a unique symbol ID.
    fn generate_id(file_id: &str, name: &str, line: usize) -> String {
        format!("{}::{}@{}", file_id, name, line)
    }

    /// Set line range.
    pub fn with_range(mut self, start_line: usize, end_line: usize) -> Self {
        self.line_start = start_line;
        self.line_end = end_line;
        self
    }

    /// Set column range.
    pub fn with_columns(mut self, start_col: usize, end_col: usize) -> Self {
        self.column_start = start_col;
        self.column_end = end_col;
        self
    }

    /// Set signature.
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// Set documentation.
    pub fn with_documentation(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    /// Set module path.
    pub fn with_module(mut self, path: impl Into<String>) -> Self {
        self.module_path = Some(path.into());
        self
    }

    /// Set parent symbol.
    pub fn with_parent(mut self, parent: SymbolId) -> Self {
        self.parent_id = Some(parent);
        self
    }

    /// Set type information.
    pub fn with_type(mut self, ty: impl Into<String>) -> Self {
        self.type_info = Some(ty.into());
        self
    }

    /// Add a generic parameter.
    pub fn add_generic(mut self, param: impl Into<String>) -> Self {
        self.generic_params.push(param.into());
        self
    }

    /// Set visibility.
    pub fn with_visibility(mut self, vis: impl Into<String>) -> Self {
        self.visibility = Some(vis.into());
        self
    }

    /// Mark as deprecated.
    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }

    /// Set language-specific metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Represents a file node in graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Unique identifier (file path)
    pub id: FileId,
    /// File path
    pub path: PathBuf,
    /// Programming language
    pub language: String,
    /// Total lines of code
    pub loc: usize,
    /// Number of symbols defined
    pub symbol_count: usize,
    /// Is this file a test file?
    pub is_test: bool,
    /// When file was last modified
    pub modified_at: Option<u64>,
}

impl FileNode {
    /// Create a new file node.
    pub fn new(path: impl AsRef<Path>, language: impl Into<String>) -> Self {
        let path = path.as_ref().to_path_buf();
        let id = Self::generate_id(&path);

        Self {
            id,
            path,
            language: language.into(),
            loc: 0,
            symbol_count: 0,
            is_test: false,
            modified_at: None,
        }
    }

    /// Generate a unique file ID.
    fn generate_id(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    /// Set lines of code.
    pub fn with_loc(mut self, loc: usize) -> Self {
        self.loc = loc;
        self
    }

    /// Set symbol count.
    pub fn with_symbol_count(mut self, count: usize) -> Self {
        self.symbol_count = count;
        self
    }

    /// Mark as test file.
    pub fn test_file(mut self) -> Self {
        self.is_test = true;
        self
    }

    /// Set modification time.
    pub fn with_modified(mut self, time: u64) -> Self {
        self.modified_at = Some(time);
        self
    }
}

/// Represents an edge between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source node ID (can be SymbolId or FileId)
    pub source: String,
    /// Target node ID (can be SymbolId or FileId)
    pub target: String,
    /// Type of relationship
    pub kind: EdgeKind,
    /// File where this edge occurs (for references)
    pub location_file: Option<FileId>,
    /// Line number where edge occurs (for references)
    pub location_line: Option<usize>,
}

impl Edge {
    /// Create a new edge.
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: EdgeKind,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            kind,
            location_file: None,
            location_line: None,
        }
    }

    /// Set location information.
    pub fn with_location(mut self, file: FileId, line: usize) -> Self {
        self.location_file = Some(file);
        self.location_line = Some(line);
        self
    }
}

/// Directed graph representing codebase structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeGraph {
    /// All symbols
    pub symbols: HashMap<SymbolId, SymbolNode>,
    /// All files
    pub files: HashMap<FileId, FileNode>,
    /// All edges
    pub edges: Vec<Edge>,
    /// Reverse edge index (target -> sources)
    pub reverse_edges: HashMap<String, Vec<Edge>>,
    /// Symbol name index (name -> IDs)
    pub symbol_name_index: HashMap<String, Vec<SymbolId>>,
    /// File language index (language -> file IDs)
    pub file_language_index: HashMap<String, Vec<FileId>>,
}

impl CodeGraph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if graph is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty() && self.files.is_empty()
    }

    /// Get total number of nodes (symbols + files).
    pub fn node_count(&self) -> usize {
        self.symbols.len() + self.files.len()
    }

    /// Get total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Add a symbol to graph.
    pub fn add_symbol(&mut self, symbol: SymbolNode) {
        // Update name index
        self.symbol_name_index
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol.id.clone());

        // Add symbol
        let symbol_id = symbol.id.clone();
        let file_id = symbol.file_id.clone();

        self.symbols.insert(symbol_id.clone(), symbol);

        // Create "defines" edge from file to symbol
        self.add_edge(Edge::new(&file_id, &symbol_id, EdgeKind::Defines));

        // Create "belongs to" edge from symbol to file
        self.add_edge(Edge::new(&symbol_id, &file_id, EdgeKind::BelongsTo));

        // Update file's symbol count
        if let Some(file) = self.files.get_mut(&file_id) {
            file.symbol_count += 1;
        }
    }

    /// Add a file to graph.
    pub fn add_file(&mut self, file: FileNode) {
        let file_id = file.id.clone();
        let language = file.language.clone();

        // Update language index
        self.file_language_index
            .entry(language)
            .or_default()
            .push(file_id.clone());

        self.files.insert(file_id, file);
    }

    /// Add an edge to graph.
    pub fn add_edge(&mut self, edge: Edge) {
        // Add to reverse index
        self.reverse_edges
            .entry(edge.target.clone())
            .or_default()
            .push(Edge::new(&edge.source, &edge.target, edge.kind));

        self.edges.push(edge);
    }

    /// Add a reference from a location to a symbol.
    pub fn add_reference(
        &mut self,
        from_file: &str,
        symbol_name: &str,
        kind: EdgeKind,
        line: usize,
    ) {
        // Find all symbols with this name
        if let Some(symbol_ids) = self.symbol_name_index.get(symbol_name) {
            for symbol_id in symbol_ids.clone() {
                self.add_edge(
                    Edge::new(from_file, symbol_id, kind)
                        .with_location(from_file.to_string(), line),
                );
            }
        }
    }

    /// Remove a symbol from graph.
    pub fn remove_symbol(&mut self, symbol_id: &SymbolId) -> Result<(), GraphError> {
        let symbol_name = self.symbols.get(symbol_id)
            .ok_or_else(|| GraphError::SymbolNotFound(symbol_id.clone()))?
            .name.clone();
        let file_id = self.symbols.get(symbol_id)
            .ok_or_else(|| GraphError::SymbolNotFound(symbol_id.clone()))?
            .file_id.clone();

        // Remove from name index
        let should_remove_key = if let Some(ids) = self.symbol_name_index.get_mut(&symbol_name) {
            ids.retain(|id| id != symbol_id);
            ids.is_empty()
        } else {
            false
        };
        if should_remove_key {
            self.symbol_name_index.remove(&symbol_name);
        }

        // Remove all edges involving this symbol
        self.edges.retain(|e| e.source != *symbol_id && e.target != *symbol_id);

        // Remove from reverse index
        self.reverse_edges.remove(symbol_id);
        for edges in self.reverse_edges.values_mut() {
            edges.retain(|e| e.source != *symbol_id);
        }

        // Remove symbol
        self.symbols.remove(symbol_id);

        // Update file's symbol count
        if let Some(file) = self.files.get_mut(&file_id) {
            file.symbol_count = file.symbol_count.saturating_sub(1);
        }

        Ok(())
    }

    /// Remove a file and all its symbols from graph.
    pub fn remove_file(&mut self, file_id: &FileId) -> Result<(), GraphError> {
        // Collect symbols in this file
        let symbols_to_remove: Vec<SymbolId> = self.symbols
            .values()
            .filter(|s| &s.file_id == file_id)
            .map(|s| s.id.clone())
            .collect();

        // Remove symbols
        for symbol_id in symbols_to_remove {
            self.remove_symbol(&symbol_id)?;
        }

        // Remove from language index
        if let Some(file) = self.files.get(file_id) {
            if let Some(files) = self.file_language_index.get_mut(&file.language) {
                files.retain(|id| id != file_id);
            }
        }

        // Remove file
        self.files.remove(file_id);

        // Remove all edges involving this file
        self.edges.retain(|e| e.source != *file_id && e.target != *file_id);

        // Remove from reverse index
        self.reverse_edges.remove(file_id);
        for edges in self.reverse_edges.values_mut() {
            edges.retain(|e| e.source != *file_id);
        }

        Ok(())
    }

    /// Get a symbol by ID.
    pub fn get_symbol(&self, id: &SymbolId) -> Option<&SymbolNode> {
        self.symbols.get(id)
    }

    /// Get a file by ID.
    pub fn get_file(&self, id: &FileId) -> Option<&FileNode> {
        self.files.get(id)
    }

    /// Find all symbols with a given name.
    pub fn find_symbols_by_name(&self, name: &str) -> Vec<&SymbolNode> {
        self.symbol_name_index
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.symbols.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all incoming edges to a node.
    pub fn incoming_edges(&self, target: &str) -> Vec<Edge> {
        self.reverse_edges.get(target).cloned().unwrap_or_default()
    }

    /// Get all outgoing edges from a node.
    pub fn outgoing_edges(&self, source: &str) -> Vec<Edge> {
        self.edges.iter().filter(|e| e.source == source).cloned().collect()
    }

    /// Get all edges of a specific kind.
    pub fn edges_by_kind(&self, kind: EdgeKind) -> Vec<Edge> {
        self.edges.iter().filter(|e| e.kind == kind).cloned().collect()
    }

    /// Find all callers of a symbol (incoming "calls" edges).
    pub fn find_callers(&self, symbol_id: &SymbolId) -> Vec<SymbolId> {
        self.incoming_edges(symbol_id)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .map(|e| e.source.clone())
            .filter(|id| self.symbols.contains_key(id))
            .collect()
    }

    /// Find all symbols called by this symbol (outgoing "calls" edges).
    pub fn find_callees(&self, symbol_id: &SymbolId) -> Vec<SymbolId> {
        self.outgoing_edges(symbol_id)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .map(|e| e.target.clone())
            .filter(|id| self.symbols.contains_key(id))
            .collect()
    }

    /// Find all references to a symbol (all incoming edges except Defines/BelongsTo).
    pub fn find_references(&self, symbol_id: &SymbolId) -> Vec<Edge> {
        self.incoming_edges(symbol_id)
            .into_iter()
            .filter(|e| !matches!(e.kind, EdgeKind::Defines | EdgeKind::BelongsTo))
            .collect()
    }

    /// Get all symbols in a file.
    pub fn symbols_in_file(&self, file_id: &FileId) -> Vec<&SymbolNode> {
        self.symbols
            .values()
            .filter(|s| &s.file_id == file_id)
            .collect()
    }

    /// Get all files of a language.
    pub fn files_by_language(&self, language: &str) -> Vec<&FileNode> {
        self.file_language_index
            .get(language)
            .map(|ids| ids.iter().filter_map(|id| self.files.get(id)).collect())
            .unwrap_or_default()
    }

    /// Clear all data from graph.
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.files.clear();
        self.edges.clear();
        self.reverse_edges.clear();
        self.symbol_name_index.clear();
        self.file_language_index.clear();
    }

    /// Merge another graph into this one.
    pub fn merge(&mut self, other: CodeGraph) {
        // Merge files
        for (id, file) in other.files {
            if !self.files.contains_key(&id) {
                self.add_file(file);
            }
        }

        // Merge symbols
        for (id, symbol) in other.symbols {
            if !self.symbols.contains_key(&id) {
                self.add_symbol(symbol);
            }
        }

        // Merge edges
        for edge in other.edges {
            // Check if edge already exists
            let exists = self.edges.iter().any(|e| {
                e.source == edge.source
                    && e.target == edge.target
                    && e.kind == edge.kind
                    && e.location_file == edge.location_file
            });
            if !exists {
                self.add_edge(edge);
            }
        }
    }
}

/// Graph operation errors.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid edge: source '{0}' or target '{1}' not found")]
    InvalidEdge(String, String),

    #[error("Graph cycle detected: {0}")]
    CycleDetected(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_node_creation() {
        let symbol = SymbolNode::new("test_func", SymbolKind::Function, "src/lib.rs", 10)
            .with_signature("fn test_func() -> i32")
            .with_documentation("A test function")
            .with_module("crate::test")
            .with_type("i32");

        assert_eq!(symbol.name, "test_func");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.line_start, 10);
        assert!(symbol.signature.is_some());
        assert!(symbol.documentation.is_some());
        assert!(symbol.module_path.is_some());
        assert!(symbol.type_info.is_some());
    }

    #[test]
    fn test_file_node_creation() {
        let file = FileNode::new("src/lib.rs", "rust")
            .with_loc(100)
            .with_symbol_count(5)
            .test_file()
            .with_modified(1234567890);

        assert_eq!(file.language, "rust");
        assert_eq!(file.loc, 100);
        assert_eq!(file.symbol_count, 5);
        assert!(file.is_test);
        assert!(file.modified_at.is_some());
    }

    #[test]
    fn test_code_graph_add_symbol() {
        let mut graph = CodeGraph::new();
        let file = FileNode::new("src/lib.rs", "rust");
        graph.add_file(file);

        let symbol = SymbolNode::new("test_func", SymbolKind::Function, "src/lib.rs", 10);
        graph.add_symbol(symbol);

        assert_eq!(graph.symbols.len(), 1);
        assert_eq!(graph.files.get("src/lib.rs").unwrap().symbol_count, 1);
        assert!(graph.symbol_name_index.contains_key("test_func"));
    }

    #[test]
    fn test_code_graph_find_by_name() {
        let mut graph = CodeGraph::new();
        let file = FileNode::new("src/lib.rs", "rust");
        graph.add_file(file);

        let s1 = SymbolNode::new("func1", SymbolKind::Function, "src/lib.rs", 10);
        let s2 = SymbolNode::new("func2", SymbolKind::Function, "src/main.rs", 20);
        graph.add_symbol(s1);
        graph.add_symbol(s2);

        let results = graph.find_symbols_by_name("func1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "func1");
    }

    #[test]
    fn test_code_graph_add_reference() {
        let mut graph = CodeGraph::new();
        let file = FileNode::new("src/lib.rs", "rust");
        graph.add_file(file);

        let symbol = SymbolNode::new("target_func", SymbolKind::Function, "src/lib.rs", 10);
        graph.add_symbol(symbol);

        graph.add_reference("src/lib.rs", "target_func", EdgeKind::Calls, 50);

        let refs = graph.find_references(&graph.symbols.keys().next().unwrap());
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, EdgeKind::Calls);
    }

    #[test]
    fn test_code_graph_remove_symbol() {
        let mut graph = CodeGraph::new();
        let file = FileNode::new("src/lib.rs", "rust");
        graph.add_file(file);

        let symbol = SymbolNode::new("test_func", SymbolKind::Function, "src/lib.rs", 10);
        let id = symbol.id.clone();
        graph.add_symbol(symbol);

        graph.remove_symbol(&id).unwrap();

        assert_eq!(graph.symbols.len(), 0);
        assert_eq!(graph.files.get("src/lib.rs").unwrap().symbol_count, 0);
        assert!(graph.symbol_name_index.get("test_func").is_none());
    }

    #[test]
    fn test_code_graph_find_callers() {
        let mut graph = CodeGraph::new();
        let file = FileNode::new("src/lib.rs", "rust");
        graph.add_file(file);

        let target = SymbolNode::new("target_func", SymbolKind::Function, "src/lib.rs", 10);
        let caller = SymbolNode::new("caller_func", SymbolKind::Function, "src/lib.rs", 20);
        let target_id = target.id.clone();
        let caller_id = caller.id.clone();

        graph.add_symbol(target);
        graph.add_symbol(caller);

        graph.add_edge(Edge::new(&caller_id, &target_id, EdgeKind::Calls));

        let callers = graph.find_callers(&target_id);
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0], caller_id);
    }

    #[test]
    fn test_symbol_kind_labels() {
        assert_eq!(SymbolKind::Function.label(), "fn");
        assert_eq!(SymbolKind::Struct.label(), "struct");
        assert_eq!(SymbolKind::Method.label(), "method");
        assert_eq!(SymbolKind::Other.label(), "symbol");
    }
}
