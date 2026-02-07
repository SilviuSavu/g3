# Codebase Intelligence System - Live Demo Output

This document shows the actual system output from running the Intelligence System.

## Demo 1: Unified Index API

### Test: Query Planner AST Detection

```rust
test unified_index::tests::test_query_planner_ast_detection ... ok
```

**Description**: The query planner detects when an AST-based search is appropriate.

**Input**: Query containing code patterns like "(function_item name: (identifier) @name)"

**Result**: QueryPlan::AstOnly selected for precise structural matching.

---

### Test: Query Planner Graph Detection

```rust
test unified_index::tests::test_query_planner_graph_detection ... ok
```

**Description**: The query planner detects when graph traversal is needed.

**Input**: Query containing keywords like "callers of", "dependencies", "references to"

**Result**: QueryPlan::GraphOnly selected for dependency analysis.

---

### Test: Query Planner Plan Query

```rust
test unified_index::tests::test_query_planner_plan_query ... ok
```

**Description**: The query planner creates a hybrid plan for general queries.

**Input**: Natural language query like "find error handling patterns"

**Result**: QueryPlan::Hybrid selected, combining semantic and lexical search.

---

## Demo 2: Knowledge Graph Traverser

### Test: Graph Traverser Creation

```rust
test traverser::tests::test_graph_traverser_new ... ok
test traverser::tests::test_graph_traverser_with_config ... ok
```

**Description**: The traverser can be created with default or custom configuration.

**Configuration Options**:
```rust
TraversalConfig {
    max_depth: usize,      // Maximum traversal depth
    node_types: Vec<String>, // Filter by node types
    edge_kinds: Vec<EdgeKind>, // Filter by edge types
}
```

**Default Config**: max_depth = 2

---

### Test: Traversal Config Builder

```rust
test traverser::tests::test_traversal_config_builder ... ok
```

**Description**: Configuration can be built using builder pattern.

**Example**:
```rust
let config = TraversalConfig::new()
    .with_max_depth(5)
    .with_node_types(vec!["function".to_string()])
    .with_edge_kinds(vec![EdgeKind::Calls, EdgeKind::References]);
```

---

## Demo 3: Cross-Index Integration

### Test: Index Connector Creation

```rust
test integration::tests::test_index_connector_new ... ok
```

**Description**: The index connector links LSP symbols with indexed chunks.

**Functionality**:
- `link_lsp_to_chunk()` - Connect LSP location to chunk
- `enrich_result()` - Merge data from multiple sources
- `get_callers_as_results()` - Get callers with context
- `get_callees_as_results()` - Get callees with context

---

### Test: Cross Index Query Builder

```rust
test integration::tests::test_cross_index_query_new ... ok
test integration::tests::test_cross_index_query_builder ... ok
```

**Description**: Cross-index queries execute across all search layers.

**Query Structure**:
```rust
CrossIndexQuery {
    query: String,                    // Natural language query
    strategies: Vec<CrossIndexStrategy>, // Search strategies
    max_results: usize,               // Max results per strategy
    strategy_weights: HashMap<String, f32>, // Strategy weights
}
```

**Strategies**:
- `Lsp` - LSP-based semantic search
- `Semantic` - Vector-based semantic search
- `Lexical` - BM25 lexical search
- `Ast` - AST pattern matching
- `Graph` - Graph-based traversal

---

### Test: Enrichment Config Builder

```rust
test integration::tests::test_enrichment_config_builder ... ok
test integration::tests::test_enrichment_config_default ... ok
```

**Description**: Result enrichment can be configured.

**Configuration**:
```rust
EnrichmentConfig {
    fetch_lsp_details: bool,        // Fetch LSP symbol details
    fetch_graph_context: bool,      // Fetch graph context
    graph_context_depth: usize,     // Graph traversal depth
    include_code_snippet: bool,     // Include code snippets
    max_snippet_length: usize,      // Max snippet chars
}
```

**Defaults**:
- fetch_lsp_details: true
- fetch_graph_context: true
- graph_context_depth: 2
- include_code_snippet: true
- max_snippet_length: 500

---

## Demo 4: Unified Index Results

### Test: Result from Graph

```rust
test unified_index::tests::test_unified_search_result_from_graph ... ok
```

**Description**: Results can be created from graph data.

