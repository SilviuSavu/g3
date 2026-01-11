# Coupling Hotspots

## Identification Method

Hotspots identified by:
1. Fan-in: Number of incoming edges (dependents)
2. Fan-out: Number of outgoing edges (dependencies)
3. Cross-crate edges: Files with imports from multiple external crates

## Crate-Level Hotspots

### By Fan-In (Most Depended Upon)

| Crate | Fan-In | Dependents |
|-------|--------|------------|
| g3-core | 43 | g3-cli, g3-ensembles, g3-planner + 40 file imports |
| g3-providers | 27 | g3-core, g3-planner + 25 file imports |
| g3-config | 16 | g3-cli, g3-core, g3-ensembles, g3-planner + 12 file imports |
| g3-computer-control | 12 | g3-cli, g3-core + 10 file imports |

### By Fan-Out (Most Dependencies)

| Crate | Fan-Out | Dependencies |
|-------|---------|-------------|
| g3-cli | 6 | g3-core, g3-config, g3-planner, g3-computer-control, g3-providers, g3-ensembles |
| g3-core | 4 | g3-providers, g3-config, g3-execution, g3-computer-control |
| g3-planner | 3 | g3-providers, g3-core, g3-config |
| g3-ensembles | 2 | g3-core, g3-config |

## File-Level Hotspots

### By Fan-Out (Files with Most Dependencies)

| File | Fan-Out | Description |
|------|---------|-------------|
| ./crates/g3-core/src/lib.rs | 27 | Core library root - declares 22 modules + 5 external imports |
| ./crates/g3-cli/src/lib.rs | 12 | CLI library root - 5 modules + 7 external imports |
| ./crates/g3-core/src/tools/mod.rs | 8 | Tools module - declares 8 submodules |
| ./crates/g3-planner/src/lib.rs | 8 | Planner library root - 7 modules + 1 external import |
| ./crates/g3-providers/src/lib.rs | 6 | Providers library root - 5 modules + 1 internal |
| ./crates/g3-computer-control/src/lib.rs | 5 | Computer control root - 5 modules |
| ./crates/g3-planner/src/llm.rs | 5 | LLM integration - 5 external imports |

### Files with Cross-Crate Imports

| File | External Crates Imported |
|------|-------------------------|
| ./crates/g3-cli/src/lib.rs | g3-config, g3-core |
| ./crates/g3-core/src/lib.rs | g3-config, g3-providers |
| ./crates/g3-core/src/context_window.rs | g3-providers |
| ./crates/g3-core/src/streaming.rs | g3-providers |
| ./crates/g3-core/src/tool_definitions.rs | g3-providers |
| ./crates/g3-core/src/tools/executor.rs | g3-config |
| ./crates/g3-core/src/tools/research.rs | g3-config |
| ./crates/g3-core/src/tools/webdriver.rs | g3-computer-control |
| ./crates/g3-core/src/webdriver_session.rs | g3-computer-control |
| ./crates/g3-planner/src/llm.rs | g3-config, g3-core, g3-providers |
| ./crates/g3-planner/src/lib.rs | g3-providers |
| ./crates/g3-ensembles/src/flock.rs | g3-config |

## High-Coupling Observations

### g3-core/src/lib.rs

- Fan-out: 27 (highest in codebase)
- Declares 22 public modules
- Imports from: g3-config, g3-providers
- Central hub for all core functionality

### g3-providers

- Fan-in: 27 (second highest)
- Imported by 25 files across 3 crates
- Provides: Message, MessageRole, CompletionRequest, LLMProvider, etc.
- Core abstraction layer for LLM communication

### g3-config

- Fan-in: 16
- Imported by 12 files across 4 crates
- Provides: Config struct
- Shared configuration across all layers

## Representative Evidence

### g3-core imports from g3-providers (18 edges)

```
./crates/g3-core/src/lib.rs: use g3_providers::{CacheControl, CompletionRequest, Message, MessageRole, ProviderRegistry};
./crates/g3-core/src/context_window.rs: use g3_providers::{Message, MessageRole, Usage};
./crates/g3-core/src/streaming.rs: use g3_providers::{CompletionRequest, MessageRole};
./crates/g3-core/src/tool_definitions.rs: use g3_providers::Tool;
```

### g3-planner imports from g3-core (5 edges)

```
./crates/g3-planner/src/llm.rs: use g3_core::project::Project;
./crates/g3-planner/src/llm.rs: use g3_core::Agent;
./crates/g3-planner/src/llm.rs: use g3_core::error_handling::{classify_error, ErrorType};
```
