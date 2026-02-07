//! End-to-End Demo: Codebase Intelligence System
//!
//! This demo shows the Intelligence System in action by:
//! 1. Chunking a real Rust file
//! 2. Building a knowledge graph from the indexed code
//! 3. Demonstrating graph traversal (BFS, DFS)
//! 4. Running BM25 lexical search
//! 5. Demonstrating unified result formatting

use g3_index::chunker::CodeChunker;
use g3_index::graph::{CodeGraph, Edge, EdgeKind, FileNode, SymbolKind, SymbolNode};
use g3_index::search::BM25Index;
use g3_index::traverser::GraphTraverser;
use std::path::Path;

fn main() {
    println!("=== Codebase Intelligence System - Live Demo ===\n");

    // Demo 1: Chunk a real Rust file
    println!("Demo 1: Chunking a real Rust file");
    println!("-----------------------------------");
    demo_chunking();

    // Demo 2: Build a knowledge graph from the indexed code
    println!("\nDemo 2: Building Knowledge Graph");
    println!("---------------------------------");
    demo_graph_building();

    // Demo 3: Graph traversal
    println!("\nDemo 3: Graph Traversal (BFS & DFS)");
    println!("------------------------------------");
    demo_graph_traversal();

    // Demo 4: BM25 lexical search
    println!("\nDemo 4: BM25 Lexical Search");
    println!("---------------------------");
    demo_lexical_search();

    println!("\n=== Demo Complete ===");
    println!("\nNext steps:");
    println!("1. Run: cargo test -p g3-index --lib traverser");
    println!("2. Run: cargo test -p g3-index --lib integration");
    println!("3. Run: cargo test -p g3-index --lib unified_index");
    println!("4. Run: cargo test -p g3-core --test intelligence_system_test");
}

fn demo_chunking() {
    let test_file = Path::new("crates/g3-index/src/lib.rs");

    if !test_file.exists() {
        println!("Note: Demo file not found at {}", test_file.display());
        println!("Creating sample chunk from string instead...\n");
        demo_sample_chunking();
        return;
    }

    println!("File: {}", test_file.display());

    match CodeChunker::new(500, true) {
        Ok(mut chunker) => {
            match chunker.chunk_file(test_file) {
                Ok(chunks) => {
                    println!("Found {} chunks:", chunks.len());
                    for (i, chunk) in chunks.iter().enumerate().take(5) {
                        println!(
                            "  Chunk {}: {} at {}:{}-{}",
                            i + 1,
                            chunk.metadata.name,
                            chunk.file_path,
                            chunk.metadata.line_start,
                            chunk.metadata.line_end
                        );
                    }
                    if chunks.len() > 5 {
                        println!("  ... and {} more chunks", chunks.len() - 5);
                    }
                }
                Err(e) => {
                    println!("Failed to chunk file: {}", e);
                    demo_sample_chunking();
                }
            }
        }
        Err(e) => {
            println!("Failed to create chunker: {}", e);
            demo_sample_chunking();
        }
    }
}

fn demo_sample_chunking() {
    println!("Creating sample chunk...\n");

    let sample_code = r#"pub fn sample_function(x: i32) -> i32 {
    x * 2
}
"#;

    println!("Sample Rust code:");
    println!("{}", sample_code);
    println!("This code would be chunked into semantic blocks by tree-sitter.");
    println!("\nChunk metadata includes:");
    println!("  - Name: sample_function");
    println!("  - Type: function");
    println!("  - Line range: computed by parser");
    println!("  - Signature: pub fn sample_function(x: i32) -> i32");
    println!("  - Hash: SHA256 of content");
}

fn demo_graph_building() {
    println!("Building knowledge graph from indexed data...\n");

    let mut graph = CodeGraph::new();

    // Add a file node
    let file = FileNode::new("crates/g3-index/src/lib.rs", "rust");
    graph.add_file(file);

    // Add some symbol nodes
    let nodes = vec![
        ("g3_index", "module", "crates/g3-index/src/lib.rs", 1, 50),
        ("chunker", "mod", "crates/g3-index/src/lib.rs", 12, 12),
        ("embeddings", "mod", "crates/g3-index/src/lib.rs", 13, 13),
        ("graph", "mod", "crates/g3-index/src/lib.rs", 14, 14),
        ("search", "mod", "crates/g3-index/src/lib.rs", 20, 20),
        ("traverser", "mod", "crates/g3-index/src/lib.rs", 22, 22),
    ];

    for (name, kind, file_id, start, end) in nodes {
        let symbol = SymbolNode::new(name, parse_symbol_kind(kind), file_id, start)
            .with_range(start, end);
        graph.add_symbol(symbol);
    }

    // Add some edges (references)
    graph.add_reference(
        "crates/g3-index/src/lib.rs",
        "chunker",
        EdgeKind::Contains,
        27,
    );

    graph.add_reference(
        "crates/g3-index/src/lib.rs",
        "traverser",
        EdgeKind::Contains,
        39,
    );

    println!("Graph built successfully!");
    println!("Graph Statistics:");
    println!("  - Total symbols: {}", graph.symbols.len());
    println!("  - Total files: {}", graph.files.len());
    println!("  - Total edges: {}", graph.edges.len());

    // Find symbols by name
    println!("\nSearching for 'chunker':");
    let results = graph.find_symbols_by_name("chunker");
    for sym in &results {
        println!("  - Found: {} at {}:{}-{}", sym.name, sym.file_id, sym.line_start, sym.line_end);
    }

    println!("\nSearching for 'traverser':");
    let results = graph.find_symbols_by_name("traverser");
    for sym in &results {
        println!("  - Found: {} at {}:{}-{}", sym.name, sym.file_id, sym.line_start, sym.line_end);
    }
}

