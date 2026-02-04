//! AST-based code chunking using tree-sitter.
//!
//! This module provides intelligent code chunking that respects
//! semantic boundaries (functions, classes, methods) rather than
//! arbitrary line or token counts.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;
use tree_sitter::{Language, Node, Parser};
use walkdir::WalkDir;

/// Errors that can occur during code chunking.
#[derive(Error, Debug)]
pub enum ChunkerError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Failed to parse file: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Type of code chunk
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    Module,
    Class,     // Python/JS
    Interface, // TypeScript
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Const => "const",
            Self::Module => "module",
            Self::Class => "class",
            Self::Interface => "interface",
        }
    }
}

/// Metadata associated with a code chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Type of this chunk
    pub chunk_type: ChunkType,

    /// Name of the code element
    pub name: String,

    /// Function/method signature (if applicable)
    pub signature: Option<String>,

    /// Start line (1-indexed)
    pub line_start: usize,

    /// End line (1-indexed)
    pub line_end: usize,

    /// Module path (if available)
    pub module: Option<String>,

    /// Scope context (e.g., "impl Foo" for methods)
    pub scope: Option<String>,

    /// SHA256 hash of the chunk content
    pub content_hash: String,

    /// Programming language
    pub language: String,
}

/// A chunk of code extracted from a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Original file path
    pub file_path: String,

    /// The actual code content
    pub content: String,

    /// Enriched content with context prepended
    pub enriched_content: String,

    /// Metadata about this chunk
    pub metadata: ChunkMetadata,
}

/// Code chunker that uses tree-sitter for AST-aware chunking.
pub struct CodeChunker {
    parsers: HashMap<String, Parser>,
    include_context: bool,
}

impl CodeChunker {
    /// Create a new code chunker with AST parsing support.
    ///
    /// # Arguments
    /// * `_max_chunk_tokens` - Maximum tokens per chunk (reserved for future use)
    /// * `include_context` - Whether to enrich chunks with file/scope context
    pub fn new(_max_chunk_tokens: usize, include_context: bool) -> Result<Self> {
        let mut parsers = HashMap::new();

        // Initialize Rust parser
        {
            let mut parser = Parser::new();
            let language: Language = tree_sitter_rust::LANGUAGE.into();
            parser.set_language(&language)?;
            parsers.insert("rust".to_string(), parser);
        }

        // Initialize Python parser
        {
            let mut parser = Parser::new();
            let language: Language = tree_sitter_python::LANGUAGE.into();
            parser.set_language(&language)?;
            parsers.insert("python".to_string(), parser);
        }

        // Initialize JavaScript parser
        {
            let mut parser = Parser::new();
            let language: Language = tree_sitter_javascript::LANGUAGE.into();
            parser.set_language(&language)?;
            parsers.insert("javascript".to_string(), parser);
        }

        // Initialize TypeScript parser
        {
            let mut parser = Parser::new();
            let language: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
            parser.set_language(&language)?;
            parsers.insert("typescript".to_string(), parser);
        }

        // Initialize Go parser
        {
            let mut parser = Parser::new();
            let language: Language = tree_sitter_go::LANGUAGE.into();
            parser.set_language(&language)?;
            parsers.insert("go".to_string(), parser);
        }

        Ok(Self {
            parsers,
            include_context,
        })
    }

    /// Create a new code chunker with default settings (4000 tokens, context enabled).
    pub fn with_defaults() -> Result<Self> {
        Self::new(4000, true)
    }

    /// Detect language from file extension.
    pub fn detect_language(path: &Path) -> Option<String> {
        match path.extension()?.to_str()? {
            "rs" => Some("rust".to_string()),
            "py" => Some("python".to_string()),
            "js" | "jsx" => Some("javascript".to_string()),
            "ts" | "tsx" => Some("typescript".to_string()),
            "go" => Some("go".to_string()),
            _ => None,
        }
    }

