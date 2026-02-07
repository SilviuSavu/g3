# g3 Code Intelligence System

**Last updated**: 2026-02-07
**Source of truth**: `crates/g3-index/src/`, `crates/g3-core/src/tools/intelligence.rs`

## Overview

The g3 Code Intelligence System provides autonomous AI agents with comprehensive codebase analysis capabilities. It integrates three search layers (lexical, vector semantic, AST-aware) with a knowledge graph to enable intelligent code traversal and pattern discovery.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                  Code Intelligence Layer                         │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   Lexical    │  │   Vector     │  │    AST       │          │
│  │    BM25      │  │   Search     │  │  Pattern     │          │
│  │   (Qwen3)    │  │  (Qdrant)    │  │  Matching    │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│          │                 │                 │                   │
│          └─────────────────┼─────────────────┘                   │
│                            │                                      │
│                  ┌─────────────────┐                             │
│                  │  Knowledge      │                             │
│                  │   Graph         │                             │
│                  │  (CodeGraph)    │                             │
│                  └─────────────────┘                             │
│                            │                                      │
│                  ┌─────────────────┐                             │
│                  │  Traverser      │                             │
│                  │  Algorithms     │                             │
│                  │  (BFS/DFS)      │                             │
│                  └─────────────────┘                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                  ┌─────────────────┐
                  │  code_intelligence│
                  │     Tool          │
                  └─────────────────┘
```

## Features

| Feature | Description |
|---------|-------------|
| **Symbol Resolution** | Find definitions, references, callers, callees |
| **Semantic Search** | Vector-based code similarity search |
| **Lexical Search** | BM25 keyword-based code search |
| **AST Pattern Matching** | Tree-sitter based structural search |
| **Graph Traversal** | Autonomous BFS/DFS on dependency graph |
| **Result Fusion** | Reciprocal Rank Fusion (RRF) for combined results |

## Components

### g3-index Modules

| Module | Purpose |
|--------|---------|
| `unified_index.rs` | Unified API for all search strategies |
| `traverser.rs` | Knowledge graph traversal (BFS, DFS, path finding) |
| `integration.rs` | Cross-index connector (LSP ↔ indexed chunks) |
| `graph.rs` | Knowledge graph data model |
| `graph_builder.rs` | Graph construction from code |
| `search/mod.rs` | Hybrid search (vector + BM25 + RRF) |
| `chunker.rs` | AST-based code chunking |

### g3-core Tools

| Tool | Description |
|------|-------------|
| `code_intelligence` | Main intelligence tool with subcommands |
| `index_codebase` | Index codebase for semantic search |
| `semantic_search` | Query indexed code semantically |
| `graph_find_symbol` | Find symbol definitions |
| `graph_find_references` | Find symbol usages |
| `graph_find_callers` | Find callers of a function |
| `graph_stats` | Get graph statistics |

## Agent Tool: code_intelligence

### Overview

The `code_intelligence` tool provides access to all intelligence capabilities through a single interface with subcommands.

### Command Syntax

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "<subcommand>",
    "symbol": "<symbol_name>",
    "depth": 2
  }
}
```

### Subcommands

#### find

Find symbol definitions by name.

**Parameters**:
- `command`: `"find"`
- `symbol`: Symbol name to search for

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "find",
    "symbol": "process_request"
  }
}
```

**Returns**: Array of symbol definitions with file location and line numbers.

---

#### refs

Find all references to a symbol.

**Parameters**:
- `command`: `"refs"`
- `symbol`: Symbol name to find references for

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "refs",
    "symbol": "DatabaseConnection"
  }
}
```

**Returns**: Array of reference locations with source and target symbols.

---

#### callers

Find functions that call a given symbol.

**Parameters**:
- `command`: `"callers"`
- `symbol`: Symbol name to find callers for
- `depth`: (optional) Traversal depth (default: 2)

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "callers",
    "symbol": "main",
    "depth": 3
  }
}
```

**Returns**: Array of caller symbols with their IDs.

---

#### callees

Find functions called by a given symbol.

**Parameters**:
- `command`: `"callees"`
- `symbol`: Symbol name to find callees for
- `depth`: (optional) Traversal depth (default: 2)

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "callees",
    "symbol": "handler",
    "depth": 2
  }
}
```

