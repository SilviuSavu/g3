# Observed Layering

## Layer Structure

The G3 codebase exhibits a 5-layer architecture derived mechanically from dependency direction:

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 4: Entry Points                                          │
│  ┌─────────┐  ┌─────────────┐                                   │
│  │   g3    │  │  g3-console │                                   │
│  └────┬────┘  └─────────────┘                                   │
│       │            (standalone)                                 │
├───────┼─────────────────────────────────────────────────────────┤
│  Layer 3: CLI/UI                                                │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────┐                                                    │
│  │ g3-cli  │                                                    │
│  └────┬────┘                                                    │
│       │                                                         │
├───────┼─────────────────────────────────────────────────────────┤
│  Layer 2: Feature Modules                                       │
│       │                                                         │
│       ├──────────────┬──────────────┐                           │
│       ▼              ▼              │                           │
│  ┌───────────┐  ┌───────────┐       │                           │
│  │g3-ensembles│ │g3-planner │       │                           │
│  └─────┬─────┘  └─────┬─────┘       │                           │
│        │              │             │                           │
├────────┼──────────────┼─────────────┼───────────────────────────┤
│  Layer 1: Core Engine                                           │
│        │              │             │                           │
│        └──────┬───────┘             │                           │
│               ▼                     │                           │
│          ┌─────────┐                │                           │
│          │ g3-core │◄───────────────┘                           │
│          └────┬────┘                                            │
│               │                                                 │
├───────────────┼─────────────────────────────────────────────────┤
│  Layer 0: Foundation                                            │
│               │                                                 │
│    ┌──────────┼──────────┬──────────────┬───────────────┐       │
│    ▼          ▼          ▼              ▼               │       │
│ ┌────────┐ ┌──────────┐ ┌───────────┐ ┌─────────────────┴─┐     │
│ │g3-config│ │g3-providers│ │g3-execution│ │g3-computer-control│  │
│ └────────┘ └──────────┘ └───────────┘ └───────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

## Layer Definitions

### Layer 0: Foundation
**Crates:** g3-config, g3-providers, g3-execution, g3-computer-control

- No internal dependencies
- Provide fundamental capabilities:
  - Configuration loading and management
  - LLM provider abstractions (Anthropic, OpenAI, Databricks, embedded)
  - Code execution engine
  - Computer control (mouse, keyboard, screenshots)

### Layer 1: Core Engine
**Crates:** g3-core

- Depends on all Layer 0 crates
- Central orchestration:
  - Agent state machine
  - Context window management
  - Tool dispatch and execution
  - Streaming response parsing
  - Session management

### Layer 2: Feature Modules
**Crates:** g3-ensembles, g3-planner

- Depend on Layer 0 (g3-config) and Layer 1 (g3-core)
- Specialized functionality:
  - g3-ensembles: Multi-agent parallel development (flock mode)
  - g3-planner: Requirements-driven planning workflow

### Layer 3: CLI/UI
**Crates:** g3-cli

- Depends on Layers 0, 1, and 2
- User interface:
  - Command-line parsing
  - Interactive REPL
  - Output formatting
  - Mode orchestration (autonomous, chat, planning, agent)

### Layer 4: Entry Points
**Crates:** g3, g3-console

- Binary entry points
- g3: Main CLI binary (delegates to g3-cli)
- g3-console: Standalone web console (no internal deps)

## Layer Violations

**None detected.**

All dependencies flow downward (higher layers depend on lower layers).

## Cross-Layer Dependencies

| From Layer | To Layer | Count | Edges |
|------------|----------|-------|-------|
| 4 → 3 | Entry → CLI | 1 | g3 → g3-cli |
| 4 → 0 | Entry → Foundation | 1 | g3 → g3-providers |
| 3 → 2 | CLI → Features | 2 | g3-cli → g3-ensembles, g3-cli → g3-planner |
| 3 → 1 | CLI → Core | 1 | g3-cli → g3-core |
| 3 → 0 | CLI → Foundation | 2 | g3-cli → g3-config, g3-cli → g3-providers |
| 2 → 1 | Features → Core | 2 | g3-ensembles → g3-core, g3-planner → g3-core |
| 2 → 0 | Features → Foundation | 3 | g3-ensembles → g3-config, g3-planner → g3-config, g3-planner → g3-providers |
| 1 → 0 | Core → Foundation | 4 | g3-core → g3-config, g3-core → g3-providers, g3-core → g3-execution, g3-core → g3-computer-control |

## Observations

1. **Clean layering**: No upward dependencies detected
2. **g3-console isolation**: Standalone binary with no internal dependencies
3. **g3-config ubiquity**: Used by 5 crates across all layers
4. **g3-core centrality**: Hub for all feature modules
5. **Skip-layer dependencies**: g3-cli directly depends on Layer 0 crates (g3-config, g3-providers) - this is acceptable for configuration and provider access