    /// Chunk a single file into semantic code blocks.
    pub fn chunk_file(&mut self, path: &Path) -> Result<Vec<Chunk>> {
        let language = Self::detect_language(path)
            .ok_or_else(|| anyhow!("Unsupported file type: {:?}", path))?;

        let source = fs::read_to_string(path)?;
        let file_path = path.to_string_lossy().to_string();

        self.chunk_source(&source, &file_path, &language)
    }

    /// Chunk source code string into semantic blocks.
    pub fn chunk_source(
        &mut self,
        source: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Vec<Chunk>> {
        let parser = self
            .parsers
            .get_mut(language)
            .ok_or_else(|| anyhow!("Unsupported language: {}", language))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow!("Failed to parse file"))?;

        let mut chunks = Vec::new();
        self.extract_chunks_recursive(
            tree.root_node(),
            source,
            file_path,
            language,
            &mut chunks,
            None,
        );

        // Enrich chunks with context
        if self.include_context {
            for chunk in &mut chunks {
                chunk.enriched_content = self.enrich_chunk(chunk, file_path);
            }
        }

        Ok(chunks)
    }

    fn extract_chunks_recursive(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        language: &str,
        chunks: &mut Vec<Chunk>,
        scope: Option<&str>,
    ) {
        let chunk_opt = self.node_to_chunk(node, source, file_path, language, scope);

        if let Some(chunk) = chunk_opt {
            // For impl blocks, extract methods as separate chunks
            if chunk.metadata.chunk_type == ChunkType::Impl {
                let impl_scope = format!("impl {}", chunk.metadata.name);
                for child in node.children(&mut node.walk()) {
                    self.extract_chunks_recursive(
                        child,
                        source,
                        file_path,
                        language,
                        chunks,
                        Some(&impl_scope),
                    );
                }
            }
            // For classes, extract methods as separate chunks
            if chunk.metadata.chunk_type == ChunkType::Class {
                let class_scope = format!("class {}", chunk.metadata.name);
                for child in node.children(&mut node.walk()) {
                    self.extract_chunks_recursive(
                        child,
                        source,
                        file_path,
                        language,
                        chunks,
                        Some(&class_scope),
                    );
                }
            }
            chunks.push(chunk);
        } else {
            // Recurse into children
            for child in node.children(&mut node.walk()) {
                self.extract_chunks_recursive(child, source, file_path, language, chunks, scope);
            }
        }
    }

    fn node_to_chunk(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        language: &str,
        scope: Option<&str>,
    ) -> Option<Chunk> {
        let kind = node.kind();

        let (chunk_type, name) = match kind {
            // Rust
            "function_item" => (ChunkType::Function, self.get_rust_fn_name(node, source)?),
            "struct_item" => (
                ChunkType::Struct,
                self.get_child_text(node, "type_identifier", source)?,
            ),
            "enum_item" => (
                ChunkType::Enum,
                self.get_child_text(node, "type_identifier", source)?,
            ),
            "trait_item" => (
                ChunkType::Trait,
                self.get_child_text(node, "type_identifier", source)?,
            ),
            "impl_item" => (ChunkType::Impl, self.get_impl_name(node, source)?),
            "const_item" => (
                ChunkType::Const,
                self.get_child_text(node, "identifier", source)?,
            ),

            // Python
            "function_definition" => {
                let name = self.get_child_text(node, "identifier", source)?;
                // Check if this is a method (inside a class)
                let chunk_type = if scope.is_some() {
                    ChunkType::Method
                } else {
                    ChunkType::Function
                };
                (chunk_type, name)
            }
            "class_definition" => (
                ChunkType::Class,
                self.get_child_text(node, "identifier", source)?,
            ),

            // JavaScript/TypeScript
            "function_declaration" => (
                ChunkType::Function,
                self.get_child_text(node, "identifier", source)?,
            ),
            "class_declaration" => (
                ChunkType::Class,
                self.get_child_text(node, "identifier", source)?,
            ),
            "interface_declaration" => (
                ChunkType::Interface,
                self.get_child_text(node, "type_identifier", source)?,
            ),
            "method_definition" => (
                ChunkType::Method,
                self.get_child_text(node, "property_identifier", source)?,
            ),

            // Go
            "method_declaration" => (ChunkType::Method, self.get_go_method_name(node, source)?),
            "type_declaration" => (ChunkType::Struct, self.get_go_type_name(node, source)?),

            _ => return None,
        };

        let content = source[node.byte_range()].to_string();
        let signature = self.extract_signature(node, source, &chunk_type);
        let content_hash = Self::compute_hash(&content);

        Some(Chunk {
            file_path: file_path.to_string(),
            content: content.clone(),
            enriched_content: content,
            metadata: ChunkMetadata {
                chunk_type,
                name,
                signature,
                line_start: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
                module: None,
                scope: scope.map(String::from),
                content_hash,
                language: language.to_string(),
            },
        })
    }