**Returns**: Array of callee symbols (uses traverser module).

---

#### similar

Find similar code patterns using semantic search.

**Parameters**:
- `command`: `"similar"`
- `symbol`: Natural language query describing the pattern

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "similar",
    "symbol": "error handling in API responses"
  }
}
```

**Returns**: Array of similar code snippets with relevance scores.

---

#### graph

Explore the dependency graph starting from a symbol.

**Parameters**:
- `command`: `"graph"`
- `symbol`: Starting symbol
- `depth`: (optional) Traversal depth (default: 2)

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "graph",
    "symbol": "UserService",
    "depth": 2
  }
}
```

**Returns**: Traversal results showing callers and references at each level.

---

#### query

Query callers and references in a single call.

**Parameters**:
- `command`: `"query"`
- `symbol`: Symbol to query
- `depth`: (optional) Traversal depth (default: 2)

**Example**:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "query",
    "symbol": "AuthService",
    "depth": 4
  }
}
```

**Returns**: Combined callers and references lists.

---

## Graph Traversal

### Traverser API

The `traverser.rs` module provides autonomous graph traversal algorithms.

#### GraphTraverser

```rust
pub struct GraphTraverser {
    config: TraversalConfig,
}
```

**Methods**:

| Method | Description |
|--------|-------------|
| `bfs(graph, start_node)` | Breadth-first search |
| `dfs(graph, start_node)` | Depth-first search |
| `find_paths(graph, start, end, max_paths)` | Find paths between nodes |
| `has_path(graph, start, end)` | Check if path exists |
| `shortest_path(graph, start, end)` | Find shortest path |
| `detect_cycles(graph, start)` | Detect cycles in graph |
| `reachable_nodes(graph, start)` | Get all reachable nodes |
| `extract_subgraph(graph, start, max_distance)` | Extract subgraph |

**TraversalConfig**:
```rust
pub struct TraversalConfig {
    max_depth: usize,      // Maximum traversal depth
    node_types: Vec<String>, // Filter by node types
    edge_kinds: Vec<EdgeKind>, // Filter by edge types
}
```

### Edge Kinds

| Kind | Description |
|------|-------------|
| `Calls` | Function calls another function |
| `References` | Symbol is referenced in code |
| `Imports` | Module imports another module |
| `Extends` | Class extends another class |
| `Implements` | Class implements an interface |

## Cross-Index Integration

### IndexConnector

Links LSP symbols with indexed chunks for complete context.

**Methods**:

| Method | Description |
|--------|-------------|
| `link_lsp_to_chunk(file, line, chunks)` | Link LSP location to chunk |
| `find_chunks_for_symbol(name, file)` | Find chunks for symbol |
| `enrich_result(result, lsp_available)` | Enrich with cross-index data |
| `link_and_enrich(...)` | Link and enrich in one call |
| `get_callers_as_results(...)` | Get callers as results |
| `get_callees_as_results(...)` | Get callees as results |

### EnrichmentConfig

Controls result enrichment behavior.

| Field | Default | Description |
|-------|---------|-------------|
| `fetch_lsp_details` | `true` | Fetch LSP symbol details |
| `fetch_graph_context` | `true` | Fetch graph context |
| `graph_context_depth` | `2` | Graph traversal depth |
| `include_code_snippet` | `true` | Include code snippets |
| `max_snippet_length` | `500` | Max snippet chars |

## Search Strategies

### Query Planning

The `QueryPlanner` automatically selects the optimal search strategy based on query characteristics.

**Query Plan Types**:

| Plan | Use Case |
|------|----------|
| `GraphOnly` | Dependency-focused queries |
| `AstOnly` | Structural pattern matching |
| `Hybrid` | General semantic+lexical search |
| `VectorOnly` | Pure semantic similarity |
| `All` | Comprehensive multi-strategy search |

### Result Fusion

Results from multiple strategies are combined using **Reciprocal Rank Fusion (RRF)**:

```
RRF(score) = sum(1 / (k + rank))
```

Where `k` is a constant (default: 60) that controls the influence of rank vs. score.

## Configuration

Enable the intelligence system in config:

```toml
[index]
enabled = true