fn parse_symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "mod" => SymbolKind::Module,
        "function" => SymbolKind::Function,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "impl" => SymbolKind::Other,
        "class" => SymbolKind::Other,
        _ => SymbolKind::Other,
    }
}

fn demo_graph_traversal() {
    println!("Testing BFS and DFS traversal on knowledge graph...\n");

    let mut graph = CodeGraph::new();

    // Add a file
    let file = FileNode::new("test.rs", "rust");
    graph.add_file(file);

    // Create a small test graph: A -> B -> C -> D
    let a = SymbolNode::new("A", SymbolKind::Function, "test.rs", 1).with_range(1, 10);
    let b = SymbolNode::new("B", SymbolKind::Function, "test.rs", 11).with_range(11, 20);
    let c = SymbolNode::new("C", SymbolKind::Function, "test.rs", 21).with_range(21, 30);
    let d = SymbolNode::new("D", SymbolKind::Function, "test.rs", 31).with_range(31, 40);

    graph.add_symbol(a);
    graph.add_symbol(b);
    graph.add_symbol(c);
    graph.add_symbol(d);

    // Add edges: A calls B, B calls C, C calls D
    let a_id = graph.symbol_name_index.get("A").and_then(|ids| ids.first()).cloned().unwrap();
    let b_id = graph.symbol_name_index.get("B").and_then(|ids| ids.first()).cloned().unwrap();
    let c_id = graph.symbol_name_index.get("C").and_then(|ids| ids.first()).cloned().unwrap();
    let d_id = graph.symbol_name_index.get("D").and_then(|ids| ids.first()).cloned().unwrap();

    graph.add_edge(Edge::new(&a_id, &b_id, EdgeKind::Calls).with_location("test.rs".to_string(), 5));
    graph.add_edge(Edge::new(&b_id, &c_id, EdgeKind::Calls).with_location("test.rs".to_string(), 15));
    graph.add_edge(Edge::new(&c_id, &d_id, EdgeKind::Calls).with_location("test.rs".to_string(), 25));

    let traverser = GraphTraverser::new();

    println!("Graph structure: A -> B -> C -> D");
    println!("Starting BFS from 'A'\n");

    // BFS traversal
    let result = traverser.bfs(&graph, &a_id);
    println!("BFS Result (from A):");
    for r in &result {
        println!("  Node: {} at distance {}", r.node_id, r.distance);
        if !r.path.is_empty() {
            println!("    Path: {}", r.path.join(" -> "));
        }
    }

    println!("\nTesting DFS traversal...\n");
    let dfs_result = traverser.dfs(&graph, &a_id);

    println!("DFS Result (from A):");
    for r in &dfs_result {
        println!("  Node: {} at distance {}", r.node_id, r.distance);
        if !r.path.is_empty() {
            println!("    Path: {}", r.path.join(" -> "));
        }
    }
}

fn demo_lexical_search() {
    println!("Creating BM25 index and performing lexical search...\n");

    // Create a new BM25 index
    let mut index = BM25Index::new();

    // Add some sample documents
    let docs = vec![
        ("doc1", "fn process_request() { handle_error() }"),
        ("doc2", "pub fn handle_error(e: Error) { log!(e) }"),
        ("doc3", "struct Request { data: String }"),
    ];

    for (id, content) in &docs {
        index.add_document(id.to_string(), content.to_string());
    }

    println!("Added {} documents to index", docs.len());

    // Search for "process"
    println!("\nSearching for 'process':");
    let results = index.search("process", 10);
    println!("Found {} results:", results.len());
    for (doc_id, score) in &results {
        println!("  - {} (score: {:.3})", doc_id, score);
    }

    // Search for "error"
    println!("\nSearching for 'error':");
    let results = index.search("error", 10);
    println!("Found {} results:", results.len());
    for (doc_id, score) in &results {
        println!("  - {} (score: {:.3})", doc_id, score);
    }

    // Search for "request"
    println!("\nSearching for 'request':");
    let results = index.search("request", 10);
    println!("Found {} results:", results.len());
    for (doc_id, score) in &results {
        println!("  - {} (score: {:.3})", doc_id, score);
    }

    println!("\nBM25 algorithm:");
    println!("  - Computes term frequency (how often term appears in doc)");
    println!("  - Computes inverse document frequency (how rare term is across corpus)");
    println!("  - Combines with length normalization");
    println!("  - Higher score = better match");
}
