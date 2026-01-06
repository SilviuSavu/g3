# Coupling Hotspots

## High Fan-In Nodes

Nodes with disproportionate incoming dependencies (dependents).

| Crate | Fan-In | % of Crates | Classification |
|-------|--------|-------------|----------------|
| g3-config | 5 | 50% | **High** - shared foundation |
| g3-core | 4 | 40% | **High** - central hub |
| g3-providers | 4 | 40% | **High** - shared foundation |
| g3-cli | 1 | 10% | Normal |
| g3-execution | 1 | 10% | Normal |
| g3-computer-control | 1 | 10% | Normal |
| g3-ensembles | 1 | 10% | Normal |
| g3-planner | 1 | 10% | Normal |

### g3-config (Fan-In: 5)

**Dependents:** g3-cli, g3-core, g3-ensembles, g3-planner, (implicit: g3)

**Evidence:**
- `g3-cli/Cargo.toml`: `g3-config = { path = "../g3-config" }`
- `g3-core/Cargo.toml`: `g3-config = { path = "../g3-config" }`
- `g3-ensembles/Cargo.toml`: `g3-config = { path = "../g3-config" }`
- `g3-planner/Cargo.toml`: `g3-config = { path = "../g3-config" }`

**Coupling Pattern:** Configuration struct (`Config`) is passed through the call stack. Changes to `Config` fields propagate to all dependents.

### g3-core (Fan-In: 4)

**Dependents:** g3-cli, g3-ensembles, g3-planner

**Evidence:**
- `g3-cli/Cargo.toml`: `g3-core = { path = "../g3-core" }`
- `g3-ensembles/Cargo.toml`: `g3-core = { path = "../g3-core" }`
- `g3-planner/Cargo.toml`: `g3-core = { path = "../g3-core" }`

**Coupling Pattern:** `Agent` struct is the primary interface. Feature modules create and orchestrate agents.

### g3-providers (Fan-In: 4)

**Dependents:** g3, g3-cli, g3-core, g3-planner

**Evidence:**
- `Cargo.toml`: `g3-providers = { path = "crates/g3-providers" }`
- `g3-cli/Cargo.toml`: `g3-providers = { path = "../g3-providers" }`
- `g3-core/Cargo.toml`: `g3-providers = { path = "../g3-providers" }`
- `g3-planner/Cargo.toml`: `g3-providers = { path = "../g3-providers" }`

**Coupling Pattern:** `Message`, `MessageRole`, `LLMProvider` trait are widely used types.

## High Fan-Out Nodes

Nodes with disproportionate outgoing dependencies.

| Crate | Fan-Out | % of Crates | Classification |
|-------|---------|-------------|----------------|
| g3-cli | 5 | 50% | **High** - orchestrator |
| g3-core | 4 | 40% | **High** - integrator |
| g3-planner | 3 | 30% | Moderate |
| g3-ensembles | 2 | 20% | Normal |
| g3 | 2 | 20% | Normal |

### g3-cli (Fan-Out: 5)

**Dependencies:** g3-core, g3-config, g3-planner, g3-providers, g3-ensembles

**Coupling Pattern:** CLI orchestrates all major features. This is expected for a top-level UI crate.

### g3-core (Fan-Out: 4)

**Dependencies:** g3-providers, g3-config, g3-execution, g3-computer-control

**Coupling Pattern:** Core integrates all foundation crates. This is the expected role of a "core" module.

## File-Level Hotspots

### g3-core/src/lib.rs

**Lines:** ~3366 (largest file in codebase)

**Concerns:**
- Contains the entire `Agent` struct implementation
- High cyclomatic complexity
- Multiple responsibilities (streaming, tools, context management)

**Evidence:** File contains 19 public module declarations and the main `Agent` implementation.

### g3-cli/src/lib.rs

**Lines:** ~2888

**Concerns:**
- Contains multiple mode implementations (autonomous, interactive, planning, agent)
- Large `run_autonomous` function
- Mixed concerns (UI, orchestration, error handling)

## Cross-Crate Type Dependencies

### Most Shared Types

| Type | Defined In | Used By |
|------|------------|--------|
| `Config` | g3-config | g3-cli, g3-core, g3-ensembles, g3-planner |
| `Message` | g3-providers | g3-core, g3-cli, g3-planner |
| `MessageRole` | g3-providers | g3-core, g3-cli, g3-planner |
| `Agent` | g3-core | g3-cli, g3-ensembles, g3-planner |
| `LLMProvider` | g3-providers | g3-core, g3-planner |
| `UiWriter` | g3-core | g3-cli |

## Metrics Summary

| Metric | Value | Threshold | Status |
|--------|-------|-----------|--------|
| Max Fan-In | 5 (g3-config) | ≤6 | ✅ OK |
| Max Fan-Out | 5 (g3-cli) | ≤6 | ✅ OK |
| Largest File | 3366 lines | ≤1000 | ⚠️ Large |
| Crates with Fan-In > 3 | 3 | ≤4 | ✅ OK |
| Crates with Fan-Out > 3 | 2 | ≤3 | ✅ OK |
