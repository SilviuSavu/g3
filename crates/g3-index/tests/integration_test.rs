//! Integration tests for g3-index crate.
//!
//! These tests verify the end-to-end functionality of the indexing system.

use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use g3_index::chunker::{ChunkType, CodeChunker};
use g3_index::manifest::IndexManifest;
use g3_index::search::BM25Index;

/// Test chunking on actual Rust code.
#[test]
fn test_chunk_real_rust_code() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test_module.rs");

    // Write a realistic Rust file
    fs::write(
        &file_path,
        r#"
//! A test module for chunking.

use std::collections::HashMap;

/// A simple key-value store.
pub struct Store {
    data: HashMap<String, String>,
}

impl Store {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Insert a key-value pair.
    pub fn insert(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

/// A helper function.
fn helper() -> i32 {
    42
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store() {
        let mut store = Store::new();
        store.insert("key".to_string(), "value".to_string());
        assert_eq!(store.get("key"), Some(&"value".to_string()));
    }
}
"#,
    )
    .unwrap();

    let mut chunker = CodeChunker::new(500, true).unwrap();
    let chunks = chunker.chunk_file(&file_path).unwrap();

    // Should have: struct Store, impl Store (with methods), helper function
    assert!(
        chunks.len() >= 3,
        "Expected at least 3 chunks, got {}",
        chunks.len()
    );

    // Verify struct is found
    let struct_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.metadata.chunk_type == ChunkType::Struct)
        .collect();
    assert!(!struct_chunks.is_empty(), "Should find struct Store");
    assert!(struct_chunks[0].metadata.name == "Store");

    // Verify impl block is found
    let impl_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.metadata.chunk_type == ChunkType::Impl)
        .collect();
    assert!(!impl_chunks.is_empty(), "Should find impl Store");

    // Verify helper function is found
    let fn_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.metadata.chunk_type == ChunkType::Function && c.metadata.name == "helper")
        .collect();
    assert!(!fn_chunks.is_empty(), "Should find helper function");
}

/// Test BM25 search on code chunks.
#[test]
fn test_bm25_code_search() {
    let mut bm25 = BM25Index::new();

    // Add some code-like documents
    bm25.add_document(
        "chunk1".to_string(),
        "pub fn calculate_total(items: Vec<Item>) -> f64 { items.iter().map(|i| i.price).sum() }"
            .to_string(),
    );
    bm25.add_document(
        "chunk2".to_string(),
        "pub struct Item { pub name: String, pub price: f64, pub quantity: u32 }".to_string(),
    );
    bm25.add_document(
        "chunk3".to_string(),
        "impl Item { pub fn new(name: String, price: f64) -> Self { Self { name, price, quantity: 1 } } }".to_string(),
    );
    bm25.add_document(
        "chunk4".to_string(),
        "fn process_order(order: Order) -> Result<Receipt, Error> { validate_order(&order)?; Ok(Receipt::new(order)) }".to_string(),
    );

    // Search for "Item" - should find struct and impl
    let results = bm25.search("Item struct", 10);
    assert!(!results.is_empty());
    // chunk2 (struct Item) or chunk3 (impl Item) should rank highly
    let all_ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
    assert!(
        all_ids.contains(&"chunk2") || all_ids.contains(&"chunk3"),
        "Item-related chunks should be in results"
    );

    // Search for "calculate_total" - the exact function name
    let results = bm25.search("calculate_total", 10);
    assert!(!results.is_empty());
    assert_eq!(
        results[0].0, "chunk1",
        "calculate_total should be top result for exact match"
    );

    // Search for exact function name
    let results = bm25.search("process_order", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "chunk4");
}

