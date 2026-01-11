# Dependency Graph Summary

## Overview

| Metric | Count |
|--------|-------|
| Total nodes | 143 |
| Crate nodes | 9 |
| File nodes | 134 |
| Total edges | 189 |

## Node Distribution

### Files by Crate

| Crate | File Count |
|-------|------------|
| g3-core | 60 |
| g3-computer-control | 29 |
| g3-cli | 12 |
| g3-planner | 12 |
| g3-providers | 10 |
| g3-ensembles | 5 |
| g3 (root) | 2 |
| g3-config | 2 |
| g3-execution | 2 |

### Files by Type

| Type | Count |
|------|-------|
| module | 72 |
| test | 37 |
| example | 15 |
| lib | 8 |
| build | 1 |
| main | 1 |

## Edge Distribution

| Edge Type | Count | Description |
|-----------|-------|-------------|
| file_to_crate | 101 | File imports from external crate |
| mod_declaration | 71 | Module declaration within crate |
| crate_dependency | 17 | Cargo.toml dependency |

## Crate Dependency Structure

```
g3 (root binary)
├── g3-cli
│   ├── g3-core
│   │   ├── g3-providers
│   │   ├── g3-config
│   │   ├── g3-execution
│   │   └── g3-computer-control
│   ├── g3-config
│   ├── g3-planner
│   │   ├── g3-providers
│   │   ├── g3-core
│   │   └── g3-config
│   ├── g3-computer-control
│   ├── g3-providers
│   └── g3-ensembles
│       ├── g3-core
│       └── g3-config
└── g3-providers
```

## Top Fan-In Nodes (Most Depended Upon)

| Node | Fan-In |
|------|--------|
| g3-core | 43 |
| g3-providers | 27 |
| g3-config | 16 |
| g3-computer-control | 12 |
| g3-planner | 10 |
| g3-cli | 5 |
| g3-ensembles | 3 |
| g3-execution | 2 |

## Top Fan-Out Nodes (Most Dependencies)

| Node | Fan-Out |
|------|--------|
| ./crates/g3-core/src/lib.rs | 27 |
| ./crates/g3-cli/src/lib.rs | 12 |
| ./crates/g3-core/src/tools/mod.rs | 8 |
| ./crates/g3-planner/src/lib.rs | 8 |
| ./crates/g3-providers/src/lib.rs | 6 |
| g3-cli | 6 |
| ./crates/g3-computer-control/src/lib.rs | 5 |
| ./crates/g3-planner/src/llm.rs | 5 |

## Entrypoints

| File | Type | Description |
|------|------|-------------|
| ./src/main.rs | main | Root binary entrypoint |
| ./crates/g3-cli/src/lib.rs | lib | CLI library root |
| ./crates/g3-core/src/lib.rs | lib | Core engine library root |
| ./crates/g3-providers/src/lib.rs | lib | LLM providers library root |
| ./crates/g3-config/src/lib.rs | lib | Configuration library root |
| ./crates/g3-execution/src/lib.rs | lib | Execution library root |
| ./crates/g3-computer-control/src/lib.rs | lib | Computer control library root |
| ./crates/g3-ensembles/src/lib.rs | lib | Ensembles library root |
| ./crates/g3-planner/src/lib.rs | lib | Planner library root |

## Extraction Method

- Crate dependencies: Parsed from `Cargo.toml` files
- File-to-crate edges: Extracted from `use g3_*::` statements
- Module declarations: Extracted from `mod` and `pub mod` statements
- File classification: Based on path patterns and filename conventions
