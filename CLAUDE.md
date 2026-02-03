# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

g3 is a modular AI coding agent built in Rust that helps complete tasks by writing and executing code. It follows a **tool-first philosophy** - actively using tools to manipulate files and execute commands rather than just providing advice.

## Build Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build (optimized)
cargo test                     # Run all tests
cargo test -p g3-core          # Run tests for a specific crate
cargo check                    # Check compilation without building
cargo run --example verify_message_id  # Run specific example
```

After building, copy binaries to PATH:
```bash
cp target/release/g3 ~/.local/bin/
cp target/release/libVisionBridge.dylib ~/.local/bin/  # macOS only
```

## Workspace Architecture

8-crate Rust workspace with clear separation of concerns:

```
g3/
├── src/main.rs           # Entry point (delegates to g3-cli)
├── crates/
│   ├── g3-cli/           # CLI interface, TUI, execution modes
│   ├── g3-core/          # Agent engine, tools, streaming parser, context management
│   ├── g3-providers/     # LLM provider abstractions (Anthropic, OpenAI, Databricks, embedded)
│   ├── g3-config/        # TOML configuration management
│   ├── g3-execution/     # Code execution engine
│   ├── g3-planner/       # Requirements-driven planning workflow with git integration
│   ├── g3-computer-control/  # GUI automation (mouse, keyboard, screenshots, OCR)
│   └── studio/           # Multi-agent session manager using git worktrees
├── agents/               # Agent persona definitions (carmack, hopper, euler, etc.)
├── analysis/             # Dependency analysis artifacts (graph.json, hotspots.md)
└── docs/                 # Technical documentation
```

### Key Data Flow

1. **CLI** parses args → creates session → loads config
2. **Config** resolves provider settings → returns provider instance
3. **Core** orchestrates: sends messages → parses streaming response → dispatches tool calls → manages context
4. **Providers** handle LLM communication with provider-specific optimizations
5. **Execution** runs shell commands with streaming output

### Critical Code Paths

| Area | Files | Notes |
|------|-------|-------|
| Streaming parser | `g3-core/src/streaming_parser.rs` | Partial JSON across chunks is subtle |
| Tool dispatch | `g3-core/src/tool_dispatch.rs`, `tools/` | Missing cases = silent failures |
| Context management | `g3-core/src/context_window.rs`, `compaction.rs` | Wrong token estimates = overflow |
| Provider trait | `g3-providers/src/lib.rs` | Must be Send + Sync |

## Adding Features

- **New tool**: Add definition in `tool_definitions.rs`, implement in `tools/`, add dispatch case in `tool_dispatch.rs`
- **New provider**: Implement `LLMProvider` trait in `g3-providers`
- **New CLI mode**: Add to CLI args in `g3-cli`, implement handler
- **New config option**: Add to structs in `g3-config`

## Critical Invariants

### MUST Hold
1. Tool calls must be valid JSON - streaming parser expects well-formed calls
2. Context window limits must be respected - exceeding causes API errors
3. Provider trait implementations must be Send + Sync
4. String slicing must be UTF-8 safe - use `chars().take(n)`, never `&s[..n]` on user text
5. Streaming is preferred - non-streaming blocks UI

### MUST NOT Do
1. Never block the async runtime - use `tokio::spawn` for CPU-intensive work
2. Never store secrets in logs - API keys are redacted
3. Never assume tool results fit in context - large results are auto-thinned
4. Never use byte-index string slicing on multi-byte text - causes panics on emoji/CJK

## Common Incorrect Assumptions

1. "All providers support tool calling" - Embedded models use JSON fallback
2. "Context window is unlimited" - Each provider has limits (4k-200k tokens)
3. "Tool results are always small" - File reads can return megabytes
4. "All platforms are equal" - macOS has more features (Vision, Accessibility)

## Execution Modes

| Mode | Command | Description |
|------|---------|-------------|
| Interactive | `g3` | Default. Accumulative autonomous mode |
| Single-shot | `g3 "task"` | One task, then exit |
| Chat only | `g3 --chat` | Traditional chat without autonomous runs |
| Planning | `g3 --planning --codepath ~/project` | Requirements-driven with git commits |
| Agent | `g3 --agent carmack` | Run specialized agent |

## Interactive Control Commands

- `/compact` - Trigger context compaction
- `/thinnify` - Replace large tool results with file references
- `/stats` - Show context and performance statistics
- `/readme` - Reload README.md and AGENTS.md
- `/help` - List all commands

## Configuration

Config file: `~/.config/g3/config.toml` (auto-created on first run)

Minimal config:
```toml
[providers]
default_provider = "anthropic.default"

[providers.anthropic.default]
api_key = "your-api-key"
model = "claude-sonnet-4-5"
```

See `config.example.toml` for all options.

## Dependency Analysis Artifacts

The `analysis/deps/` directory contains static analysis generated by the euler agent:

| File | Purpose |
|------|---------|
| `graph.json` | Canonical dependency graph (nodes, edges) |
| `sccs.md` | Strongly connected components (cycles) |
| `hotspots.md` | Files with high coupling (fan-in/fan-out) |
| `layers.observed.md` | Layering structure from dependency direction |

## Do's and Don'ts

### Do
- Run `cargo check` after modifications
- Run `cargo test` before committing
- Update tool definitions when adding tools
- Keep functions under 80 lines

### Don't
- Add blocking code in async contexts
- Create deeply nested conditionals (>6 levels)
- Add external dependencies for simple tasks
- Ignore error handling