[index.embeddings]
api_key = "${OPENROUTER_API_KEY}"
model = "Qwen/Qwen3-Embedding-8B"

[index.qdrant]
url = "http://localhost:6333"
```

## Examples

### Example: Find All Callers of a Function

```bash
g3 "Use the code_intelligence tool to find all callers of 'main' with depth 3"
```

Tool call:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "callers",
    "symbol": "main",
    "depth": 3
  }
}
```

### Example: Find Similar Error Handling Code

```bash
g3 "Find similar error handling patterns in the codebase"
```

Tool call:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "similar",
    "symbol": "error handling in API responses"
  }
}
```

### Example: Explore Dependency Graph

```bash
g3 "Explore the graph starting from UserService to depth 2"
```

Tool call:
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "graph",
    "symbol": "UserService",
    "depth": 2
  }
}
```

## Testing

Run intelligence system tests:

```bash
# Run all g3-core tests (includes intelligence tests)
cargo test -p g3-core

# Run only intelligence system tests
cargo test -p g3-core --test intelligence_system_test

# Run g3-index tests
cargo test -p g3-index

# Run tests for specific modules
cargo test -p g3-index traverser integration unified_index
```

## API Reference

### UnifiedIndex (g3-index)

```rust
pub trait UnifiedIndex {
    async fn search_semantic(&self, query: &str, limit: usize) -> Result<Vec<UnifiedSearchResult>>;
    async fn search_lexical(&self, query: &str, limit: usize) -> Result<Vec<UnifiedSearchResult>>;
    async fn search_ast(&self, query: &str, limit: usize) -> Result<Vec<UnifiedSearchResult>>;
    async fn query_graph(&self, symbol: &str, depth: usize) -> Result<Vec<UnifiedSearchResult>>;
}
```

### IndexClient (g3-core)

```rust
impl IndexClient {
    pub async fn find_symbols_by_name(&self, name: &str) -> Result<Vec<SymbolInfo>>;
    pub async fn find_references(&self, symbol_id: &str) -> Result<Vec<ReferenceInfo>>;
    pub async fn find_callers(&self, symbol_id: &str) -> Result<Vec<String>>;
    pub async fn search(&self, query: &str, limit: usize, file_filter: Option<&str>) -> Result<Vec<SearchResult>>;
}
```

## Limitations

1. **LSP Integration** - Requires running language servers; fallback to indexed search if unavailable
2. **Vector Search** - Requires Qdrant running; fallback to lexical search if unavailable
3. **Graph Depth** - Traversal depth should be limited to prevent infinite loops (recommended: 1-5)
4. **Result Limits** - Large result sets are truncated to prevent context overflow

## Future Enhancements

- Parallel query execution across layers
- Progressive result display (fast results first)
- Query history and learning
- Code example extraction and summarization
- Cross-project knowledge transfer
- Incremental indexing for large codebases

## Troubleshooting

### No Results Returned

1. Run `index_codebase` to ensure the codebase is indexed
2. Check `index_status` for index health
3. Verify the symbol name is spelled correctly
4. Try a broader search query

### LSP Not Available

The system falls back to indexed search when LSP is unavailable. Enable LSP with:

```toml
[index]
lsp_enabled = true
```

### Qdrant Connection Failed

The system falls back to lexical search when vector search is unavailable. Check Qdrant is running:

```bash
# Check Qdrant status
curl http://localhost:6333

# Start Qdrant (Docker)
docker run -p 6333:6333 qdrant/qdrant
```

## See Also

- [Tools Reference](tools.md) - All available tools
- [Code Search Guide](CODE_SEARCH.md) - Tree-sitter query patterns
- [Configuration](configuration.md) - System configuration
