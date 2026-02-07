# Plan: Codebase Intelligence System

## Task Description

Create a comprehensive codebase intelligence system that integrates Language Server Protocol (LSP) static analysis with a tri-layered search infrastructure combining lexical codebase indexing, dense vector semantic retrieval, and abstract syntax tree (AST)-aware code search to construct a navigable, dependency-resolved knowledge graph of the repository. This system will enable autonomous AI agents to independently traverse complex code architectures, resolve cross-references, and locate relevant implementation patterns without manual guidance.

## Objective

Build a unified codebase intelligence layer that allows AI agents to:

1. ** statically analyze code structure** using LSP protocol for real-time symbol resolution
2. **index and search code semantically** using dense vector embeddings (Qwen3-Embedding-8B)
3. **perform lexical keyword search** using BM25 for precise term matching
4. **navigate AST-aware code chunks** for structural understanding
5. **traverse the knowledge graph** to discover dependencies, call hierarchies, and cross-references

When complete, agents will be able to answer complex queries like:
- "Find all implementations of trait X"
- "Trace the call chain from function A to function B"
- "Identify similar code patterns across the codebase"
- "List all files that depend on module Y"

## Problem Statement

The current codebase has several **disconnected components**:

1. **g3-lsp** - LSP client manager for go-to-definition, find-references, hover
2. **g3-index** - AST chunking + vector embeddings (Qdrant) + BM25 indexing
3. **g3-core/index_client** - High-level interface to indexing functionality
4. **g3-core/code_search** - Tree-sitter based AST pattern matching

These components exist in isolation with no unified interface for autonomous agents to leverage them cooperatively. The knowledge graph exists but lacks:
- Real-time LSP augmented symbol resolution
- Cross-index graph connections (LSP symbols ↔ indexed chunks)
- Autonomous traversal algorithms for dependency resolution

## Solution Approach

**Three-Layer Integration Architecture:**

```
┌─────────────────────────────────────────────────────────────────┐
│                    Query Router & Planner                        │
│                  (New: QueryPlanner component)                   │
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
                    │  (New)              │
                    │ - BFS/DFS traversal │
                    │ - Dependency walks  │
                    │ - Pattern matching  │
                    └─────────────────────┘
```

**Key Components to Build:**

1. **QueryPlanner** - Routes queries to appropriate search strategies
2. **Unified Index API** - Single interface covering LSP + vector + lexical + graph
3. **KnowledgeGraphTraverser** - Algorithms for autonomous graph traversal
4. **Cross-Index Connector** - Links LSP symbols with indexed chunks
5. **Agent Tool Integration** - Exposes intelligence via tool calls

## Relevant Files

### Existing Files (Leverage)

| File | Purpose |
|------|---------|
| `crates/g3-lsp/src/manager.rs` | Multi-server LSP manager |
| `crates/g3-lsp/src/client.rs` | Single LSP server client |
| `crates/g3-lsp/src/discovery.rs` | Language detection & server discovery |
| `crates/g3-index/src/indexer.rs` | Codebase indexing orchestrator |
| `crates/g3-index/src/graph_builder.rs` | Knowledge graph construction |
| `crates/g3-index/src/graph.rs` | Graph data model (nodes, edges) |
| `crates/g3-index/src/search/mod.rs` | Hybrid search (vector + BM25 + RRF) |
| `crates/g3-index/src/chunker.rs` | AST-based code chunking |
| `crates/g3-core/src/index_client.rs` | High-level index client API |
| `crates/g3-core/src/code_search/searcher.rs` | Tree-sitter pattern matching |
| `crates/g3-core/src/tools/lsp.rs` | LSP tools for agent execution |

### New Files to Create

| File | Purpose |
|------|---------|
| `crates/g3-index/src/planner.rs` | Query planning and routing |
| `crates/g3-index/src/unified_index.rs` | Unified API for all search strategies |
| `crates/g3-index/src/traverser.rs` | Knowledge graph traversal algorithms |
| `crates/g3-index/src/integration.rs` | Cross-index connector (LSP ↔ chunks) |
| `crates/g3-core/src/tools/intelligence.rs` | Agent tool for codebase intelligence |
| `crates/g3-cli/src/commands/intelligence.rs` | CLI intelligence commands |

## Implementation Phases

### Phase 1: Foundation - Unified Index API

Create a unified API that abstracts all search capabilities behind a single interface.

**Key Tasks:**
- Design the `UnifiedIndex` trait with methods for each search type
- Implement query routing based on query characteristics
- Create shared result types for consistent data handling

### Phase 2: Knowledge Graph Traversal

Build autonomous traversal algorithms for dependency resolution.

**Key Tasks:**
- Implement BFS/DFS traversal with configurable depth
- Create dependency walks (callers, callees, references)
- Add pattern matching for AST-aware code search

### Phase 3: Cross-Index Integration

Connect LSP symbols with indexed chunks for complete context.

**Key Tasks:**
- Build index connector to link LSP results with chunk metadata
- Implement result enrichment with cross-index data
- Create unified result formatter

### Phase 4: Agent Tool Integration

Expose intelligence capabilities via tool calls.

**Key Tasks:**
- Implement `code_intelligence` tool in `g3-core/src/tools/`
- Add CLI commands for interactive exploration
- Create tool schemas for LLM understanding

