# Observed Layering

## Derivation Method

Layers derived mechanically from:
1. Crate dependency direction in Cargo.toml
2. Path-based module grouping
3. Import directionality analysis

## Crate Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 0: Binaries                                           │
│   g3 (main entry)                                           │
│   studio (standalone tool)                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: Application                                        │
│   g3-cli (CLI interface, 16 files)                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: Orchestration                                      │
│   g3-planner (planning logic, 8 files)                      │
│   g3-ensembles (multi-agent, 4 files)                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Core Engine                                        │
│   g3-core (agent engine, 38 files)                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 4: Infrastructure                                     │
│   g3-providers (LLM providers, 7 files)                     │
│   g3-config (configuration, 2 files)                        │
│   g3-execution (code execution, 1 file)                     │
│   g3-computer-control (desktop automation, 16 files)        │
└─────────────────────────────────────────────────────────────┘
```

## Intra-Crate Module Structure

### g3-core (38 files)

```
lib.rs
├── acd.rs                    # Aggressive Context Dehydration
├── background_process.rs     # Background process management
├── code_search/              # Tree-sitter code search
│   ├── mod.rs
│   └── searcher.rs
├── compaction.rs             # Context compaction
├── context_window.rs         # Context window management
├── error_handling.rs         # Error classification
├── feedback_extraction.rs    # Coach feedback extraction
├── paths.rs                  # Path utilities
├── project.rs                # Project abstraction
├── prompts.rs                # System prompts
├── provider_config.rs        # Provider configuration
├── provider_registration.rs  # Provider registration
├── retry.rs                  # Retry logic
├── session.rs                # Session management
├── session_continuation.rs   # Session continuation
├── streaming.rs              # Streaming utilities
├── streaming_parser.rs       # Tool call parser
├── task_result.rs            # Task result types
├── tool_definitions.rs       # Tool definitions
├── tool_dispatch.rs          # Tool routing
├── tools/                    # Tool implementations
│   ├── mod.rs
│   ├── acd.rs
│   ├── executor.rs
│   ├── file_ops.rs
│   ├── memory.rs
│   ├── misc.rs
│   ├── research.rs
│   ├── shell.rs
│   ├── todo.rs
│   └── webdriver.rs
├── ui_writer.rs              # UI abstraction
├── utils.rs                  # General utilities
└── webdriver_session.rs      # WebDriver session
```

### g3-cli (16 files)

```
lib.rs
├── accumulative.rs           # Accumulative mode
├── agent_mode.rs             # Agent mode
├── autonomous.rs             # Autonomous mode
├── cli_args.rs               # CLI argument parsing
├── coach_feedback.rs         # Coach feedback
├── filter_json.rs            # JSON filtering
├── interactive.rs            # Interactive mode
├── metrics.rs                # Metrics/timing
├── project_files.rs          # Project file loading
├── simple_output.rs          # Simple output helper
├── streaming_markdown.rs     # Markdown formatting
├── task_execution.rs         # Task execution
├── theme.rs                  # UI theming
├── ui_writer_impl.rs         # UiWriter implementation
└── utils.rs                  # CLI utilities
```

### g3-computer-control (16 files)

```
lib.rs
├── macax/                    # macOS Accessibility
│   ├── mod.rs
│   └── controller.rs
├── ocr/                      # OCR engines
│   ├── mod.rs
│   ├── tesseract.rs
│   └── vision.rs
├── platform/                 # Platform implementations
│   ├── mod.rs
│   ├── linux.rs
│   ├── macos.rs
│   └── windows.rs
├── types.rs                  # Shared types
└── webdriver/                # WebDriver implementations
    ├── mod.rs
    ├── chrome.rs
    ├── diagnostics.rs
    └── safari.rs
```

### g3-providers (7 files)

```
lib.rs
├── anthropic.rs              # Anthropic Claude
├── databricks.rs             # Databricks
├── embedded.rs               # Local llama.cpp
├── oauth.rs                  # OAuth flow
├── openai.rs                 # OpenAI-compatible
└── streaming.rs              # Streaming utilities
```

### g3-planner (8 files)

```
lib.rs
├── code_explore.rs           # Code exploration
├── git.rs                    # Git operations
├── history.rs                # History management
├── llm.rs                    # LLM integration
├── planner.rs                # Planning logic
├── prompts.rs                # Planner prompts
└── state.rs                  # State management
```

## Layer Violations

**None detected.**

All dependencies flow downward through the layer hierarchy. No upward dependencies exist.

## Uncertainty

- Layer assignment is based on dependency direction, not semantic intent
- `studio` is isolated (no internal crate dependencies) - layer assignment is nominal
- Some crates at Layer 4 could arguably be split further (e.g., `g3-computer-control` is large)
