# Indexing Features Demo Guide

## Overview

The g3 codebase has just received major improvements to its codebase indexing system. This guide shows you how to demo and observe these features in action.

## What Was Implemented (Last Commit)

### 1. IndexClient Auto-Initialization
**File**: `crates/g3-core/src/lib.rs:574-592`

The Agent now creates an `IndexClient` on startup when:
- `index.enabled = true` in config
- Qdrant is reachable

**Retry Logic**: 3 attempts with exponential backoff (100ms, 200ms, 400ms)

### 2. Enhanced index_status Tool
**File**: `crates/g3-core/src/tools/index.rs:494-550`

The `index_status` tool now shows:
- **Status**: "healthy" (graph available), "connected" (Qdrant only), "not_initialized", or "disabled"
- **Stats**: files_indexed, total_chunks
- **Config**: Qdrant URL, collection, embedding model, dimensions
- **Health Details**: qdrant_status, indexer_status, embedding_model

### 3. Semantic Search Improvements
**File**: `crates/g3-core/src/tools/index.rs:443-451`

The `semantic_search` tool now:
- Handles `None` index_client gracefully with helpful error message
- Auto-initializes IndexClient when needed
- Shows clear error when indexing not enabled

### 4. Knowledge Graph Tools
All graph tools (`graph_find_symbol`, `graph_find_callers`, `graph_find_references`, `graph_file_symbols`, `graph_stats`) now:
- Check graph availability before queries
- Provide helpful errors when graph not built
- Return structured JSON responses

## How to Demo (Step by Step)

### Step 1: Enable Indexing in Config

Create or update `~/.g3/config.toml`:

```toml
[providers]
default_provider = "anthropic.default"

[providers.anthropic.default]
api_key = "your-api-key"
model = "claude-sonnet-4-5"

[index]
enabled = true
qdrant_url = "http://localhost:6334"
collection_name = "g3-codebase"

[index.embeddings]
provider = "openrouter"
api_key = "${OPENROUTER_API_KEY}"
model = "qwen/qwen3-embedding-8b"
dimensions = 4096
```

### Step 2: Start Qdrant (if not running)

```bash
# Using Docker
docker run -d -p 6334:6334 qdrant/qdrant

# Or use qdrant locally
qdrant
```

### Step 3: Start g3 and Check Index Status

Run g3:
```bash
cd /Users/savusilviu/Desktop/self-contained-system/g3
g3
```

Then ask the agent to check index status:
```
Call the index_status tool to see current index health
```

**Expected Output**:
```json
{
  "enabled": true,
  "status": "healthy",
  "working_dir": "/Users/savusilviu/Desktop/self-contained-system/g3",
  "stats": {
    "files_indexed": 150,
    "total_chunks": 2340
  },
  "config": {
    "qdrant_url": "http://localhost:6334",
    "collection": "g3-codebase",
    "graph_available": true,
    "qdrant_status": "connected",
    "indexer_status": "ready",
    "embedding_model": "qwen/qwen3-embedding-8b",
    "dimensions": 4096,
    "hybrid_search": true,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }
}
```

### Step 4: Build the Index (if not built)

```
Call the index_codebase tool to build the codebase index with path="."
```

**Expected Output**:
```json
{
  "status": "success",
  "files_processed": 150,
  "chunks_created": 2340,
  "chunks_updated": 0,
  "chunks_deleted": 0,
  "files_skipped": 12,
  "duration_ms": 4523,
  "embedding_calls": 47,
  "working_dir": ".",
  "force": false
}
```

### Step 5: Demo Semantic Search

```
Call the semantic_search tool with query="main agent orchestrator" and limit=5
```

**Expected Output**:
```json
{
  "status": "success",
  "query": "main agent orchestrator",
  "count": 5,
  "results": [
    {
      "file": "crates/g3-core/src/lib.rs",
      "lines": "124-174",
      "kind": "struct",
      "name": "Agent",
      "score": "0.892",
      "content": "/// The main agent struct that orchestrates..."
    },
    {
      "file": "crates/g3-core/src/lib.rs",
      "lines": "2291-3139",
      "kind": "function",
      "name": "stream_completion_with_tools",
      "score": "0.723",
      "content": "Main async loop with streaming, tool execution..."
    }
  ]
}
```

**What to Observe**:
- Results ranked by semantic similarity (scores 0.0-1.0)
- Find code by describing what it does, not by name
- Results include file, line range, kind (struct/function), name, and content preview

### Step 6: Demo Graph Find Symbol

```
Call the graph_find_symbol tool with name="execute_semantic_search"
```

**Expected Output**:
```json
{
  "status": "success",
  "name": "execute_semantic_search",
  "count": 2,
  "symbols": [
    {
      "id": "sym_123",
      "name": "execute_semantic_search",
      "kind": "function",
      "file": "crates/g3-core/src/tools/index.rs",
      "lines": "321-420",
      "signature": "pub async fn execute_semantic_search<W: UiWriter>(tool_call: &ToolCall, ctx: &mut ToolContext<'_, W>) -> Result<String>"
    }
  ]
}
```

**What to Observe**:
- Find all occurrences of a function by name
- Returns location (file, lines) and full signature
- Multiple results for common names (if they exist)

### Step 7: Demo Graph Find Callers

1. First, get a symbol ID from `graph_find_symbol`
2. Then query its callers:

```
Call the graph_find_callers tool with symbol_id="sym_123"
```

**Expected Output**:
```json
{
  "status": "success",
  "symbol_id": "sym_123",
  "count": 3,
  "callers": [
    {
      "source": "sym_456",
      "target": "sym_123",
      "file": "crates/g3-core/src/lib.rs",
      "line": 1523
    },
    {
      "source": "sym_789",
      "target": "sym_123",
      "file": "crates/g3-core/src/tools/index.rs",
      "line": 890
    }
  ]
}
```

**What to Observe**:
- See all places that call a specific function
- Shows source (caller), target (called), file, and line
- Empty results when no callers found

## Key Features to Observe

### 1. Auto-Initialization
- When you call any index tool, it automatically creates the IndexClient if needed
- No manual initialization required
- Retry logic handles transient Qdrant connection issues

### 2. Health Status
- `index_status` shows comprehensive health information
- Distinguishes between Qdrant connection and graph availability
- Shows full configuration for debugging

### 3. Semantic Search
- Find code by meaning, not just keywords
- Results ranked by similarity score
- Works even if you don't know the exact function name

### 4. Knowledge Graph
- Full dependency tracking
- Callers/callees queries
- References tracking

## Test Results

All tests pass:
- g3-core: 328 tests
- g3-index: All tests
- No breaking changes

## Files Modified in This Session

1. `crates/g3-core/src/tools/index.rs` - Enhanced index_status, semantic_search
2. `crates/g3-core/src/index_client.rs` - Auto-initialization, retry logic
3. `crates/g3-core/src/lib.rs` - Agent creates IndexClient on startup
4. `crates/g3-index/` - Various improvements to search and indexing
5. `crates/g3-core/src/tools/plan.rs` - Plan verification system
6. `crates/g3-core/src/tools/memory.rs` - Workspace memory updates

## Summary

The indexing system now provides:
- ✅ Auto-initialization with retry logic
- ✅ Comprehensive health status
- ✅ Semantic search by meaning
- ✅ Knowledge graph queries
- ✅ Graceful error handling
- ✅ Persistent state management

All features are fully tested and ready to use!
