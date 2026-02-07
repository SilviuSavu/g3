//! Tool dispatch module - routes tool calls to their implementations.
//!
//! This module provides a clean dispatch mechanism that routes tool calls
//! to the appropriate handler in the `tools/` module.

use anyhow::Result;
use tracing::{debug, warn};

use crate::tools::executor::ToolContext;
use crate::tools::{acd, beads, file_ops, index, intelligence, lsp, mcp_tools, memory, misc, plan, research, shell, webdriver, zai_tools};
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Dispatch a tool call to the appropriate handler.
///
/// This function routes tool calls to their implementations in the `tools/` module,
/// providing a single point of dispatch for all tool execution.
pub async fn dispatch_tool<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Dispatching tool: {}", tool_call.tool);

    match tool_call.tool.as_str() {
        // Shell tools
        "shell" => shell::execute_shell(tool_call, ctx).await,
        "background_process" => shell::execute_background_process(tool_call, ctx).await,

        // File operations
        "read_file" => file_ops::execute_read_file(tool_call, ctx).await,
        "read_image" => file_ops::execute_read_image(tool_call, ctx).await,
        "write_file" => file_ops::execute_write_file(tool_call, ctx).await,
        "str_replace" => file_ops::execute_str_replace(tool_call, ctx).await,

        // Plan Mode
        "plan_read" => plan::execute_plan_read(tool_call, ctx).await,
        "plan_write" => plan::execute_plan_write(tool_call, ctx).await,
        "plan_approve" => plan::execute_plan_approve(tool_call, ctx).await,

        // Miscellaneous tools
        "screenshot" => misc::execute_take_screenshot(tool_call, ctx).await,
        "coverage" => misc::execute_code_coverage(tool_call, ctx).await,
        "code_search" => misc::execute_code_search(tool_call, ctx).await,

        // Research tool
        "research" => research::execute_research(tool_call, ctx).await,
        "research_status" => research::execute_research_status(tool_call, ctx).await,

        // Workspace memory tools
        "remember" => memory::execute_remember(tool_call, ctx).await,

        // ACD (Aggressive Context Dehydration) tools
        "rehydrate" => acd::execute_rehydrate(tool_call, ctx).await,

        // WebDriver tools
        "webdriver_start" => webdriver::execute_webdriver_start(tool_call, ctx).await,
        "webdriver_navigate" => webdriver::execute_webdriver_navigate(tool_call, ctx).await,
        "webdriver_get_url" => webdriver::execute_webdriver_get_url(tool_call, ctx).await,
        "webdriver_get_title" => webdriver::execute_webdriver_get_title(tool_call, ctx).await,
        "webdriver_find_element" => webdriver::execute_webdriver_find_element(tool_call, ctx).await,
        "webdriver_find_elements" => webdriver::execute_webdriver_find_elements(tool_call, ctx).await,
        "webdriver_click" => webdriver::execute_webdriver_click(tool_call, ctx).await,
        "webdriver_send_keys" => webdriver::execute_webdriver_send_keys(tool_call, ctx).await,
        "webdriver_execute_script" => webdriver::execute_webdriver_execute_script(tool_call, ctx).await,
        "webdriver_get_page_source" => webdriver::execute_webdriver_get_page_source(tool_call, ctx).await,
        "webdriver_screenshot" => webdriver::execute_webdriver_screenshot(tool_call, ctx).await,
        "webdriver_back" => webdriver::execute_webdriver_back(tool_call, ctx).await,
        "webdriver_forward" => webdriver::execute_webdriver_forward(tool_call, ctx).await,
        "webdriver_refresh" => webdriver::execute_webdriver_refresh(tool_call, ctx).await,
        "webdriver_quit" => webdriver::execute_webdriver_quit(tool_call, ctx).await,

        // Z.ai standalone tools
        "zai_web_search" => zai_tools::execute_web_search(tool_call, ctx).await,
        "zai_web_reader" => zai_tools::execute_web_reader(tool_call, ctx).await,
        "zai_ocr" => zai_tools::execute_ocr(tool_call, ctx).await,

        // MCP tools (Z.ai MCP servers)
        "mcp_web_search" => mcp_tools::execute_mcp_web_search(tool_call, ctx).await,
        "mcp_web_reader" => mcp_tools::execute_mcp_web_reader(tool_call, ctx).await,
        "mcp_search_doc" => mcp_tools::execute_mcp_search_doc(tool_call, ctx).await,
        "mcp_get_repo_structure" => mcp_tools::execute_mcp_get_repo_structure(tool_call, ctx).await,
        "mcp_read_file" => mcp_tools::execute_mcp_read_file(tool_call, ctx).await,

        // Beads tools (issue tracking and molecule workflows)
        "beads_ready" => beads::execute_beads_ready(tool_call, ctx).await,
        "beads_create" => beads::execute_beads_create(tool_call, ctx).await,
        "beads_update" => beads::execute_beads_update(tool_call, ctx).await,
        "beads_close" => beads::execute_beads_close(tool_call, ctx).await,
        "beads_show" => beads::execute_beads_show(tool_call, ctx).await,
        "beads_list" => beads::execute_beads_list(tool_call, ctx).await,
        "beads_dep" => beads::execute_beads_dep(tool_call, ctx).await,
        "beads_sync" => beads::execute_beads_sync(tool_call, ctx).await,
        "beads_prime" => beads::execute_beads_prime(tool_call, ctx).await,

        // Beads molecule/formula tools
        "beads_formula_list" => beads::execute_formula_list(tool_call, ctx).await,
        "beads_formula_cook" => beads::execute_formula_cook(tool_call, ctx).await,
        "beads_mol_pour" => beads::execute_mol_pour(tool_call, ctx).await,
        "beads_mol_wisp" => beads::execute_mol_wisp(tool_call, ctx).await,
        "beads_mol_current" => beads::execute_mol_current(tool_call, ctx).await,
        "beads_mol_squash" => beads::execute_mol_squash(tool_call, ctx).await,

        // Index tools (codebase indexing and semantic search)
        "index_codebase" => index::execute_index_codebase(tool_call, ctx).await,
        "semantic_search" => index::execute_semantic_search(tool_call, ctx).await,
        "index_status" => index::execute_index_status(tool_call, ctx).await,

        // Self-improvement tools
        "list_directory" => index::execute_list_directory(tool_call, ctx).await,
        "preview_file" => index::execute_preview_file(tool_call, ctx).await,

        // Knowledge Graph tools
        "list_files" => index::execute_list_files(tool_call, ctx).await,
        "graph_find_symbol" => index::execute_graph_find_symbol(tool_call, ctx).await,
        "graph_file_symbols" => index::execute_graph_file_symbols(tool_call, ctx).await,
        "graph_find_callers" => index::execute_graph_find_callers(tool_call, ctx).await,
        "graph_find_references" => index::execute_graph_find_references(tool_call, ctx).await,
        "graph_stats" => index::execute_graph_stats(tool_call, ctx).await,

        // Code Intelligence tool
        "code_intelligence" => intelligence::execute_code_intelligence(tool_call, ctx).await,

        // LSP tools (code intelligence)
        "lsp_goto_definition" => lsp::execute_goto_definition(tool_call, ctx).await,
        "lsp_find_references" => lsp::execute_find_references(tool_call, ctx).await,
        "lsp_hover" => lsp::execute_hover(tool_call, ctx).await,
        "lsp_document_symbols" => lsp::execute_document_symbols(tool_call, ctx).await,
        "lsp_workspace_symbols" => lsp::execute_workspace_symbols(tool_call, ctx).await,
        "lsp_goto_implementation" => lsp::execute_goto_implementation(tool_call, ctx).await,
        "lsp_call_hierarchy" => lsp::execute_call_hierarchy(tool_call, ctx).await,
        "lsp_diagnostics" => lsp::execute_diagnostics(tool_call, ctx).await,
        "lsp_status" => lsp::execute_status(tool_call, ctx).await,

        // Unknown tool
        _ => {
            warn!("Unknown tool: {}", tool_call.tool);
            Ok(format!("❓ Unknown tool: {}", tool_call.tool))
        }
    }
}
