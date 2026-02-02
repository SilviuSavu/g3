# Strongly Connected Components (Cycles)

**Generated**: 2025-02-02  
**Method**: Manual analysis of crate and file-level dependency graph

## Crate-Level Cycles

**None detected.**

The crate dependency graph is a DAG (directed acyclic graph). All crate dependencies flow downward:

```
g3-cli → g3-core → g3-providers
       → g3-config
       → g3-planner → g3-core (creates diamond, not cycle)
                    → g3-providers
                    → g3-config
       → g3-computer-control
       → g3-providers

g3-core → g3-execution
        → g3-computer-control
```

## File-Level Cycles Within Crates

### g3-core

**Potential cycle via lib.rs re-exports:**

Multiple modules import from `lib.rs` (for `ToolCall`, `Agent`, etc.), and `lib.rs` declares these modules. This is standard Rust module structure, not a problematic cycle.

```
lib.rs ←──mod──→ streaming_parser.rs (uses crate::ToolCall)
lib.rs ←──mod──→ context_window.rs (uses crate::ToolCall)
lib.rs ←──mod──→ acd.rs (uses crate::ToolCall)
lib.rs ←──mod──→ streaming.rs (uses crate::ToolCall)
lib.rs ←──mod──→ stats.rs (uses crate::CacheStats)
lib.rs ←──mod──→ retry.rs (uses crate::{Agent, DiscoveryOptions, TaskResult})
lib.rs ←──mod──→ feedback_extraction.rs (uses crate::{Agent, TaskResult})
```

This is the standard Rust pattern where `lib.rs` defines types and submodules import them via `crate::`. Not a problematic SCC.

### g3-cli

**No problematic cycles detected.**

Dependencies flow from high-level modules (interactive, agent_mode, accumulative) down to utilities (simple_output, g3_status, template).

### g3-providers

**No cycles detected.**

All provider implementations (anthropic, openai, gemini, databricks, embedded, mock) import from `lib.rs` only.

### g3-planner

**No cycles detected.**

`planner.rs` imports from `git.rs`, `history.rs`, `llm.rs`, `state.rs`. No reverse dependencies.

## Cross-Crate Diamonds (Not Cycles)

The following diamond patterns exist but are not cycles:

1. **g3-cli → g3-core → g3-config** and **g3-cli → g3-config**
2. **g3-cli → g3-planner → g3-core** and **g3-cli → g3-core**
3. **g3-cli → g3-planner → g3-providers** and **g3-cli → g3-providers**

These are valid DAG structures where multiple paths lead to the same dependency.

## Summary

| Scope | Cycles Found | Severity |
|-------|--------------|----------|
| Crate-level | 0 | N/A |
| File-level (g3-core) | 0 problematic | N/A |
| File-level (g3-cli) | 0 | N/A |
| File-level (g3-providers) | 0 | N/A |
| File-level (g3-planner) | 0 | N/A |

The codebase has a clean acyclic dependency structure.
