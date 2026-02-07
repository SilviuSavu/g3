# Codebase Intelligence System - Demo Guide

This document demonstrates the Codebase Intelligence System with real-world scenarios.

## Prerequisites

Ensure the codebase is indexed first:

```bash
# In g3 interactive mode, run:
index_codebase
```

## Demo Scenarios

### Scenario 1: Find Symbol Definitions

**Use Case**: You want to find all definitions of a symbol by name.

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "find",
    "symbol": "process_request"
  }
}
```

**Expected Response**:
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

---

### Scenario 2: Find All References

**Use Case**: You want to find where a symbol is used throughout the codebase.

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "refs",
    "symbol": "DatabaseConnection"
  }
}
```

**Expected Response**:
```json
{
  "status": "success",
  "source": "indexed",
  "results": [
    {
      "source": "sym_def456",
      "target": "DatabaseConnection",
      "file": "src/db.rs",
      "line": 23
    },
    {
      "source": "sym_ghi789",
      "target": "DatabaseConnection",
      "file": "src/models/user.rs",
      "line": 45
    }
  ],
  "count": 2
}
```

---

### Scenario 3: Find Callers with Depth

**Use Case**: You want to understand the call hierarchy for a function.

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

**Expected Response**:
```json
{
  "status": "success",
  "source": "graph",
  "symbol": "main",
  "callers": [
    {
      "caller_id": "sym_entry_point",
      "depth": 1
    }
  ],
  "count": 1
}
```

---

### Scenario 4: Semantic Search for Similar Code

**Use Case**: You want to find similar code patterns (e.g., error handling patterns).

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "similar",
    "symbol": "error handling in API responses"
  }
}
```

**Expected Response**:
```json
{
  "status": "success",
  "source": "semantic",
  "query": "error handling in API responses",
  "results": [
    {
      "id": "chunk_xyz123",
      "file_path": "src/handlers.rs",
      "start_line": 100,
      "end_line": 120,
      "content": "if let Err(e) = process_request(...) {\n    return Err(ApiError::InternalServerError(e.to_string()));\n}",
      "kind": "code",
      "name": "process_request_error",
      "score": 0.87
    }
  ],
  "count": 1
}
```

---

### Scenario 5: Explore Dependency Graph

**Use Case**: You want to explore the dependency graph starting from a service.

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

**Expected Response**:
```json
{
  "status": "success",
  "source": "graph",
  "symbol": "UserService",
  "depth": 2,
  "traversal": [
    {
      "node_id": "sym_auth_caller",
      "type": "caller",
      "relation": "calls"
    },
    {
      "node_id": "ref_db_usage",
      "type": "reference",
      "line": 23
    }
  ],
  "count": 2
}
```

---

### Scenario 6: Combined Query

**Use Case**: You want to query both callers and references in one call.

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

**Expected Response**:
```json
{
  "status": "success",
  "source": "graph",
  "symbol": "AuthService",
  "callers": [
    {"id": "sym_api_handler"},
    {"id": "sym_worker_service"}
  ],
  "references": [
    {"file": "src/api.rs", "line": 45},
    {"file": "src/config.rs", "line": 12}
  ]
}
```

---

## Interactive Demo Commands

### In g3 Interactive Mode

Run these commands to explore:

```
/code_intelligence command=find symbol=process_request
/code_intelligence command=refs symbol=DatabaseConnection
/code_intelligence command=callers symbol=main depth=3
/code_intelligence command=similar symbol="error handling patterns"
/code_intelligence command=graph symbol=UserService depth=2
/code_intelligence command=query symbol=AuthService depth=4
```

### Using the Index Status Tool

Check if the codebase is indexed:

```
/index_status
```

### Using Semantic Search Directly

For direct semantic search on the indexed codebase:

```
/semantic_search query=find all functions that handle authentication
```

---

## API Demo with curl

You can also use the intelligence tools directly via the g3 API:

```bash
# Index the codebase first
curl -X POST http://localhost:3000/index_codebase \
  -H "Content-Type: application/json" \
  -d '{"path": ".", "force": false}'

# Find a symbol
curl -X POST http://localhost:3000/code_intelligence \
  -H "Content-Type: application/json" \
  -d '{
    "command": "find",
    "symbol": "process_request"
  }'
```

---

## Real-World Example: Understanding a Codebase

Let's say you're new to a codebase and want to understand the authentication flow:

### Step 1: Find the AuthService

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "find",
    "symbol": "AuthService"
  }
}
```

### Step 2: Find References to AuthService

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "refs",
    "symbol": "AuthService"
  }
}
```

### Step 3: Find Callers of AuthService

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "callers",
    "symbol": "AuthService",
    "depth": 2
  }
}
```

### Step 4: Explore the Dependency Graph

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "graph",
    "symbol": "AuthService",
    "depth": 3
  }
}
```

### Step 5: Find Similar Auth Patterns

```json
{
  "tool": "code_intelligence",
  "args": {
    "command": "similar",
    "symbol": "authentication middleware"
  }
}
```

---

## Troubleshooting

### No Results Returned

1. Run `index_codebase` to ensure the codebase is indexed
2. Check `index_status` for index health
3. Try a broader search query

### LSP Not Available

The system falls back to indexed search when LSP is unavailable.

### Qdrant Connection Failed

The system falls back to lexical search when vector search is unavailable.

---

## Performance Tips

1. **Use depth wisely**: Keep graph traversal depth low (1-5) for faster responses
2. **Limit results**: Use `semantic_search` with `limit` parameter
3. **Use specific queries**: Semantic search works better with specific natural language queries

---

## Next Steps

After understanding the basics:
1. Try complex queries combining multiple subcommands
2. Use the graph traversal for dependency analysis
3. Leverage semantic search for finding code patterns
4. Explore the knowledge graph for call hierarchies
