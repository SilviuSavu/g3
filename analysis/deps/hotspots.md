# Coupling Hotspots

## Method

Hotspots identified by:
1. Fan-in > 2× average (high incoming dependencies)
2. Fan-out > 2× average (high outgoing dependencies)
3. Cross-group edge concentration

## Metrics

### Crate Level

| Metric | Value |
|--------|-------|
| Average fan-in | 2.0 |
| Average fan-out | 1.7 |
| Threshold (2×) | 4.0 / 3.4 |

### File Level

| Metric | Value |
|--------|-------|
| Total edges | 123 |
| Total files | 95 |
| Average fan-out | 1.3 |
| Threshold (2×) | 2.6 |

## Crate-Level Hotspots

### High Fan-In (Most Depended Upon)

| Crate | Fan-In | Status |
|-------|--------|--------|
| g3-config | 4 | **HOTSPOT** (2× avg) |
| g3-providers | 4 | **HOTSPOT** (2× avg) |
| g3-core | 3 | Near threshold |

**Evidence for g3-config:**
- Depended on by: g3-cli, g3-core, g3-planner, g3-ensembles
- Contains: Configuration types, loading logic

**Evidence for g3-providers:**
- Depended on by: g3, g3-cli, g3-core, g3-planner
- Contains: LLM provider trait, message types, streaming

### High Fan-Out (Most Dependencies)

| Crate | Fan-Out | Status |
|-------|---------|--------|
| g3-cli | 6 | **HOTSPOT** (3.5× avg) |
| g3-core | 4 | **HOTSPOT** (2.4× avg) |
| g3-planner | 3 | Near threshold |

**Evidence for g3-cli:**
- Depends on: g3-core, g3-config, g3-planner, g3-computer-control, g3-providers, g3-ensembles
- Role: Top-level integration point

**Evidence for g3-core:**
- Depends on: g3-providers, g3-config, g3-execution, g3-computer-control
- Role: Central engine with multiple infrastructure dependencies

## File-Level Hotspots

### High Fan-Out Files

| File | Fan-Out | Threshold | Status |
|------|---------|-----------|--------|
| crates/g3-core/src/lib.rs | 29 | 2.6 | **HOTSPOT** (22× avg) |
| crates/g3-cli/src/lib.rs | 17 | 2.6 | **HOTSPOT** (13× avg) |
| crates/g3-core/src/tools/mod.rs | 9 | 2.6 | **HOTSPOT** (7× avg) |
| crates/g3-planner/src/lib.rs | 8 | 2.6 | **HOTSPOT** (6× avg) |
| crates/g3-providers/src/lib.rs | 6 | 2.6 | **HOTSPOT** (4.6× avg) |
| crates/g3-computer-control/src/lib.rs | 5 | 2.6 | **HOTSPOT** (3.8× avg) |
| crates/g3-planner/src/llm.rs | 5 | 2.6 | **HOTSPOT** (3.8× avg) |

**Note:** High fan-out in `lib.rs` files is expected (module re-exports). The `tools/mod.rs` and `llm.rs` hotspots are more significant as they represent actual coupling.

### Cross-Crate Import Concentration

| Source File | Cross-Crate Imports |
|-------------|--------------------|
| crates/g3-cli/src/lib.rs | 5 (g3-core, g3-config, g3-providers, g3-planner, g3-ensembles) |
| crates/g3-planner/src/llm.rs | 4 (g3-config, g3-core, g3-providers) |
| crates/g3-cli/src/autonomous.rs | 2 (g3-core) |
| crates/g3-cli/src/task_execution.rs | 2 (g3-core) |

## Observations

1. **g3-core/src/lib.rs** has extreme fan-out (29 edges) due to declaring 22+ modules
2. **g3-config** and **g3-providers** are foundational crates with high fan-in
3. **g3-cli** is the integration hub, pulling together all subsystems
4. **tools/mod.rs** aggregates 9 tool modules - natural aggregation point
5. **g3-planner/src/llm.rs** has notable cross-crate coupling (imports from 3 other crates)

## Cross-Group Edges

Total cross-crate imports: 43

| From Crate | To Crate | Count |
|------------|----------|-------|
| g3-cli | g3-core | 21 |
| g3-cli | g3-config | 4 |
| g3-cli | g3-providers | 2 |
| g3-planner | g3-core | 5 |
| g3-planner | g3-providers | 4 |
| g3-planner | g3-config | 2 |
| g3-core | g3-providers | 8 |
| g3-core | g3-config | 3 |
| g3-core | g3-computer-control | 2 |
| g3-ensembles | g3-core | 1 |
| g3-ensembles | g3-config | 1 |