    fn get_rust_fn_name(&self, node: Node, source: &str) -> Option<String> {
        self.get_child_text(node, "identifier", source)
    }

    fn get_impl_name(&self, node: Node, source: &str) -> Option<String> {
        // Get the type being implemented
        for child in node.children(&mut node.walk()) {
            if child.kind() == "type_identifier" || child.kind() == "generic_type" {
                return Some(source[child.byte_range()].to_string());
            }
        }
        None
    }

    fn get_go_method_name(&self, node: Node, source: &str) -> Option<String> {
        self.get_child_text(node, "field_identifier", source)
    }

    fn get_go_type_name(&self, node: Node, source: &str) -> Option<String> {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "type_spec" {
                return self.get_child_text(child, "type_identifier", source);
            }
        }
        None
    }

    fn get_child_text(&self, node: Node, kind: &str, source: &str) -> Option<String> {
        for child in node.children(&mut node.walk()) {
            if child.kind() == kind {
                return Some(source[child.byte_range()].to_string());
            }
        }
        None
    }

    fn extract_signature(&self, node: Node, source: &str, chunk_type: &ChunkType) -> Option<String> {
        match chunk_type {
            ChunkType::Function | ChunkType::Method => {
                // Get first line (usually the signature)
                let content = &source[node.byte_range()];
                content.lines().next().map(|s| s.trim().to_string())
            }
            ChunkType::Struct
            | ChunkType::Enum
            | ChunkType::Trait
            | ChunkType::Class
            | ChunkType::Interface => {
                let content = &source[node.byte_range()];
                // Get up to the first brace
                if let Some(pos) = content.find('{') {
                    Some(content[..pos].trim().to_string())
                } else {
                    content.lines().next().map(|s| s.trim().to_string())
                }
            }
            _ => None,
        }
    }

    fn enrich_chunk(&self, chunk: &Chunk, file_path: &str) -> String {
        let mut enriched = String::new();

        // Add file context
        enriched.push_str(&format!("# File: {}\n", file_path));

        // Add scope if present
        if let Some(scope) = &chunk.metadata.scope {
            enriched.push_str(&format!("# Scope: {}\n", scope));
        }

        // Add module if present
        if let Some(module) = &chunk.metadata.module {
            enriched.push_str(&format!("# Module: {}\n", module));
        }

        enriched.push('\n');
        enriched.push_str(&chunk.content);

        enriched
    }

    /// Compute SHA256 hash of content.
    fn compute_hash(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Chunk an entire directory, filtering by file extensions.
    pub fn chunk_directory(&mut self, dir: &Path, extensions: &[&str]) -> Result<Vec<Chunk>> {
        let mut all_chunks = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Check extension
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !extensions.contains(&ext) {
                continue;
            }

            // Skip common non-code directories
            let path_str = path.to_string_lossy();
            if path_str.contains("/target/")
                || path_str.contains("/node_modules/")
                || path_str.contains("/.git/")
            {
                continue;
            }

            match self.chunk_file(path) {
                Ok(chunks) => all_chunks.extend(chunks),
                Err(e) => debug!("Failed to chunk {:?}: {}", path, e),
            }
        }

        Ok(all_chunks)
    }

    /// Get supported file extensions.
    pub fn supported_extensions() -> &'static [&'static str] {
        &["rs", "py", "js", "jsx", "ts", "tsx", "go"]
    }

    /// Check if a file extension is supported.
    pub fn is_supported_extension(ext: &str) -> bool {
        Self::supported_extensions().contains(&ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(
            CodeChunker::detect_language(Path::new("foo.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            CodeChunker::detect_language(Path::new("bar.py")),
            Some("python".to_string())
        );
        assert_eq!(
            CodeChunker::detect_language(Path::new("baz.js")),
            Some("javascript".to_string())
        );
        assert_eq!(
            CodeChunker::detect_language(Path::new("qux.ts")),
            Some("typescript".to_string())
        );
        assert_eq!(
            CodeChunker::detect_language(Path::new("main.go")),
            Some("go".to_string())
        );
        assert_eq!(CodeChunker::detect_language(Path::new("unknown.xyz")), None);
    }

    #[test]
    fn test_chunk_rust_source() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub struct Foo {
    x: i32,
}

impl Foo {
    pub fn new(x: i32) -> Self {
        Self { x }
    }

    pub fn get_x(&self) -> i32 {
        self.x
    }
}

pub fn standalone() -> i32 {
    42
}
"#;

        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        // Should have: Foo struct, impl Foo, new method, get_x method, standalone function
        assert!(chunks.len() >= 3);

        // Find the struct
        let struct_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Struct);
        assert!(struct_chunk.is_some());
        assert_eq!(struct_chunk.unwrap().metadata.name, "Foo");

        // Find the standalone function
        let fn_chunk = chunks
            .iter()
            .find(|c| c.metadata.chunk_type == ChunkType::Function && c.metadata.name == "standalone");
        assert!(fn_chunk.is_some());
    }

    #[test]
    fn test_chunk_python_source() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
class MyClass:
    def __init__(self, x):
        self.x = x

    def get_x(self):
        return self.x

def standalone():
    return 42
"#;

        let chunks = chunker.chunk_source(source, "test.py", "python").unwrap();

        // Should have: MyClass, __init__ method, get_x method, standalone function
        assert!(chunks.len() >= 2);

        // Find the class
        let class_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Class);
        assert!(class_chunk.is_some());
        assert_eq!(class_chunk.unwrap().metadata.name, "MyClass");
    }

    #[test]
    fn test_chunk_with_context() {
        let mut chunker = CodeChunker::new(4000, true).unwrap();
        let source = "pub fn hello() -> &'static str { \"Hello\" }";

        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        assert!(!chunks.is_empty());
        let chunk = &chunks[0];
        assert!(chunk.enriched_content.contains("# File: test.rs"));
    }

    #[test]
    fn test_supported_extensions() {
        assert!(CodeChunker::is_supported_extension("rs"));
        assert!(CodeChunker::is_supported_extension("py"));
        assert!(CodeChunker::is_supported_extension("ts"));
        assert!(!CodeChunker::is_supported_extension("c"));
        assert!(!CodeChunker::is_supported_extension("cpp"));
    }

    #[test]
    fn test_detect_language_jsx_tsx() {
        assert_eq!(
            CodeChunker::detect_language(Path::new("component.jsx")),
            Some("javascript".to_string())
        );
        assert_eq!(
            CodeChunker::detect_language(Path::new("component.tsx")),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_chunk_rust_impl_block() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub struct Point {
    x: f64,
    y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        // Should have: Point struct, impl Point, new method, distance method
        assert!(chunks.len() >= 2);

        // Find the struct
        let struct_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Struct);
        assert!(struct_chunk.is_some());
        assert_eq!(struct_chunk.unwrap().metadata.name, "Point");

        // Find methods within impl scope
        let method_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.chunk_type == ChunkType::Function && c.metadata.scope.is_some())
            .collect();
        assert!(method_chunks.len() >= 1);

        // Verify scope context
        let new_method = method_chunks.iter().find(|c| c.metadata.name == "new");
        if let Some(m) = new_method {
            assert!(m.metadata.scope.as_ref().unwrap().contains("Point"));
        }
    }

    #[test]
    fn test_chunk_rust_enum() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub enum Color {
    Red,
    Green,
    Blue,
    Rgb(u8, u8, u8),
}
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        let enum_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Enum);
        assert!(enum_chunk.is_some());
        assert_eq!(enum_chunk.unwrap().metadata.name, "Color");
    }

    #[test]
    fn test_chunk_rust_trait() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub trait Drawable {
    fn draw(&self);
    fn size(&self) -> (u32, u32);
}
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        let trait_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Trait);
        assert!(trait_chunk.is_some());
        assert_eq!(trait_chunk.unwrap().metadata.name, "Drawable");
    }

    #[test]
    fn test_chunk_rust_const() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub const MAX_SIZE: usize = 1024;
