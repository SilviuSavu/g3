# Extraction Limitations

## Scope of Analysis

This structural analysis was performed using static extraction methods only.

## What Was Observed

| Source | Method | Confidence |
|--------|--------|------------|
| Crate dependencies | Cargo.toml `[dependencies]` parsing | High |
| Module structure | `pub mod` declarations in lib.rs files | High |
| File inventory | Filesystem traversal of `src/` directories | High |
| Use statements | `grep` for `^use ` patterns | Medium |
| External dependencies | Cargo.toml `[dependencies]` sections | High |

## What Was NOT Observed

### 1. Conditional Compilation

`#[cfg(...)]` attributes were not parsed. Dependencies that only exist under certain feature flags or target platforms are included unconditionally.

**Example:** `g3-computer-control` has platform-specific dependencies:
- `core-graphics`, `cocoa`, `objc` (macOS only)
- `x11` (Linux only)
- `windows` (Windows only)

These are all listed as dependencies but only one set compiles per platform.

### 2. Test Dependencies

`[dev-dependencies]` were excluded from the graph. Test-only coupling is not represented.

### 3. Build Dependencies

`[build-dependencies]` were excluded. Build script dependencies are not represented.

### 4. Dynamic/Runtime Dependencies

The following cannot be detected statically:
- Process spawning (e.g., `g3-console` spawns `g3` processes)
- Plugin loading
- Configuration-driven provider selection
- WebDriver browser automation targets

### 5. Trait Implementation Coupling

Trait implementations create implicit coupling not captured by `use` statements:
- `UiWriter` implementations in `g3-cli`
- `LLMProvider` implementations in `g3-providers`
- `ComputerController` implementations in `g3-computer-control`

### 6. Re-exports

`pub use` re-exports create transitive dependencies not fully traced:
- `g3-core` re-exports types from `g3-providers`
- `g3-computer-control` re-exports WebDriver types

### 7. Macro Expansion

Macro-generated code (e.g., `#[derive(...)]`, `async_trait`) creates dependencies not visible in source.

### 8. Workspace Inheritance

`workspace = true` dependencies inherit versions from root `Cargo.toml`. The actual version constraints are not captured in crate-level analysis.

## Inference Notes

### Inferred: g3-console Isolation

`g3-console` has no internal crate dependencies in its `Cargo.toml`. It interacts with `g3` via:
- Process spawning (`std::process::Command`)
- Log file parsing
- Filesystem monitoring

This runtime coupling is not represented in the dependency graph.

### Inferred: g3 → g3-providers Direct Dependency

The root `g3` crate depends directly on `g3-providers` despite `g3-cli` also depending on it. This may be for:
- Type re-exports
- Direct provider access in main.rs
- Historical reasons

The actual usage was not verified.

## Recommendations for Future Analysis

1. **cargo-depgraph**: Use `cargo depgraph` for authoritative Cargo-resolved dependencies
2. **cargo-tree**: Use `cargo tree` for transitive dependency analysis
3. **rust-analyzer**: Use LSP for precise type-level dependency tracking
4. **cargo-udeps**: Identify unused dependencies
5. **cargo-machete**: Detect potentially unused dependencies

## Validity Conditions

This analysis is valid as of the extraction date. It may be invalidated by:

- Adding/removing crates from the workspace
- Changing `[dependencies]` in any Cargo.toml
- Restructuring module hierarchies
- Adding conditional compilation features