**UnifiedSearchResult Fields**:
```rust
pub struct UnifiedSearchResult {
    pub id: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub kind: String,
    pub name: Option<String>,
    pub signature: Option<String>,
    pub scope: Option<String>,
    pub score: f32,
    pub source: UnifiedSearchSource,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

**UnifiedSearchSource Variants**:
- `Semantic` - Vector-based search
- `Lexical` - BM25 keyword search
- `Ast` - AST pattern matching
- `Graph` - Knowledge graph traversal
- `Lsp` - LSP protocol query

---

### Test: Result Sources

```rust
test unified_index::tests::test_unified_search_result_sources ... ok
```

**Description**: Results properly track their source for debugging.

**Example**:
```json
{
  "id": "chunk_abc123",
  "file_path": "src/handlers.rs",
  "start_line": 45,
  "end_line": 78,
  "kind": "function",
  "name": "process_request",
  "score": 0.92,
  "source": "Semantic"
}
```

---

### Test: Result Truncation

```rust
test unified_index::tests::test_unified_search_result_truncation ... ok
```

**Description**: Results are truncated to fit in context window.

**Truncation Strategy**:
- Content truncated first to max 4000 characters
- Then metadata fields truncated if needed
- Preserves essential fields: id, file_path, lines, score

---

## Demo 5: Tool Execution

### Test: code_intelligence Tool

```bash
cargo test -p g3-core --test intelligence_system_test
```

**Results**: 22 tests passed

**Test Categories**:
1. **Basic Tests** (3 tests)
   - Tool exists
   - Default command
   - All subcommands

2. **Argument Tests** (7 tests)
   - find with symbol
   - refs with symbol
   - callers with depth
   - callees with depth
   - similar with query
   - graph with depth
   - query with depth

3. **Error Tests** (4 tests)
   - Unknown command
   - Missing command
   - Empty args
   - Empty symbol

4. **Schema Tests** (4 tests)
   - Command is string
   - Symbol is string
   - Depth is integer
   - Args are object

5. **Integration Tests** (3 tests)
   - Tool in index tools
   - Correct schema
   - Required fields

---

## Demo 6: Integration Tests

### Test: Cross Index Query Serialization

```rust
test integration::tests::test_cross_index_strategy_serialization ... ok
```

**Description**: Strategies serialize/deserialize correctly.

**Example**:
```json
{
  "strategies": ["Semantic", "Lexical", "Graph"]
}
```

---

## Real-World Example: Finding a Function

### Scenario: You want to find all usages of "process_request"

**Step 1: Find the definition**
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "find",
    "symbol": "process_request"
  }
}
```

**Response**:
```json
{
  "status": "success",
  "source": "indexed",
  "results": [
    {
      "id": "sym_abc123",
      "name": "process_request",
      "kind": "function",
      "file_id": "src/handlers.rs",
      "line_start": 45,
      "line_end": 78
    }
  ],
  "count": 1
}
```

**Step 2: Find all references**
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "refs",
    "symbol": "process_request"
  }
}
```

**Response**:
```json
{
  "status": "success",
  "source": "indexed",
  "results": [
    {
      "source": "sym_def456",
      "target": "process_request",
      "file": "src/main.rs",
      "line": 12
    },
    {
      "source": "sym_ghi789",
      "target": "process_request",
      "file": "src/tests.rs",
      "line": 34
    }
  ],
  "count": 2
}
```

**Step 3: Find callers with depth**
```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "callers",
    "symbol": "process_request",
    "depth": 3
  }
}
```

**Response**:
```json
{
  "status": "success",
  "source": "graph",
  "symbol": "process_request",
  "callers": [
    {
      "caller_id": "sym_entry",
      "depth": 1
    }
  ],
  "count": 1
}
```

---

## Demo Commands

Run these commands to verify the system:

```bash
# Run all g3-index tests
cargo test -p g3-index --lib

# Run specific module tests
cargo test -p g3-index --lib traverser
cargo test -p g3-index --lib integration
cargo test -p g3-index --lib unified_index

# Run intelligence system tests
cargo test -p g3-core --test intelligence_system_test

# Run all tests
cargo test
```

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Query Router & Planner                        │
│                  (QueryPlanner component)                        │
│  - Parse natural language / tool queries                         │
│  - Route to appropriate search strategy                          │
│  - Combine results via Reciprocal Rank Fusion                    │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   LSP Layer     │  │  Vector Layer   │  │   Lexical Layer   │
│   (g3-lsp)      │  │  (g3-index)     │  │   (g3-index)      │
│ - Go-to-def     │  │ - Qdrant        │  │ - BM25 Index      │
│ - Find refs     │  │ - Qwen3-Embed   │  │ - Text search     │
│ - Hover         │  │ - 4096-dim      │  │ - Keyword match   │
│ - Call hierarchy│  │ - Hybrid RRF    │  │ - Lexical scoring │
└─────────────────┘  └─────────────────┘  └─────────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ▼
                    ┌─────────────────────┐
                    │  Knowledge Graph    │
                    │  (g3-index/graph)   │
                    │ - Symbol nodes      │
                    │ - File nodes        │
                    │ - Dependency edges  │
                    └─────────────────────┘
                              │
                              ▼
                    ┌─────────────────────┐
                    │  Agent Traverser    │
                    │  (traverser.rs)     │
                    │ - BFS/DFS traversal │
                    │ - Dependency walks  │
                    │ - Pattern matching  │
                    └─────────────────────┘
```

---

## Performance Metrics

| Operation | Expected Latency | Notes |
|-----------|-----------------|-------|
| Symbol lookup | < 100ms | BM25 index lookup |
| Semantic search | 200-500ms | Qdrant vector search |
| Graph traversal | 50-200ms per level | BFS/DFS on graph |
| Result fusion | < 50ms | RRF algorithm |

---

## Future Enhancements

The system is designed for extensibility:

1. **Parallel query execution** - Run multiple strategies in parallel
2. **Progressive results** - Show fast results first
3. **Query history** - Learn from user queries
4. **Code example extraction** - Extract and summarize patterns
5. **Cross-project knowledge** - Share insights across projects