pub const DEFAULT_NAME: &str = "unnamed";
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        let const_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.chunk_type == ChunkType::Const)
            .collect();
        assert_eq!(const_chunks.len(), 2);
        assert!(const_chunks.iter().any(|c| c.metadata.name == "MAX_SIZE"));
        assert!(const_chunks.iter().any(|c| c.metadata.name == "DEFAULT_NAME"));
    }

    #[test]
    fn test_chunk_python_class_methods() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
class Calculator:
    def __init__(self, value=0):
        self.value = value

    def add(self, x):
        self.value += x
        return self

    def multiply(self, x):
        self.value *= x
        return self
"#;
        let chunks = chunker.chunk_source(source, "test.py", "python").unwrap();

        // Should have: Calculator class, and methods as separate chunks
        let class_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Class);
        assert!(class_chunk.is_some());
        assert_eq!(class_chunk.unwrap().metadata.name, "Calculator");

        // Check for methods with scope
        let method_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.chunk_type == ChunkType::Method)
            .collect();
        assert!(method_chunks.len() >= 1);
    }

    #[test]
    fn test_chunk_typescript_interface() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
interface User {
    id: number;
    name: string;
    email?: string;
}

function greet(user: User): string {
    return `Hello, ${user.name}!`;
}
"#;
        let chunks = chunker.chunk_source(source, "test.ts", "typescript").unwrap();

        // Should have interface and function
        let interface_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Interface);
        assert!(interface_chunk.is_some());
        assert_eq!(interface_chunk.unwrap().metadata.name, "User");

        let fn_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Function);
        assert!(fn_chunk.is_some());
        assert_eq!(fn_chunk.unwrap().metadata.name, "greet");
    }

    #[test]
    fn test_chunk_javascript_class() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