/// Test manifest persistence and change detection.
#[test]
fn test_manifest_change_detection() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.json");

    let mut manifest = IndexManifest::new();

    // Record some files
    manifest.record_indexed(
        PathBuf::from("src/lib.rs"),
        "hash1".to_string(),
        vec!["c1".to_string(), "c2".to_string()],
    );
    manifest.record_indexed(
        PathBuf::from("src/main.rs"),
        "hash2".to_string(),
        vec!["c3".to_string()],
    );

    // Save and reload
    manifest.save(&manifest_path).unwrap();
    let loaded = IndexManifest::load(&manifest_path).unwrap();

    // Verify change detection
    assert!(
        !loaded.needs_update(&PathBuf::from("src/lib.rs"), "hash1"),
        "Same hash should not need update"
    );
    assert!(
        loaded.needs_update(&PathBuf::from("src/lib.rs"), "hash_changed"),
        "Different hash should need update"
    );
    assert!(
        loaded.needs_update(&PathBuf::from("src/new.rs"), "any_hash"),
        "New file should need update"
    );
}

/// Test chunking multiple files in a directory.
#[test]
fn test_chunk_directory() {
    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();

    // Create multiple files
    fs::write(src_dir.join("lib.rs"), "pub fn lib_fn() {}").unwrap();
    fs::write(
        src_dir.join("utils.rs"),
        "pub fn util_fn() {} pub fn another() {}",
    )
    .unwrap();
    fs::write(src_dir.join("readme.txt"), "This is not code").unwrap(); // Should be ignored

    let mut chunker = CodeChunker::new(500, true).unwrap();
    let chunks = chunker.chunk_directory(&src_dir, &["rs"]).unwrap();

    // Should find functions from both .rs files, ignore .txt
    assert!(
        chunks.len() >= 3,
        "Should find at least 3 functions from 2 files"
    );

    // Verify no chunks from readme.txt
    let txt_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.file_path.contains("readme.txt"))
        .collect();
    assert!(txt_chunks.is_empty(), "Should not chunk .txt files");
}

/// Test enriched content includes context.
#[test]
fn test_enriched_content_context() {
    let mut chunker = CodeChunker::new(500, true).unwrap();

    let source = r#"
impl MyTrait for MyStruct {
    fn trait_method(&self) {
        println!("Hello");
    }
}
"#;

    let chunks = chunker.chunk_source(source, "src/impl.rs", "rust").unwrap();

    // Find the impl chunk
    let impl_chunk = chunks
        .iter()
        .find(|c| c.metadata.chunk_type == ChunkType::Impl)
        .expect("Should find impl chunk");

    // Enriched content should include file path
    assert!(
        impl_chunk.enriched_content.contains("src/impl.rs"),
        "Enriched content should include file path"
    );
}

/// Test that line numbers are accurate.
#[test]
fn test_accurate_line_numbers() {
    let mut chunker = CodeChunker::new(500, true).unwrap();

    let source = r#"// Line 1
// Line 2
// Line 3
fn first_fn() {
    // Line 5
}

fn second_fn() {
    // Line 9
}
"#;

    let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

    let first = chunks
        .iter()
        .find(|c| c.metadata.name == "first_fn")
        .unwrap();
    let second = chunks
        .iter()
        .find(|c| c.metadata.name == "second_fn")
        .unwrap();

    // first_fn starts at line 4, second_fn starts at line 8
    assert_eq!(
        first.metadata.line_start, 4,
        "first_fn should start at line 4"
    );
    assert_eq!(
        second.metadata.line_start, 8,
        "second_fn should start at line 8"
    );
}

/// Test BM25 index persistence.
#[test]
fn test_bm25_save_and_load() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("bm25_index.json");

    // Create and populate index
    let mut bm25 = BM25Index::new();
    bm25.add_document("doc1".to_string(), "rust programming language".to_string());
    bm25.add_document("doc2".to_string(), "python scripting language".to_string());
    bm25.add_document(
        "doc3".to_string(),
        "async rust tokio runtime".to_string(),
    );

    // Save
    bm25.save(&index_path).unwrap();

    // Load and verify
    let loaded = BM25Index::load(&index_path).unwrap();
    assert_eq!(loaded.len(), 3);

    // Search should work on loaded index
    let results = loaded.search("rust", 10);
    assert_eq!(results.len(), 2); // doc1 and doc3 have "rust"
}

