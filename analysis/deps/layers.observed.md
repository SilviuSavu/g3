# Observed Layering

## Derivation Method

Layers derived mechanically from:
1. Crate dependency direction in Cargo.toml
2. Path-based grouping within crates
3. Import directionality between files

## Crate Layers

### Layer 0: Foundation (No internal dependencies)

| Crate | Purpose (from Cargo.toml description) |
|-------|---------------------------------------|
| g3-config | Configuration management |
| g3-execution | Code execution engine |
| g3-providers | LLM provider abstractions |
| g3-computer-control | Computer control (no description) |

### Layer 1: Core Services

| Crate | Dependencies |
|-------|-------------|
| g3-core | g3-providers, g3-config, g3-execution, g3-computer-control |

### Layer 2: Higher-Level Services

| Crate | Dependencies |
|-------|-------------|
| g3-ensembles | g3-core, g3-config |
| g3-planner | g3-providers, g3-core, g3-config |

### Layer 3: Application

| Crate | Dependencies |
|-------|-------------|
| g3-cli | g3-core, g3-config, g3-planner, g3-computer-control, g3-providers, g3-ensembles |
| g3 (root) | g3-cli, g3-providers |

## Module Groupings Within Crates

### g3-core (60 files)

| Group | Files | Purpose |
|-------|-------|--------|
| tools/ | 9 | Tool implementations (file_ops, shell, webdriver, etc.) |
| code_search/ | 2 | Tree-sitter based code search |
| root modules | 22 | Core functionality (agent, context, streaming, etc.) |
| tests/ | 27 | Integration and unit tests |

### g3-computer-control (29 files)

| Group | Files | Purpose |
|-------|-------|--------|
| platform/ | 5 | OS-specific implementations (macos, linux, windows) |
| webdriver/ | 4 | Browser automation (safari, chrome) |
| ocr/ | 3 | Text recognition (tesseract, vision) |
| macax/ | 3 | macOS accessibility |
| examples/ | 12 | Usage examples |

### g3-providers (10 files)

| Group | Files | Purpose |
|-------|-------|--------|
| providers | 5 | LLM implementations (anthropic, databricks, openai, embedded) |
| support | 3 | OAuth, streaming utilities |
| tests/ | 3 | Provider tests |

### g3-planner (12 files)

| Group | Files | Purpose |
|-------|-------|--------|
| core | 7 | Planner logic (planner, state, llm, git, history) |
| tests/ | 4 | Planner tests |

## Observed Directionality

### Cross-Crate Import Patterns

| From Crate | To Crate | Edge Count |
|------------|----------|------------|
| g3-cli | g3-core | 5 |
| g3-cli | g3-config | 2 |
| g3-cli | g3-computer-control | 1 |
| g3-core | g3-providers | 18 |
| g3-core | g3-config | 6 |
| g3-core | g3-computer-control | 2 |
| g3-planner | g3-providers | 3 |
| g3-planner | g3-core | 5 |
| g3-planner | g3-config | 1 |
| g3-ensembles | g3-core | 1 |
| g3-ensembles | g3-config | 1 |

### Layer Violations

**None detected.**

All observed imports follow the declared crate dependency direction.
No file imports from a crate that is not declared as a dependency.

## Uncertainty

- Internal `use crate::` imports not fully resolved to specific files
- Some module relationships inferred from `mod` declarations only
- Test files may have additional dev-dependencies not tracked