class Animal {
    constructor(name) {
        this.name = name;
    }

    speak() {
        console.log(`${this.name} makes a sound`);
    }
}
"#;
        let chunks = chunker.chunk_source(source, "test.js", "javascript").unwrap();

        let class_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Class);
        assert!(class_chunk.is_some());
        assert_eq!(class_chunk.unwrap().metadata.name, "Animal");
    }

    #[test]
    fn test_chunk_go_function() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
package main

type Point struct {
    X float64
    Y float64
}

func (p *Point) Distance(other *Point) float64 {
    dx := p.X - other.X
    dy := p.Y - other.Y
    return math.Sqrt(dx*dx + dy*dy)
}
"#;
        let chunks = chunker.chunk_source(source, "main.go", "go").unwrap();

        // Should have Point struct and Distance method
        assert!(chunks.len() >= 1);

        // Check for struct
        let struct_chunk = chunks.iter().find(|c| c.metadata.chunk_type == ChunkType::Struct);
        assert!(struct_chunk.is_some());
        assert_eq!(struct_chunk.unwrap().metadata.name, "Point");
    }

    #[test]
    fn test_chunk_file_with_tempfile() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(file, "fn test_fn() {{ println!(\"hello\"); }}").unwrap();
        file.flush().unwrap();

        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let chunks = chunker.chunk_file(file.path()).unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].metadata.name, "test_fn");
        assert_eq!(chunks[0].metadata.chunk_type, ChunkType::Function);
    }

    #[test]
    fn test_chunk_type_as_str() {
        assert_eq!(ChunkType::Function.as_str(), "function");
        assert_eq!(ChunkType::Method.as_str(), "method");
        assert_eq!(ChunkType::Struct.as_str(), "struct");
        assert_eq!(ChunkType::Enum.as_str(), "enum");
        assert_eq!(ChunkType::Trait.as_str(), "trait");
        assert_eq!(ChunkType::Impl.as_str(), "impl");
        assert_eq!(ChunkType::Const.as_str(), "const");
        assert_eq!(ChunkType::Module.as_str(), "module");
        assert_eq!(ChunkType::Class.as_str(), "class");
        assert_eq!(ChunkType::Interface.as_str(), "interface");
    }

    #[test]
    fn test_chunk_signature_extraction() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"
