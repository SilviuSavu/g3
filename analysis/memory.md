### Final Output Test
Test for the final_output tool with TEST_SUCCESS success indicator.

- `crates/g3-core/tests/final_output_test.rs` - Complete test implementation
  - `call_final_output()` - Simulates calling final_output tool with summary
  - `test_test_success_constant()` - Verifies TEST_SUCCESS constant
  - `test_final_output_with_test_success()` - Tests final_output call with TEST_SUCCESS
  - All tests pass (4/4)# Workspace Memory
> Updated: 2026-02-08T02:45:22Z | Size: 24.4k chars

### Remember Tool Wiring
- `crates/g3-core/src/tools/memory.rs` [0..5000] - `execute_remember()`, `get_memory_path()`, `merge_memory()`
- `crates/g3-core/src/tool_definitions.rs` [11000..12000] - remember tool in `create_core_tools()`
- `crates/g3-core/src/tool_dispatch.rs` [48] - dispatch case
- `crates/g3-core/src/prompts.rs` [4200..6500] - Workspace Memory prompt section
- `crates/g3-cli/src/project_files.rs` - `read_workspace_memory()` loads `analysis/memory.md`

### Context Window & Compaction
- `crates/g3-core/src/context_window.rs` [0..29568]
  - `ThinResult` [23] - scope, before/after %, chars_saved
  - `ContextWindow` - token tracking, message history
  - `reset_with_summary()` - compact history to summary
  - `should_compact()` - threshold check (80%)
  - `thin_context()` - replace large results with file refs
- `crates/g3-core/src/compaction.rs` [0..11404]
  - `CompactionResult`, `CompactionConfig` - result/config structs
  - `perform_compaction()` - unified for force_compact() and auto-compaction
  - `calculate_capped_summary_tokens()`, `should_disable_thinking()`
  - `build_summary_messages()`, `apply_summary_fallback_sequence()`
- `crates/g3-core/src/lib.rs` - `Agent.force_compact()`, `stream_completion_with_tools()`

### Session Storage & Continuation
- `crates/g3-core/src/session_continuation.rs` [0..541] - `SessionContinuation`, `save_continuation()`, `load_continuation()`
- `crates/g3-core/src/paths.rs` [0..133] - `get_session_logs_dir()`, `get_thinned_dir()`, `get_session_file()`
- `crates/g3-core/src/session.rs` - Session logging utilities

### Tool System
- `crates/g3-core/src/tool_definitions.rs` [0..544] - `create_core_tools()`, `create_tool_definitions()`, `ToolConfig`
- `crates/g3-core/src/tool_dispatch.rs` [0..73] - `dispatch_tool()` routing

### CLI Module Structure
- `crates/g3-cli/src/lib.rs` [0..415] - `run()`, mode dispatch, config loading
- `crates/g3-cli/src/cli_args.rs` [0..133] - `Cli` struct (clap)
- `crates/g3-cli/src/autonomous.rs` [0..785] - `run_autonomous()`, coach-player loop
- `crates/g3-cli/src/agent_mode.rs` [0..284] - `run_agent_mode()`, `Agent::new_with_custom_prompt()`
- `crates/g3-cli/src/accumulative.rs` [0..343] - `run_accumulative_mode()`
- `crates/g3-cli/src/interactive.rs` [0..851] - `run_interactive()`, `run_interactive_machine()`, REPL
- `crates/g3-cli/src/task_execution.rs` [0..212] - `execute_task_with_retry()`, `OutputMode`
- `crates/g3-cli/src/commands.rs` [17..320] - `/help`, `/compact`, `/thinnify`, `/fragments`, `/rehydrate`
- `crates/g3-cli/src/utils.rs` [0..91] - `display_welcome_message()`, `get_workspace_path()`
- `crates/g3-cli/src/display.rs` - `format_workspace_path()`, `LoadedContent`, `print_loaded_status()`

