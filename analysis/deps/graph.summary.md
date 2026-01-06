# G3 Dependency Graph Summary

## Overview

| Metric | Value |
|--------|-------|
| Total Crates | 10 |
| Internal Dependency Edges | 16 |
| Total Source Files | 76 |
| Leaf Crates (no internal dependents) | 4 |
| Root Crates (no internal dependencies except leaves) | 2 |

## Crate Inventory

| Crate | Type | Path | Files |
|-------|------|------|-------|
| g3 | binary | . | 1 |
| g3-cli | library | crates/g3-cli | 8 |
| g3-core | library | crates/g3-core | 27 |
| g3-providers | library | crates/g3-providers | 6 |
| g3-config | library | crates/g3-config | 1 |
| g3-execution | library | crates/g3-execution | 1 |
| g3-computer-control | library | crates/g3-computer-control | 14 |
| g3-console | library+binary | crates/g3-console | 15 |
| g3-ensembles | library | crates/g3-ensembles | 3 |
| g3-planner | library | crates/g3-planner | 8 |

## Dependency Structure

### Internal Crate Dependencies (16 edges)

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
│   ├── g3-providers
│   └── g3-ensembles
│       ├── g3-core
│       └── g3-config
└── g3-providers

g3-console (standalone binary)
    (no internal dependencies - uses g3 via process spawning)
```

### Fan-In (Dependents Count)

| Crate | Fan-In | Dependents |
|-------|--------|------------|
| g3-config | 5 | g3-cli, g3-core, g3-ensembles, g3-planner |
| g3-core | 4 | g3-cli, g3-ensembles, g3-planner |
| g3-providers | 4 | g3, g3-cli, g3-core, g3-planner |
| g3-cli | 1 | g3 |
| g3-execution | 1 | g3-core |
| g3-computer-control | 1 | g3-core |
| g3-ensembles | 1 | g3-cli |
| g3-planner | 1 | g3-cli |
| g3 | 0 | (root) |
| g3-console | 0 | (standalone) |

### Fan-Out (Dependencies Count)

| Crate | Fan-Out | Dependencies |
|-------|---------|-------------|
| g3-cli | 5 | g3-core, g3-config, g3-planner, g3-providers, g3-ensembles |
| g3-core | 4 | g3-providers, g3-config, g3-execution, g3-computer-control |
| g3-planner | 3 | g3-providers, g3-core, g3-config |
| g3-ensembles | 2 | g3-core, g3-config |
| g3 | 2 | g3-cli, g3-providers |
| g3-config | 0 | (leaf) |
| g3-execution | 0 | (leaf) |
| g3-computer-control | 0 | (leaf) |
| g3-providers | 0 | (leaf) |
| g3-console | 0 | (standalone) |

## Entrypoints

1. **g3** (main binary) - `src/main.rs` delegates to `g3-cli`
2. **g3-console** (standalone binary) - `crates/g3-console/src/main.rs`

## Layering (Observed)

```
Layer 4 (Entry):      g3, g3-console
Layer 3 (CLI/UI):     g3-cli
Layer 2 (Features):   g3-ensembles, g3-planner
Layer 1 (Core):       g3-core
Layer 0 (Foundation): g3-config, g3-providers, g3-execution, g3-computer-control
```

## Extraction Limitations

- Graph derived from Cargo.toml `[dependencies]` sections only
- File-level dependencies inferred from `use` statements (not fully traced)
- Conditional compilation (`#[cfg(...)]`) dependencies not distinguished
- Test dependencies excluded from analysis
- Dynamic/runtime dependencies not captured
