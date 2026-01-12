# Dependency Graph Summary

## Overview

| Metric | Count |
|--------|-------|
| Workspace crates | 10 |
| Crate-level edges | 17 |
| Source files (non-test) | 95 |
| File-level edges | 123 |
| Cross-crate imports | 43 |
| Strongly connected components | 0 |

## Crate-Level Structure

### Crates by Type

| Crate | Type | Files |
|-------|------|-------|
| g3 | bin (root) | 1 |
| g3-cli | lib | 16 |
| g3-core | lib | 38 |
| g3-providers | lib | 7 |
| g3-config | lib | 2 |
| g3-execution | lib | 1 |
| g3-computer-control | lib | 16 |
| g3-planner | lib | 8 |
| g3-ensembles | lib | 4 |
| studio | bin | 3 |

### Fan-In (Most Depended Upon)

| Crate | Dependents |
|-------|------------|
| g3-config | 4 |
| g3-providers | 4 |
| g3-core | 3 |
| g3-computer-control | 2 |
| g3-cli | 1 |
| g3-ensembles | 1 |
| g3-execution | 1 |
| g3-planner | 1 |

### Fan-Out (Most Dependencies)

| Crate | Dependencies |
|-------|-------------|
| g3-cli | 6 |
| g3-core | 4 |
| g3-planner | 3 |
| g3 | 2 |
| g3-ensembles | 2 |

## File-Level Structure

### Top Fan-Out Files (Most Outgoing Edges)

| File | Edges | Description |
|------|-------|-------------|
| crates/g3-core/src/lib.rs | 29 | Core library root |
| crates/g3-cli/src/lib.rs | 17 | CLI library root |
| crates/g3-core/src/tools/mod.rs | 9 | Tools module root |
| crates/g3-planner/src/lib.rs | 8 | Planner library root |
| crates/g3-providers/src/lib.rs | 6 | Providers library root |
| crates/g3-computer-control/src/lib.rs | 5 | Computer control root |
| crates/g3-planner/src/llm.rs | 5 | LLM integration |

### Top Fan-In (Most Imported)

| Target | Imports |
|--------|--------|
| g3-core (crate) | 21 |
| g3-providers (crate) | 11 |
| g3-config (crate) | 9 |
| g3-computer-control (crate) | 2 |

## Entrypoints

| File | Type |
|------|------|
| src/main.rs | Binary entrypoint (g3) |
| crates/studio/src/main.rs | Binary entrypoint (studio) |
| crates/g3-cli/src/lib.rs | Library root |
| crates/g3-core/src/lib.rs | Library root |

## Extraction Limitations

- Only `use` and `mod` statements at line start are parsed
- Conditional compilation (`#[cfg(...)]`) not evaluated
- Macro-generated imports not detected
- Re-exports through `pub use` not fully traced
- Test modules (`mod tests`) excluded from graph
- Test files (`*_test.rs`, `tests/`) excluded from graph
