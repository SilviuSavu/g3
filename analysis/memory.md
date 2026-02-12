# Workspace Memory
> Updated: 2026-02-12 | Compacted from 101k to ~25k chars

## Core Architecture

### Crate Structure (Layered)
- **Layer 0 (Foundation)**: g3-config, g3-providers, g3-execution, g3-computer-control - zero outgoing deps
- **Layer 1 (Core Engine)**: g3-core - Agent, streaming, context, tools (18 incoming deps)
- **Layer 2 (Orchestration)**: g3-planner - requirements-driven development
- **Layer 3 (CLI)**: g3-cli - 6 execution modes
- **Layer 4 (Binary)**: g3 root - delegates to g3-cli
- **Separate**: studio - standalone, no g3 deps

### Core Abstractions
| Name | File | Purpose |
|------|------|---------|
| `Agent<W>` | g3-core/lib.rs:124 | Main agent with context, provider, tools |
| `LLMProvider` | g3-providers/lib.rs:14 | Send+Sync trait for all providers |
| `ContextWindow` | g3-core/context_window.rs:75 | Token tracking, message history |
| `ToolConfig` | g3-core/tool_definitions.rs:12 | Builder for 42 tools |
| `StreamingState` | g3-core/streaming.rs:16 | Cross-iteration state |
| `UiWriter` | g3-core/ui_writer.rs | Output abstraction trait |

### Execution Modes (6)
1. **Interactive** (default): `g3` - REPL conversation
2. **Autonomous**: `g3 --autonomous` - coach-player loop
3. **Accumulative**: Interactive + autonomous runs
4. **Planning**: `g3 --planning` - requirements-driven
5. **Agent**: `g3 --agent <name>` - specialized personas
6. **Studio**: `studio run` - multi-agent worktrees

### Data Flow
```
User Input → g3-cli → Agent.add_message() → stream_completion_with_tools() →
LLM Provider → StreamingParser → ToolCall → ToolDispatch → Execution →
ContextWindow.update() → Continue or Complete
```

## Tool System (42 Tools)

**Core (22)**: shell, write_file, str_replace, read_file, preview_file, list_directory, list_files, scan_folder, complexity_metrics, pattern_search, code_intelligence, code_search, rg, switch_mode, final_output, plan_read, plan_write, plan_approve, plan_verify, todo_read, todo_write, remember

**Webdriver (10)**: webdriver_start, navigate, get_url, get_title, find_element, find_elements, click, send_keys, execute_script, screenshot

**Beads (10)**: ready, create, update, close, list, show, sync, prime, formula_list, mol_pour

**MCP (4)**: web_search, web_reader, search_doc, get_repo_structure

**Builder**: `ToolConfig::new().with_mcp_tools().with_index_tools().with_lsp_tools()`

## Key Subsystems

### Context Window Management
- Thinning at 50%, 60%, 70%, 80% thresholds
- Compaction at 80% capacity
- Location: `g3-core/context_window.rs`, `g3-core/compaction.rs`

### Plan Mode
- Structured planning with happy/negative/boundary checks
- Evidence verification (code locations, test refs)
- `blocked_by` field for dependencies
- Tools: plan_read, plan_write, plan_approve
- Location: `g3-core/tools/plan.rs`

### Session Continuation
- Symlink-based: `.g3/session` → `.g3/sessions/<id>/`
- Save/restore via `SessionContinuation`
- Location: `g3-core/session_continuation.rs`

### Codebase Intelligence (g3-index)
- Semantic search via Qdrant + Qwen3-Embedding-8B
- Knowledge graph with 12 relationship types
- Tools: index_status, index_codebase, semantic_search, graph_find_*
- Location: `g3-index/unified_index.rs`, `g3-core/tools/intelligence.rs`

### Async Research Tool
- Spawns scout in background tokio task
- Returns immediately with research_id
- Results injected at next iteration
- Location: `g3-core/pending_research.rs`, `g3-core/tools/research.rs`

### Codebase Scout Agent
- Produces structural overview with ---SCOUT_REPORT_START---/---SCOUT_REPORT_END--- markers
- Uses analysis/deps/ artifacts (graph.json, hotspots.md, etc.)
- Location: `g3-core/tools/codebase_scout.rs`, `agents/codebase-scout.md`

### Error Handling
- Recoverable: rate limits, network, server, timeout (exponential backoff)
- Non-recoverable: auth failures, invalid requests
- 3 retries default, 6 in autonomous mode
- Location: `g3-core/error_handling.rs`

### LLM Providers (5)
- Anthropic (native tool calling)
- OpenAI
- Gemini
- Databricks (OAuth)
- Embedded (local llama.cpp with Metal)

### Auto-Memory System
- Triggers MEMORY CHECKPOINT when tool_calls_this_turn > 0
- Location: `g3-core/lib.rs:send_auto_memory_reminder()`

## Important Patterns

### UTF-8 Safe String Slicing
```rust
s.char_indices().nth(n).map(|(i,_)| i).unwrap_or(s.len())
```
Danger zones: display truncation, ACD stubs, user input

### Retry Logic Pattern (Qdrant)
```rust
let client = 'outer: {
    for attempt in 1..=MAX_RETRIES {
        match try_connect().await {
            Ok(c) => break 'outer c,
            Err(e) if attempt < MAX_RETRIES => {
                sleep(exp_backoff(attempt)).await;
            }
            Err(e) => return Err(e),
        }
    }
};
```

### Streaming Loop
1. MAX_ITERATIONS = 400 prevents runaway
2. Real-time JSON tool call detection
3. Auto-continue for incomplete calls in autonomous mode
4. Location: `g3-core/streaming.rs`, `g3-core/lib.rs:stream_completion_with_tools()`

## Configuration

### Indexing (config.toml)
```toml
[index]
enabled = true
qdrant_url = "http://localhost:6334"
[index.embeddings]
provider = "openrouter"
model = "qwen/qwen3-embedding-8b"
dimensions = 4096
```

### Embedded Provider
```toml
[providers.embedded.glm4]
model_path = "~/.g3/models/THUDM_GLM-4-32B-0414-Q6_K_L.gguf"
model_type = "glm4"
context_length = 32768
gpu_layers = 99
```

## CLI Entry Points
- `g3-cli/interactive.rs` - Interactive, mode selection menu
- `g3-cli/autonomous.rs` - Coach-player loop
- `g3-cli/agent_mode.rs` - Agent personas
- `g3-cli/accumulative.rs` - Evolutionary requirements
- `g3-planner/lib.rs` - Planning mode

## Test Status
- g3-core: 328 tests PASS
- g3-index: 134 tests PASS
- g3-cli: 170+ tests PASS
- Total: 500+ tests passing

## Build Status
- Release: `target/release/g3` (42 MB)
- Debug: `target/debug/g3` (92 MB)
- Studio: `target/debug/studio` (5.4 MB)