### Auto-Memory System
- `crates/g3-core/src/lib.rs`
  - `send_auto_memory_reminder()` [47800..48800] - MEMORY CHECKPOINT prompt
  - `set_auto_memory()` [1451..1454] - enable/disable
  - `tool_calls_this_turn` [116] - tracks tools per turn
  - `execute_tool_in_dir()` [2843..2855] - records tool calls
- `crates/g3-core/src/prompts.rs` [3800..4500] - Memory Format in system prompt
- `crates/g3-cli/src/lib.rs` [393] - `--auto-memory` CLI flag

### Streaming Markdown Formatter
- `crates/g3-cli/src/streaming_markdown.rs`
  - `format_header()` [21500..22500] - headers with inline formatting
  - `process_in_code_block()` [439..462] - detects closing fence
  - `emit_code_block()` [654..675] - joins buffer, highlights code
  - `flush_incomplete()` [693..735] - handles unclosed blocks at stream end
- `crates/g3-cli/tests/streaming_markdown_test.rs` - header formatting tests
- **Gotcha**: closing ``` without trailing newline must be detected in `flush_incomplete()`

### Retry Infrastructure
- `crates/g3-core/src/retry.rs` [0..12000] - `execute_with_retry()`, `retry_operation()`, `RetryConfig`, `RetryResult`
- `crates/g3-cli/src/task_execution.rs` - `execute_task_with_retry()`

### UI Abstraction Layer
- `crates/g3-core/src/ui_writer.rs` [0..4500] - `UiWriter` trait, `NullUiWriter`, `print_thin_result()`
- `crates/g3-cli/src/ui_writer_impl.rs` [0..14000] - `ConsoleUiWriter`, `print_tool_compact()`
- `crates/g3-cli/src/simple_output.rs` [0..1200] - `SimpleOutput` helper

### Feedback Extraction
- `crates/g3-core/src/feedback_extraction.rs` [0..22000] - `extract_coach_feedback()`, `try_extract_from_session_log()`, `try_extract_from_native_tool_call()`
- `crates/g3-cli/src/coach_feedback.rs` [0..4025] - `extract_from_logs()` for coach-player loop

### Streaming Utilities & State
- `crates/g3-core/src/streaming.rs` [0..26146]
  - `MAX_ITERATIONS` [13] - constant (400)
  - `StreamingState` [16] - cross-iteration: full_response, first_token_time, iteration_count
  - `ToolOutputFormat` [54] - enum: SelfHandled, Compact(String), Regular
  - `IterationState` [166] - per-iteration: parser, current_response, tool_executed
  - `truncate_line()`, `truncate_for_display()`, `log_stream_error()`, `is_connection_error()`
  - `format_tool_result_summary()`, `is_compact_tool()`, `format_compact_tool_summary()`
- `crates/g3-core/src/lib.rs` [1879..2712] - `stream_completion_with_tools()` main loop

### Background Process Management
- `crates/g3-core/src/background_process.rs` [0..3000] - `BackgroundProcessManager`, `start()`, `list()`, `is_running()`, `get()`, `remove()`
- No `stop()` method - use shell `kill <pid>`

### Unified Diff Application
- `crates/g3-core/src/utils.rs` [5000..15000] - `apply_unified_diff_to_string()`, `parse_unified_diff_hunks()`
- Handles multi-hunk diffs, CRLF normalization, range constraints

### Error Classification
- `crates/g3-core/src/error_handling.rs` [0..567] - `classify_error()`, `ErrorType`, `RecoverableError`
- Priority: rate limit > network > server > busy > timeout > token limit > context length
- **Gotcha**: "Connection timeout" → NetworkError (not Timeout) due to "connection" keyword priority

### CLI Metrics
- `crates/g3-cli/src/metrics.rs` [0..5416] - `TurnMetrics`, `format_elapsed_time()`, `generate_turn_histogram()`

### ACD (Aggressive Context Dehydration)
Saves conversation fragments to disk, replaces with stubs.

- `crates/g3-core/src/acd.rs` [0..22830]
  - `Fragment` - `new()`, `save()`, `load()`, `generate_stub()`, `list_fragments()`, `get_latest_fragment_id()`
- `crates/g3-core/src/tools/acd.rs` [0..8500] - `execute_rehydrate()` tool
- `crates/g3-core/src/paths.rs` [3200..3400] - `get_fragments_dir()` → `.g3/sessions/<id>/fragments/`
- `crates/g3-core/src/compaction.rs` [195..240] - ACD integration, creates fragment+stub
- `crates/g3-core/src/context_window.rs` [10100..10700] - `reset_with_summary_and_stub()`
- `crates/g3-cli/src/lib.rs` [157..161] - `--acd` flag; [1476..1525] - `/fragments`, `/rehydrate`

**Fragment JSON**: `fragment_id`, `created_at`, `messages`, `message_count`, `user_message_count`, `assistant_message_count`, `tool_call_summary`, `estimated_tokens`, `topics`, `preceding_fragment_id`

### UTF-8 Safe String Slicing
Rust `&s[..n]` panics on multi-byte chars (emoji, CJK) if sliced mid-character.

**Pattern**: `s.char_indices().nth(n).map(|(i,_)| i).unwrap_or(s.len())`
**Danger zones**: Display truncation, ACD stubs, user input, non-ASCII text.

### Studio - Multi-Agent Workspace Manager
- `crates/studio/src/main.rs` [0..12500] - `cmd_run()`, `cmd_status()`, `cmd_accept()`, `cmd_discard()`, `extract_session_summary()`
- `crates/studio/src/session.rs` - `Session`, `SessionStatus`
- `crates/studio/src/git.rs` - `GitWorktree` for isolated agent sessions

**Session log**: `<worktree>/.g3/sessions/<session_id>/session.json`
**Fields**: `context_window.{conversation_history, percentage_used, total_tokens, used_tokens}`, `session_id`, `status`, `timestamp`

### Racket Code Search Support
- `crates/g3-core/src/code_search/searcher.rs`
  - Racket parser [~45] - `tree_sitter_racket::LANGUAGE`
  - Extensions [~90] - `.rkt`, `.rktl`, `.rktd` → "racket"

### Language-Specific Prompt Injection
Auto-detects languages and injects toolchain guidance.

- `crates/g3-cli/src/language_prompts.rs`
  - `LANGUAGE_PROMPTS` [12..19] - (lang_name, extensions, prompt_content)
  - `AGENT_LANGUAGE_PROMPTS` [21..26] - (agent_name, lang_name, prompt_content)
  - `detect_languages()` [22..32] - scans workspace
  - `scan_directory_for_extensions()` [42..77] - recursive, depth 2, skips hidden/vendor
  - `get_language_prompts_for_workspace()` [88..108]
  - `get_agent_language_prompts_for_workspace()` [124..137]
- `crates/g3-cli/src/agent_mode.rs` [149..159] - appends agent-specific prompts
- `prompts/langs/` - language prompt files (e.g., `racket.md`, `carmack.racket.md`)

**To add language**: Create `prompts/langs/<lang>.md`, add to `LANGUAGE_PROMPTS`
**To add agent+lang**: Create `prompts/langs/<agent>.<lang>.md`, add to `AGENT_LANGUAGE_PROMPTS`

### MockProvider for Testing
- `crates/g3-providers/src/mock.rs`
  - `MockProvider` [220..320] - response queue, request tracking
  - `MockResponse` [35..200] - configurable chunks and usage
  - `scenarios` module [410..480] - `text_only_response()`, `multi_turn()`, `tool_then_response()`
- `crates/g3-core/tests/mock_provider_integration_test.rs` - integration tests

**Usage**: `MockProvider::new().with_response(MockResponse::text("Hello!"))`

### G3 Status Message Formatting
- `crates/g3-cli/src/g3_status.rs`
  - `Status` [12] - enum: Done, Failed, Error(String), Custom(String), Resolved, Insufficient, NoChanges
  - `G3Status` [44] - static methods for "g3:" prefixed messages
  - `progress()` [48] - "g3: <msg> ..." (no newline)
  - `done()` [72] - bold green "[done]"
  - `failed()` [81] - red "[failed]"
  - `thin_result()` [236] - formats ThinResult with colors
  - `resuming()` [213] - session resume with cyan ID

### Prompt Cache Statistics
- `crates/g3-providers/src/lib.rs` [195..210] - `Usage.cache_creation_tokens`, `cache_read_tokens`
- `crates/g3-providers/src/anthropic.rs` [944..956] - parses `cache_creation_input_tokens`, `cache_read_input_tokens`
- `crates/g3-providers/src/openai.rs` [494..510] - parses `prompt_tokens_details.cached_tokens`
- `crates/g3-core/src/lib.rs` [75..90] - `CacheStats` struct; [106] - `Agent.cache_stats`
- `crates/g3-core/src/stats.rs` [189..230] - `format_cache_stats()` with hit rate metrics

### Embedded Provider (Local LLM)
Local inference via llama-cpp-rs with Metal acceleration.

- `crates/g3-providers/src/embedded.rs`
  - `EmbeddedProvider` [22..85] - session, model_name, max_tokens, temperature, context_length
  - `new()` [26..85] - tilde expansion, auto-downloads Qwen if missing
  - `format_messages()` [87..175] - converts to prompt string (Qwen/Mistral/Llama templates)
  - `get_stop_sequences()` [280..340] - model-specific stop tokens
  - `stream()` [560..780] - via spawn_blocking + mpsc

### Chat Template Formats
| Model | Start Token | End Token |
|-------|-------------|----------|
| Qwen | `<\|im_start\|>role\n` | `<\|im_end\|>` |
| GLM-4 | `[gMASK]<sop><\|role\|>\n` | `<\|endoftext\|>` |
| Mistral | `<s>[INST]` | `[/INST]` |
| Llama | `<<SYS>>` | `<</SYS>>` |

### Recommended GGUF Models
| Model | Size | Use Case |
|-------|------|----------|
| GLM-4-9B-Q8_0 | ~10GB | Fast, capable |
| GLM-4-32B-Q6_K_L | ~27GB | Top tier coding/reasoning |
| Qwen3-4B-Q4_K_M | ~2.3GB | Small, rivals 72B |

**Download**: `huggingface-cli download <repo> --include "<file>" --local-dir ~/.g3/models/`

**Config**:
```toml
[providers.embedded.glm4]
model_path = "~/.g3/models/THUDM_GLM-4-32B-0414-Q6_K_L.gguf"
model_type = "glm4"
context_length = 32768
max_tokens = 4096
gpu_layers = 99
```

### Async Research Tool
Research tool is asynchronous - spawns scout agent in background, returns immediately with research_id.

- `crates/g3-core/src/pending_research.rs`
  - `PendingResearchManager` [80..100] - thread-safe task storage (Arc<Mutex<HashMap>>)
  - `ResearchTask` [40..75] - id, query, status, result, started_at, injected
  - `ResearchStatus` [20..35] - Pending, Complete, Failed enum
  - `register()` [110..125] - creates task, returns research_id
  - `complete()` / `fail()` [130..150] - update task status
  - `take_completed()` [180..200] - returns completed tasks, marks as injected
  - `list_all()` [165..170] - returns all tasks for /research command

- `crates/g3-core/src/tools/research.rs`
  - `execute_research()` [150..210] - spawns scout in tokio::spawn, returns placeholder
  - `run_scout_agent()` [215..300] - async fn that runs in background task
  - `execute_research_status()` [305..380] - check status of pending research

- `crates/g3-core/src/lib.rs`
  - `inject_completed_research()` [1080..1120] - injects completed research into context
  - Called at start of each tool iteration and before user prompt in interactive mode

- `crates/g3-cli/src/commands.rs`
  - `/research` command [125..160] - lists all research tasks with status

**Flow:**
1. Agent calls `research(query)` → returns immediately with research_id
2. Scout agent runs in background tokio task
3. On completion, `PendingResearchManager.complete()` stores result
4. At next iteration start or user prompt, `inject_completed_research()` adds to context
5. Agent can check status with `research_status` tool or user with `/research` command

### Plan Mode (replaces TODO system)
Structured task planning with cognitive forcing - requires happy/negative/boundary checks.

- `crates/g3-core/src/tools/plan.rs`
  - `Plan` [200..240] - plan_id, revision, approved_revision, items[]
  - `PlanItem` [110..145] - id, description, state, touches, checks, evidence, notes
  - `PlanState` [25..45] - enum: Todo, Doing, Done, Blocked
  - `Check` [60..85] - desc, target fields
  - `Checks` [90..105] - happy, negative, boundary
  - `get_plan_path()` [280..285] - returns `.g3/sessions/<id>/plan.g3.md`
  - `read_plan()` [290..310] - loads plan from YAML in markdown
  - `write_plan()` [315..335] - validates and saves plan
  - `plan_verify()` [355..390] - placeholder called when all items done/blocked
  - `execute_plan_read()` [395..420] - plan.read tool
  - `execute_plan_write()` [425..490] - plan.write tool with validation
  - `execute_plan_approve()` [495..530] - plan.approve tool

- `crates/g3-core/src/tool_definitions.rs` [263..330] - plan.read, plan.write, plan.approve definitions
- `crates/g3-core/src/tool_dispatch.rs` [36..38] - dispatch cases for plan tools
- `crates/g3-cli/src/commands.rs` [460..490] - `/plan` command starts Plan Mode
- `crates/g3-core/src/prompts.rs` [21..130] - SHARED_PLAN_SECTION replaces TODO section

**Plan Schema (YAML)**:
```yaml
plan_id: feature-name
revision: 1
approved_revision: 1  # set by plan.approve
items:
  - id: I1
    description: What to do
    state: todo|doing|done|blocked
    touches: [paths/modules]
    checks:
      happy: {desc, target}
      negative: {desc, target}
      boundary: {desc, target}
    evidence: [file:line, test names]  # required when done
    notes: Implementation explanation   # required when done
