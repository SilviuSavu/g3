# Coupling Hotspots

**Generated**: 2025-02-02  
**Method**: Fan-in/fan-out analysis from dependency graph

## High Fan-In Files (Most Depended Upon)

These files are imported by many other files. Changes here have wide impact.

| File | Fan-In | Dependents |
|------|--------|------------|
| `g3-core/src/lib.rs` | 18 | streaming_parser, context_window, acd, streaming, stats, retry, feedback_extraction, task_result, tool_dispatch, tools/* |
| `g3-core/src/ui_writer.rs` | 14 | retry, compaction, feedback_extraction, tool_dispatch, tools/{acd,executor,shell,research,file_ops,plan,memory,misc,webdriver} |
| `g3-cli/src/simple_output.rs` | 9 | utils, interactive, autonomous, coach_feedback, accumulative, task_execution, commands, agent_mode |
| `g3-cli/src/template.rs` | 6 | project_files, accumulative, commands, embedded_agents, agent_mode, interactive |
| `g3-core/src/paths.rs` | 6 | acd, session, context_window, tools/{executor,shell,plan} |
| `g3-core/src/context_window.rs` | 5 | compaction, stats, streaming, session, task_result |
| `g3-cli/src/g3_status.rs` | 5 | simple_output, interactive, task_execution, commands |
| `g3-cli/src/display.rs` | 4 | interactive, ui_writer_impl, agent_mode |
| `g3-cli/src/ui_writer_impl.rs` | 4 | autonomous, coach_feedback, accumulative, agent_mode |
| `g3-core/src/utils.rs` | 3 | tools/{shell,file_ops} |

## High Fan-Out Files (Most Dependencies)

These files import many other modules. They are integration points.

| File | Fan-Out | Dependencies |
|------|---------|-------------|
| `g3-cli/src/interactive.rs` | 11 | g3-core, completion, commands, display, g3_status, project, project_files, simple_output, input_formatter, template, task_execution, utils |
| `g3-cli/src/agent_mode.rs` | 11 | g3-core, project_files, display, language_prompts, simple_output, embedded_agents, ui_writer_impl, interactive, template, project, cli_args |
| `g3-cli/src/accumulative.rs` | 8 | g3-core, autonomous, cli_args, interactive, simple_output, ui_writer_impl, utils, template |
| `g3-cli/src/commands.rs` | 7 | g3-core, completion, g3_status, simple_output, project, template, task_execution |
| `g3-core/src/tools/executor.rs` | 7 | g3-config, background_process, pending_research, paths, ui_writer, webdriver_session, lib (ToolCall) |
| `g3-cli/src/autonomous.rs` | 5 | g3-core, coach_feedback, metrics, simple_output, ui_writer_impl |
| `g3-core/src/compaction.rs` | 4 | g3-providers, context_window, provider_config, ui_writer |
| `g3-core/src/tool_dispatch.rs` | 4 | tools/executor, tools/mod, ui_writer, lib (ToolCall) |
| `g3-cli/src/ui_writer_impl.rs` | 4 | g3-core, filter_json, display, streaming_markdown |
| `g3-core/src/streaming.rs` | 4 | g3-providers, context_window, streaming_parser, lib (ToolCall) |

## Cross-Crate Coupling Points

Files that bridge multiple crates:

| File | Cross-Crate Imports | Crates Touched |
|------|---------------------|----------------|
| `g3-core/src/lib.rs` | 2 | g3-config, g3-providers |
| `g3-core/src/webdriver_session.rs` | 1 | g3-computer-control |
| `g3-core/src/tools/webdriver.rs` | 1 | g3-computer-control |
| `g3-core/src/tools/executor.rs` | 1 | g3-config |
| `g3-core/src/tools/research.rs` | 1 | g3-config |
| `g3-core/src/provider_registration.rs` | 2 | g3-config, g3-providers |
| `g3-planner/src/llm.rs` | 3 | g3-config, g3-core, g3-providers |

## Crate-Level Coupling

| Crate | Outgoing Deps | Incoming Deps | Coupling Score |
|-------|---------------|---------------|----------------|
| g3-cli | 5 | 1 | High (consumer) |
| g3-core | 4 | 3 | High (hub) |
| g3-planner | 3 | 1 | Medium |
| g3-providers | 0 | 5 | High (foundation) |
| g3-config | 0 | 5 | High (foundation) |
| g3-computer-control | 0 | 2 | Low |
| g3-execution | 0 | 1 | Low |
| studio | 0 | 0 | Isolated |

## Observations

1. **g3-core/src/lib.rs** is the primary coupling hotspot. It defines `Agent`, `ToolCall`, and other core types used throughout the codebase.

2. **g3-core/src/ui_writer.rs** defines the `UiWriter` trait used by all tool implementations for output. High fan-in is expected for a trait definition.

3. **g3-cli/src/simple_output.rs** is a utility wrapper used by most CLI modules. High fan-in indicates it's a well-factored common dependency.

4. **g3-cli/src/interactive.rs** and **g3-cli/src/agent_mode.rs** have the highest fan-out, as expected for top-level orchestration modules.

5. **g3-providers** and **g3-config** are foundation crates with zero outgoing dependencies and high incoming dependencies. This is the expected pattern for leaf crates.

6. **studio** is completely isolated from the g3 crate ecosystem.