/// Test full pipeline: chunk -> index -> search.
#[test]
fn test_chunk_to_bm25_pipeline() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("example.rs");

    // Write Rust code
    fs::write(
        &file_path,
        r#"
/// Calculate the fibonacci sequence.
pub fn fibonacci(n: u64) -> u64 {
    if n <= 1 {
        n
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

/// Check if a number is prime.
pub fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for i in 2..=(n as f64).sqrt() as u64 {
        if n % i == 0 {
            return false;
        }
    }
    true
}

/// Calculate factorial using recursion.
pub fn factorial(n: u64) -> u64 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}
"#,
    )
    .unwrap();

    // Chunk the file
    let mut chunker = CodeChunker::new(500, true).unwrap();
    let chunks = chunker.chunk_file(&file_path).unwrap();

    assert_eq!(chunks.len(), 3, "Should find 3 functions");

    // Index chunks in BM25
    let mut bm25 = BM25Index::new();
    for chunk in &chunks {
        let id = format!(
            "{}:{}:{}",
            chunk.file_path, chunk.metadata.line_start, chunk.metadata.name
        );
        bm25.add_document(id, chunk.enriched_content.clone());
    }

    // Search for fibonacci using exact function name
    let results = bm25.search("fibonacci", 10);
    assert!(!results.is_empty(), "Should find results for 'fibonacci'");
    assert!(
        results[0].0.contains("fibonacci"),
        "Should find fibonacci function"
    );

    // Search for prime using exact function name
    let results = bm25.search("is_prime", 10);
    assert!(!results.is_empty(), "Should find results for 'is_prime'");
    assert!(
        results[0].0.contains("is_prime"),
        "Should find is_prime function"
    );

    // Search for factorial using exact function name
    let results = bm25.search("factorial", 10);
    assert!(!results.is_empty(), "Should find results for 'factorial'");
    assert!(
        results[0].0.contains("factorial"),
        "Should find factorial function"
    );
}

/// Test chunking different languages.
#[test]
fn test_multi_language_chunking() {
    let dir = tempdir().unwrap();

    // Create Python file
    let py_path = dir.path().join("example.py");
    fs::write(
        &py_path,
        r#"
class Calculator:
    def __init__(self):
        self.value = 0

    def add(self, x):
        self.value += x
        return self

def helper_function():
    return 42
"#,
    )
    .unwrap();

    // Create TypeScript file
    let ts_path = dir.path().join("example.ts");
    fs::write(
        &ts_path,
        r#"
interface User {
    id: number;
    name: string;
}

function createUser(name: string): User {
    return { id: Date.now(), name };
}

class UserService {
    constructor(private users: User[] = []) {}
}
"#,
    )
    .unwrap();

    let mut chunker = CodeChunker::new(500, true).unwrap();

    // Chunk Python
    let py_chunks = chunker.chunk_file(&py_path).unwrap();
    assert!(py_chunks.len() >= 2, "Should find Python class and function");

    let py_class = py_chunks
        .iter()
        .find(|c| c.metadata.chunk_type == ChunkType::Class);
    assert!(py_class.is_some(), "Should find Calculator class");
    assert_eq!(py_class.unwrap().metadata.name, "Calculator");

    // Chunk TypeScript
    let ts_chunks = chunker.chunk_file(&ts_path).unwrap();
    assert!(
        ts_chunks.len() >= 2,
        "Should find TypeScript interface, function, and class"
    );

    let ts_interface = ts_chunks
        .iter()
        .find(|c| c.metadata.chunk_type == ChunkType::Interface);
    assert!(ts_interface.is_some(), "Should find User interface");
    assert_eq!(ts_interface.unwrap().metadata.name, "User");
}