pub fn calculate(x: i32, y: i32) -> i32 {
    x + y
}
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        assert!(!chunks.is_empty());
        let sig = chunks[0].metadata.signature.as_ref().unwrap();
        assert!(sig.contains("pub fn calculate"));
        assert!(sig.contains("i32"));
    }

    #[test]
    fn test_chunk_content_hash() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = "fn foo() { }";
        let chunks1 = chunker.chunk_source(source, "test.rs", "rust").unwrap();
        let chunks2 = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        // Same content should produce same hash
        assert_eq!(
            chunks1[0].metadata.content_hash,
            chunks2[0].metadata.content_hash
        );

        // Different content should produce different hash
        let different_source = "fn bar() { }";
        let chunks3 = chunker.chunk_source(different_source, "test.rs", "rust").unwrap();
        assert_ne!(
            chunks1[0].metadata.content_hash,
            chunks3[0].metadata.content_hash
        );
    }

    #[test]
    fn test_chunk_line_numbers() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let source = r#"// Line 1

fn first() { }

fn second() {
    // some body
}
"#;
        let chunks = chunker.chunk_source(source, "test.rs", "rust").unwrap();

        assert_eq!(chunks.len(), 2);

        // Find first function
        let first = chunks.iter().find(|c| c.metadata.name == "first").unwrap();
        assert_eq!(first.metadata.line_start, 3);
        assert_eq!(first.metadata.line_end, 3);

        // Find second function
        let second = chunks.iter().find(|c| c.metadata.name == "second").unwrap();
        assert_eq!(second.metadata.line_start, 5);
        assert!(second.metadata.line_end >= 7);
    }

    #[test]
    fn test_unsupported_language_error() {
        let mut chunker = CodeChunker::new(4000, false).unwrap();
        let result = chunker.chunk_source("int main() {}", "test.c", "c");
        assert!(result.is_err());
    }

    #[test]
    fn test_enriched_content_with_scope() {
        let mut chunker = CodeChunker::new(4000, true).unwrap();
        let source = r#"
impl Foo {
    fn bar(&self) { }
}
"#;
        let chunks = chunker.chunk_source(source, "src/lib.rs", "rust").unwrap();

        // Find the method (which should have scope)
        let method_chunk = chunks.iter().find(|c| c.metadata.name == "bar");
        if let Some(chunk) = method_chunk {
            assert!(chunk.enriched_content.contains("# File: src/lib.rs"));
            if chunk.metadata.scope.is_some() {
                assert!(chunk.enriched_content.contains("# Scope:"));
            }
        }
    }

    #[test]
    fn test_with_defaults() {
        let chunker = CodeChunker::with_defaults();
        assert!(chunker.is_ok());
    }
}