### Phase 5: Testing & Validation

Comprehensive testing of the integrated system.

**Key Tasks:**
- Write integration tests for full query flows
- Create test fixtures with known graph structures
- Validate result fusion and ranking

## Step by Step Tasks

### 1. Design Unified Index API (`crates/g3-index/src/unified_index.rs`)

- Create `UnifiedIndex` trait with methods:
  - `search_semantic()` - Vector-based semantic search
  - `search_lexical()` - BM25 keyword search
  - `search_ast()` - AST pattern matching
  - `search_lsp()` - LSP protocol queries (go-to-def, find-refs)
  - `query_graph()` - Knowledge graph queries
- Define `UnifiedSearchResult` with common fields
- Implement `QueryPlanner` that selects optimal strategy based on query

### 2. Implement Knowledge Graph Traverser (`crates/g3-index/src/traverser.rs`)

- Implement `BfsTraverser` - Breadth-first search from start nodes
- Implement `DfsTraverser` - Depth-first search with path tracking
- Add `DependencyWalker` - Specialized traversal for call/reference chains
- Create `PathFinder` - Find paths between two symbols
- Support configurable filters (max depth, node types, edge kinds)

### 3. Build Cross-Index Connector (`crates/g3-index/src/integration.rs`)

- Implement `IndexConnector` struct
- Add `link_lsp_to_chunks()` - Connect LSP locations to indexed chunks
- Add `enrich_result()` - Merge data from multiple sources
- Create `CrossIndexQuery` that executes across all layers

### 4. Create Agent Intelligence Tool (`crates/g3-core/src/tools/intelligence.rs`)

- Implement `code_intelligence` tool with subcommands:
  - `find_definitions <symbol>` - Go to symbol definitions
  - `find_references <symbol>` - Find all usages
  - `find_callers <symbol>` - Find functions that call this
  - `find_callees <symbol>` - Find functions this calls
  - `find_similar <code>` - Find similar code patterns
  - `explore_graph <symbol> <depth>` - Explore dependency graph
- Add tool schema for LLM understanding
- Handle streaming results for large result sets

### 5. Add CLI Intelligence Commands (`crates/g3-cli/src/commands/intelligence.rs`)

- Implement `/find <symbol>` - Find symbol definitions
- Implement `/refs <symbol>` - Find symbol references
- Implement `/callers <symbol>` - Show callers
- Implement `/graph <symbol> <depth>` - Visualize graph
- Implement `/search <query>` - Hybrid semantic+lexical search

### 6. Create Integration Tests

- Test query routing (semantic vs lexical vs AST)
- Test graph traversal with known test fixtures
- Test cross-index result fusion
- Test agent tool execution end-to-end

### 7. Update Documentation & Examples

- Add API documentation for new modules
- Create example showing end-to-end query
- Document the architecture in `docs/`

## Testing Strategy

### Unit Tests
- Query planner routing logic
- Traverser algorithms on small test graphs
- Cross-index connector linking
- Result fusion with RRF

### Integration Tests
- End-to-end query flow: Natural language query → Plan → Execute → Fuse → Return
- Graph traversal accuracy against known dependency chains
- LSP + index integration with real language servers

### Performance Tests
- Large codebase indexing throughput
- Search latency under various conditions
- Memory usage during graph traversal

## Acceptance Criteria

The task is complete when:

1. **Unified API** - Single interface (`UnifiedIndex`) supports all search strategies
2. **Query Planning** - Queries automatically routed to optimal strategy
3. **Graph Traversal** - Autonomous BFS/DFS/dependency-walk on knowledge graph
4. **Cross-Index Linking** - LSP symbols linked to indexed chunks
5. **Agent Tool** - `code_intelligence` tool available to agents
6. **CLI Commands** - Interactive exploration via `/find`, `/refs`, `/graph`, etc.
7. **Tests Pass** - All new tests passing with 80%+ coverage target

## Validation Commands

```bash
# Build the project
cargo build --release

# Run new index crate tests
cargo test -p g3-index --lib unified_index traverser integration

# Run core intelligence tool tests
cargo test -p g3-core --lib tools/intelligence

# Check all tests pass
cargo test

# Build LSP crate (ensure no regressions)
cargo build -p g3-lsp

# Example: Test graph traversal with known fixture
cargo run --example test_graph_traversal
```

## Notes

### Dependencies to Add

No new external dependencies required. Existing dependencies sufficient:
- `tokio` for async runtime
- `tracing` for logging
- `thiserror` for errors
- `serde` for serialization
- `tree-sitter` for AST parsing
- `lsp-types` for LSP protocol
- `qdrant-client` for vector search

### Design Considerations

1. **Lazy Loading** - Load LSP clients and indexes only when needed
2. **Streaming Results** - Support streaming for large result sets
3. **Result Fusion** - Use RRF to combine results from multiple sources
4. **Context Window** - Respect LLM context limits when returning results
5. **Error Resilience** - Fail gracefully if LSP server unavailable
6. **Caching** - Cache frequently accessed graph paths

### Future Enhancements

- Parallel query execution across layers
- Progressive result display (show fast results first)
- Query history and learning
- Code example extraction and summarization
- Cross-project knowledge transfer