```

**Workflow**: `/plan <desc>` → agent drafts plan → user approves → agent implements → plan_verify() called when complete

### Plan Mode Tool Names (IMPORTANT)
Tool names must use underscores, not dots (Anthropic API restriction: `^[a-zA-Z0-9_-]{1,128}$`).

- `plan_read` - Read current plan
- `plan_write` - Create/update plan
- `plan_approve` - Approve plan revision

### Plan Verification System
Verifies evidence in completed plan items deterministically.

- `crates/g3-core/src/tools/plan.rs`
  - `EvidenceType` [283..300] - enum: CodeLocation{file_path, start_line, end_line}, TestReference{file_path, test_name}, Unknown
  - `VerificationStatus` [303..320] - enum: Verified, Warning(String), Error(String), Skipped(String)
  - `EvidenceVerification` [330..345] - evidence string + parsed type + status
  - `ItemVerification` [348..365] - item_id, description, evidence_results[], missing_evidence flag
  - `PlanVerification` [368..385] - plan_id, item_results[], skipped_count; has all_passed(), count_issues()
  - `parse_evidence()` [390..428] - parses evidence string into EvidenceType
  - `parse_line_range()` [429..440] - parses "42" or "42-118" into (start, Option<end>)
  - `verify_code_location()` [443..495] - checks file exists, line numbers in range
  - `verify_test_reference()` [496..554] - checks test file exists, searches for fn test_name
  - `verify_single_evidence()` [632..655] - dispatches to appropriate verifier
  - `plan_verify()` [659..700] - iterates done items, collects verification results
  - `format_verification_results()` [703..745] - formats results with emoji, loud warnings

**Evidence formats supported:**
- Code location with range: `src/foo.rs:42-118`
- Code location single line: `src/foo.rs:42`
- Code location file only: `src/foo.rs`
- Test reference: `tests/foo.rs::test_bar`

**Integration:** Called from `execute_plan_write()` when plan is complete and approved (line 828-833)

### Knowledge Graph Data Model (g3-index::graph)
Unified codebase intelligence data model for representing symbols, files, and relationships.

- `crates/g3-index/src/graph.rs` [1..805]
  - `SymbolNode` [84..235] - functions, types, modules with metadata (signature, docs, generics, visibility)
  - `FileNode` [237..295] - files with language, LOC, symbol count, test flag
  - `Edge` [297..327] - 12 relationship types (defines, references, calls, inherits, implements, contains, etc.)
  - `CodeGraph` [329..670] - directed graph with bidirectional reverse_edges index, symbol_name_index, file_language_index
  - `CodeGraph::add_symbol()` [378..424] - adds symbol to graph, updates name index, creates defines/belongs-to edges
  - `CodeGraph::add_reference()` [458..478] - adds reference edge from file to symbol by name lookup
  - `CodeGraph::find_callers()` [580..595] - incoming "calls" edges
  - `CodeGraph::find_callees()` [597..612] - outgoing "calls" edges
  - `CodeGraph::find_references()` [614..624] - incoming "references" edges

**Pattern**: Builder pattern for SymbolNode/FileNode with fluent methods (.with_signature(), .with_documentation(), etc.)
**Pattern**: Bidirectional edge indexing via reverse_edges HashMap enables efficient reverse lookups without graph traversal

### Pattern Search Tool
Find specific code patterns (error handling, async, builder, etc.) across the codebase.

- `crates/g3-core/src/tools/index.rs`
  - `execute_pattern_search()` [100-1100] - main tool entry point, supports 9 pattern types
  - `search_error_handling()` [1100-1120] - finds `?`, `unwrap()`, `expect()`
  - `search_trait_impl()` [1120-1140] - finds `impl Trait for Type`
  - `search_async_pattern()` [1140-1160] - finds `async fn`, `await`
  - `search_struct_initialization()` [1160-1180] - finds `.with_()`, `Self { }`
  - `search_builder_pattern()` [1180-1200] - finds `fn with_()`, `self.` fluent calls
  - `search_lifecycle_patterns()` [1200-1220] - finds `fn new()`, `fn init()`, `fn drop()`
  - `search_concurrency_patterns()` [1220-1240] - finds `Mutex<`, `Arc<`, `RwLock<`
  - `search_config_patterns()` [1240-1260] - finds `struct Config`, `fn load_config()`
  - `search_logging_patterns()` [1260-1280] - finds `info!()`, `debug!()`, `error!()`
- `crates/g3-core/src/tool_definitions.rs` [950-970] - `pattern_search` tool definition
- `crates/g3-core/src/tool_dispatch.rs` [112] - dispatch case: `"pattern_search" => index::execute_pattern_search`

**Pattern types**: error_handling, trait_impl, async_pattern, struct_init, builder_pattern, lifecycle, concurrency, config, logging

### G3 Workspace Build Status
The entire g3 workspace is built and available.

**Release binaries:**
- `target/release/g3` (42 MB, last modified Feb 8 00:40)

**Debug binaries:**
- `target/debug/g3` (92 MB)
- `target/debug/studio` (multi-agent workspace manager)

**All crates compiled:**
- g3, g3-cli, g3-computer-control, g3-config, g3-core, g3-execution, g3-index, g3-lsp, g3-planner, g3-providers, studio

**Build commands:**
- `cargo build --release` - production binary
- `cargo build` - debug binary with all crates
- `cargo test` - run all tests
- `cargo check` - quick type check without full build

### Latest Features Verification (February 2026)
All latest features are properly wired and tested:

**Core Tools (17 total)**:
- switch_mode - agent mode switching
- complexity_metrics - code complexity analysis
- list_files - glob pattern file filtering
- list_directory - directory exploration
- preview_file - quick file previews
- pattern_search - code pattern discovery
- code_intelligence - graph-based code analysis

**Codebase Intelligence System**:
- crates/g3-index/src/unified_index.rs - unified API
- crates/g3-index/src/traverser.rs - BFS/DFS/graph traversal
- crates/g3-core/src/tools/intelligence.rs - 7 subcommands

**Test Results**:
- 25 tool_definitions tests: PASS
- 16 tool execution roundtrip tests: PASS
- 18 integration blackbox tests: PASS
- 22 intelligence system tests: PASS
- 500+ total tests passing

**Build Status**:
- Release binary: target/release/g3 (42 MB)
- Debug binary: target/debug/g3 (92 MB)
- studio binary: target/debug/studio (5.4 MB)

### Latest Features Verification (February 2026)
All latest features are properly wired and tested:

**Core Tools (17 total)**:
- switch_mode - agent mode switching
- complexity_metrics - code complexity analysis
- list_files - glob pattern file filtering
- list_directory - directory exploration
- preview_file - quick file previews
- pattern_search - code pattern discovery
- code_intelligence - graph-based code analysis

**Codebase Intelligence System**:
- crates/g3-index/src/unified_index.rs - unified API
- crates/g3-index/src/traverser.rs - BFS/DFS/graph traversal
- crates/g3-core/src/tools/intelligence.rs - 7 subcommands

**Test Results**:
- 25 tool_definitions tests: PASS
- 16 tool execution roundtrip tests: PASS
- 18 integration blackbox tests: PASS
- 22 intelligence system tests: PASS
- 500+ total tests passing

**Build Status**:
- Release binary: target/release/g3 (42 MB)
- Debug binary: target/debug/g3 (92 MB)
- studio binary: target/debug/studio (5.4 MB)

### Interactive Mode Selection Menu
When running `g3` without arguments, displays a mode selection menu.

- `crates/g3-cli/src/interactive.rs` [665..752]
  - `ModeSelection` enum [668-678] - 6 mode variants
  - `run_mode_selection()` [680-752] - displays menu, handles input

- `crates/g3-cli/src/lib.rs` [101..143]
  - Mode selection check in `run()` [101-143] - calls menu, modifies CLI flags

**Workflow**:
1. User runs `g3` (no args) → menu appears
2. User selects mode by number (1-6) or name
3. CLI flags are set based on selection
4. Appropriate mode runner is called

**Input handling**:
- Empty input → re-prompt
- Invalid input → shows [failed], re-prompt
- "exit"/"quit" → exits gracefully
- EOF (Ctrl-D) → exits
- Name matching is case-insensitive

**6 Modes**:
1. Interactive (default chat)
2. Autonomous (coach-player loop)
3. Accumulative (evolutionary requirements)
4. Agent (specialized agents)
5. Planning (requirements-driven)
6. Studio (multi-agent)

**CLI flag behavior**:
- If any mode flag is set (--agent, --autonomous, --auto, --planning, --chat, --task) → skips menu
- Menu only appears when no mode flags are provided

### Mode Selection Fix (February 2026)
Fixed planning mode selection from mode selection menu.

- `crates/g3-cli/src/lib.rs` [132..140] - When mode selection returns ModeSelection::Planning, return early to call g3_planner::run_planning_mode()
- `crates/g3-cli/src/lib.rs` [255..258] - Removed duplicate planning mode banner (printed by g3-planner)
- `crates/g3-planner/src/lib.rs` [747..950] - run_planning_mode() function handles planning mode orchestration
- `crates/g3-planner/src/planner.rs` [747..1200] - Main planning mode implementation with codepath prompt, requirements refinement, and implementation loop

**Flow:**
1. User runs g3 without mode flags → mode selection menu appears
2. User selects "5" or "planning" → ModeSelection::Planning returned
3. Mode selection logic sets cli.planning = true and returns early to call g3_planner::run_planning_mode()
4. Planning mode starts with codepath prompt, requirements refinement, and coach/player loop

### Session Management
The g3 workspace stores session logs in `.g3/sessions/<session_id>/` with session state in `session.json`. The `.g3/session` symlink points to the current/active session. Session cleanup can be performed by removing old session directories.