# G3 - AI Coding Agent

A modular, composable AI coding agent built in Rust that helps you complete tasks by writing and executing code.

## Quick Start

```bash
# Interactive mode (default)
g3

# Single-shot mode
g3 "implement a fibonacci function in Rust"

# Autonomous mode with coach-player feedback
g3 --autonomous --max-turns 10

# Agent mode with specialized persona
g3 --agent carmack "optimize this code"

# Studio mode for multi-agent workflows
studio run --agent carmack
```

## Features

- **40+ Tools**: File ops, shell, planning, indexing, LSP, WebDriver, Beads issue tracker
- **6 Execution Modes**: Interactive, Autonomous, Accumulative, Agent, Studio, Planning
- **6 LLM Providers**: Anthropic, OpenAI, Gemini, Databricks, Z.ai, Embedded (local)
- **Code Intelligence**: Semantic search, knowledge graph, LSP integration
- **Context Management**: Auto-compaction, thinning, session continuation

## Architecture

12 Rust crates with clear separation of concerns:

- `g3-core` - Agent engine, tools, streaming
- `g3-cli` - CLI interface, modes
- `g3-providers` - LLM provider abstractions
- `g3-index` - Codebase indexing (Qdrant, BM25, graph)
- `g3-lsp` - Language Server Protocol client
- `g3-config` - Configuration management
- `g3-planner` - Requirements-driven planning
- `g3-computer-control` - Computer automation
- `g3-execution` - Code execution
- `g3-console` - Console utilities
- `g3-ensembles` - Multi-agent workflows
- `studio` - Multi-agent workspace

## Configuration

Create `~/.config/g3/config.toml`:

```toml
[providers]
default_provider = "anthropic"

[providers.anthropic]
api_key = "sk-ant-..."
model = "claude-3-5-sonnet-20241022"
```

## Documentation

- [DESIGN.md](DESIGN.md) - Architecture and design decisions
- [AGENTS.md](AGENTS.md) - Instructions for AI agents working with this codebase
- [analysis/memory.md](analysis/memory.md) - Workspace memory with code locations

## License

MIT