/// Test manifest tracks files correctly across updates.
#[test]
fn test_manifest_file_tracking() {
    let mut manifest = IndexManifest::new();

    // Index some files
    manifest.record_indexed(
        PathBuf::from("src/a.rs"),
        "hash_a".to_string(),
        vec!["chunk1".to_string(), "chunk2".to_string()],
    );
    manifest.record_indexed(
        PathBuf::from("src/b.rs"),
        "hash_b".to_string(),
        vec!["chunk3".to_string()],
    );
    manifest.record_indexed(
        PathBuf::from("src/c.rs"),
        "hash_c".to_string(),
        vec!["chunk4".to_string(), "chunk5".to_string()],
    );

    assert_eq!(manifest.total_chunks, 5);
    assert_eq!(manifest.files.len(), 3);

    // Simulate file b.rs being deleted
    let current_files = vec![PathBuf::from("src/a.rs"), PathBuf::from("src/c.rs")];

    let deleted = manifest.find_deleted_files(&current_files);
    assert_eq!(deleted.len(), 1);
    assert!(deleted.contains(&PathBuf::from("src/b.rs")));

    // Remove the deleted file from manifest
    let removed_state = manifest.remove_file(&PathBuf::from("src/b.rs"));
    assert!(removed_state.is_some());
    assert_eq!(removed_state.unwrap().chunk_count, 1);

    assert_eq!(manifest.total_chunks, 4);
    assert_eq!(manifest.files.len(), 2);
}

/// Test that chunk metadata is complete.
#[test]
fn test_chunk_metadata_completeness() {
    let mut chunker = CodeChunker::new(500, true).unwrap();

    let source = r#"
/// Documentation for the function.
pub fn well_documented(arg1: i32, arg2: String) -> Result<bool, Error> {
    // Implementation
    Ok(true)
}
"#;

    let chunks = chunker.chunk_source(source, "src/lib.rs", "rust").unwrap();
    assert!(!chunks.is_empty());

    let chunk = &chunks[0];

    // Verify all metadata fields are populated
    assert_eq!(chunk.metadata.chunk_type, ChunkType::Function);
    assert_eq!(chunk.metadata.name, "well_documented");
    assert!(chunk.metadata.signature.is_some());
    assert!(chunk
        .metadata
        .signature
        .as_ref()
        .unwrap()
        .contains("well_documented"));
    assert!(chunk.metadata.line_start > 0);
    assert!(chunk.metadata.line_end >= chunk.metadata.line_start);
    assert!(!chunk.metadata.content_hash.is_empty());
    assert_eq!(chunk.metadata.language, "rust");

    // Content should contain the function
    assert!(chunk.content.contains("well_documented"));
    assert!(chunk.content.contains("Result<bool, Error>"));
}

/// Test BM25 handles code-specific tokenization.
#[test]
fn test_bm25_code_tokenization() {
    let mut bm25 = BM25Index::new();

    // Add documents with snake_case and camelCase identifiers
    bm25.add_document(
        "snake".to_string(),
        "fn calculate_user_total() {}".to_string(),
    );
    bm25.add_document(
        "camel".to_string(),
        "function calculateUserTotal() {}".to_string(),
    );

    // Search for snake_case identifier (should match with underscores preserved)
    let results = bm25.search("calculate_user_total", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "snake");

    // Search for camelCase identifier
    let results = bm25.search("calculateUserTotal", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "camel");
}

/// Test RRF fusion function directly.
#[test]
fn test_rrf_fusion_ranking() {
    use g3_index::search::reciprocal_rank_fusion;

    // Vector results favor doc1
    let vector_results = vec![
        ("doc1".to_string(), 0.95f32),
        ("doc2".to_string(), 0.85f32),
        ("doc3".to_string(), 0.75f32),
    ];

    // BM25 results favor doc2
    let bm25_results = vec![
        ("doc2".to_string(), 10.0f64),
        ("doc1".to_string(), 8.0f64),
        ("doc4".to_string(), 6.0f64),
    ];

    let fused = reciprocal_rank_fusion(&vector_results, &bm25_results, 60.0, 0.7, 0.3);

    // doc1 and doc2 should be top 2 (both appear in both lists)
    let top_ids: Vec<&str> = fused.iter().take(2).map(|(id, _)| id.as_str()).collect();
    assert!(top_ids.contains(&"doc1"));
    assert!(top_ids.contains(&"doc2"));

    // doc4 should be present (only in BM25)
    let all_ids: Vec<&str> = fused.iter().map(|(id, _)| id.as_str()).collect();
    assert!(all_ids.contains(&"doc4"));
}
