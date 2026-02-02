# Observed Layering Structure

**Generated**: 2025-02-02  
**Method**: Derived from crate dependencies and file import patterns

## Crate Layers

Based on dependency direction, the crates form the following layers:

```
Layer 0 (Leaf/Foundation):
┌─────────────────┬─────────────────┬─────────────────────────┬─────────────────┐
│   g3-config     │  g3-execution   │  g3-computer-control    │  g3-providers   │
│  (config mgmt)  │ (code execution)│  (browser/UI control)   │ (LLM providers) │
└─────────────────┴─────────────────┴─────────────────────────┴─────────────────┘
                                    │
                                    ▼
Layer 1 (Core Engine):
┌───────────────────────────────────────────────────────────────────────────────┐
│                               g3-core                                         │
│  (Agent, ToolCall, context management, tool dispatch, streaming)              │
└───────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
Layer 2 (Orchestration):
┌───────────────────────────────────────────────────────────────────────────────┐
│                              g3-planner                                       │
│  (fast-discovery planner, git integration, LLM orchestration)                 │
└───────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
Layer 3 (CLI/Application):
┌───────────────────────────────────────────────────────────────────────────────┐
│                               g3-cli                                          │
│  (interactive mode, autonomous mode, agent mode, commands, UI)                │
└───────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
Layer 4 (Binary Entry):
┌───────────────────────────────────────────────────────────────────────────────┐
│                                 g3                                            │
│  (main binary, minimal - delegates to g3-cli)                                 │
└───────────────────────────────────────────────────────────────────────────────┘

Separate:
┌───────────────────────────────────────────────────────────────────────────────┐
│                               studio                                          │
│  (standalone multi-agent workspace manager, no g3 crate dependencies)         │
└───────────────────────────────────────────────────────────────────────────────┘
```

## Layer Violations

**None detected at crate level.**

All dependencies flow downward (higher layer → lower layer). No upward dependencies exist.

## File-Level Layering Within g3-cli

```
Entry Layer:
  lib.rs (run function, mode dispatch)
      │
      ▼
Mode Layer:
  interactive.rs, autonomous.rs, agent_mode.rs, accumulative.rs
      │
      ▼
Command/Execution Layer:
  commands.rs, task_execution.rs, coach_feedback.rs
      │
      ▼
Utility Layer:
  simple_output.rs, g3_status.rs, display.rs, template.rs,
  ui_writer_impl.rs, streaming_markdown.rs, filter_json.rs,
  metrics.rs, project_files.rs, language_prompts.rs, embedded_agents.rs
      │
      ▼
Data Layer:
  cli_args.rs, project.rs, completion.rs, theme.rs
```

## File-Level Layering Within g3-core

```
Entry Layer:
  lib.rs (Agent struct, stream_completion_with_tools)
      │
      ▼
Orchestration Layer:
  retry.rs, compaction.rs, feedback_extraction.rs
      │
      ▼
Tool Layer:
  tool_dispatch.rs, tool_definitions.rs
  tools/mod.rs → tools/{shell,file_ops,plan,webdriver,research,memory,misc,acd}.rs
  tools/executor.rs
      │
      ▼
Streaming Layer:
  streaming.rs, streaming_parser.rs
      │
      ▼
Context Layer:
  context_window.rs, acd.rs, session.rs, session_continuation.rs
      │
      ▼
Infrastructure Layer:
  paths.rs, utils.rs, error_handling.rs, stats.rs,
  provider_config.rs, provider_registration.rs,
  background_process.rs, pending_research.rs,
  webdriver_session.rs, ui_writer.rs, project.rs, prompts.rs
      │
      ▼
Search Layer:
  code_search/mod.rs, code_search/searcher.rs
```

## File-Level Layering Within g3-providers

```
Entry Layer:
  lib.rs (LLMProvider trait, ProviderRegistry, Message types)
      │
      ▼
Provider Implementations:
  anthropic.rs, openai.rs, gemini.rs, databricks.rs, mock.rs
  embedded/mod.rs → embedded/provider.rs, embedded/adapters/{mod,glm}.rs
      │
      ▼
Utility Layer:
  streaming.rs, oauth.rs
```

## Directionality Confidence

| Layer Boundary | Confidence | Notes |
|----------------|------------|-------|
| g3 → g3-cli | High | Single dependency, clear delegation |
| g3-cli → g3-core | High | Many imports, clear consumer relationship |
| g3-cli → g3-planner | High | Explicit dependency for planning features |
| g3-core → g3-providers | High | Core uses provider abstractions |
| g3-core → g3-config | High | Core reads configuration |
| g3-core → g3-computer-control | High | WebDriver integration |
| g3-core → g3-execution | Medium | Declared but minimal observed usage |
| g3-planner → g3-core | High | Planner uses Agent, error handling |

## Uncertainty

1. **g3-execution usage**: Declared as dependency of g3-core but no `use g3_execution::` statements found in source. May be used transitively or for future features.

2. **studio isolation**: studio has no g3 crate dependencies. It may interact with g3 via filesystem/process boundaries rather than library calls.
