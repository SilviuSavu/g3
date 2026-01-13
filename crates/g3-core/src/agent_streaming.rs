//! Streaming completion logic for the Agent.
//!
//! This module contains the streaming-related methods for Agent, extracted
//! from lib.rs for maintainability. These methods handle:
//! - Streaming completions from LLM providers
//! - Retry logic for recoverable errors
//! - Tool call detection and execution during streaming
//! - Context management during streaming (compaction, thinning)

use anyhow::Result;
use g3_providers::{CompletionRequest, Message, MessageRole};
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use crate::{
    compaction::{CompactionConfig, perform_compaction},
    context_window::ContextWindow,
    error_handling::ErrorContext,
    streaming,
    streaming_parser::StreamingToolParser,
    task_result::TaskResult,
    tool_definitions,
    ui_writer::UiWriter,
    ToolCall,
};

/// Helper function to parse diff stats from str_replace result.
/// Result format: "✅ +N insertions | -M deletions"
pub(crate) fn parse_diff_stats(result: &str) -> (i32, i32) {
    let mut insertions = 0i32;
    let mut deletions = 0i32;
    
    // Look for "+N insertions" pattern
    if let Some(pos) = result.find('+') {
        let after_plus = &result[pos + 1..];
        insertions = after_plus.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
    }
    // Look for "-M deletions" pattern  
    if let Some(pos) = result.find('-') {
        let after_minus = &result[pos + 1..];
        deletions = after_minus.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
    }
    (insertions, deletions)
}

impl<W: UiWriter> crate::Agent<W> {
    /// Stream a completion request, delegating to stream_completion_with_tools.
    pub(crate) async fn stream_completion(
        &mut self,
        request: CompletionRequest,
        show_timing: bool,
    ) -> Result<TaskResult> {
        self.stream_completion_with_tools(request, show_timing)
            .await
    }

    /// Helper method to stream with retry logic.
    pub(crate) async fn stream_with_retry(
        &self,
        request: &CompletionRequest,
        error_context: &ErrorContext,
    ) -> Result<g3_providers::CompletionStream> {
        use crate::error_handling::{calculate_retry_delay, classify_error, ErrorType};

        let mut attempt = 0;
        let max_attempts = if self.is_autonomous {
            self.config.agent.autonomous_max_retry_attempts
        } else {
            self.config.agent.max_retry_attempts
        };

        loop {
            attempt += 1;
            let provider = self.providers.get(None)?;

            match provider.stream(request.clone()).await {
                Ok(stream) => {
                    if attempt > 1 {
                        debug!("Stream started successfully after {} attempts", attempt);
                    }
                    debug!("Stream started successfully");
                    debug!(
                        "Request had {} messages, tools={}, max_tokens={:?}",
                        request.messages.len(),
                        request.tools.is_some(),
                        request.max_tokens
                    );
                    return Ok(stream);
                }
                Err(e) if attempt < max_attempts => {
                    if matches!(classify_error(&e), ErrorType::Recoverable(_)) {
                        let delay = calculate_retry_delay(attempt, self.is_autonomous);
                        warn!(
                            "Recoverable error on attempt {}/{}: {}. Retrying in {:?}...",
                            attempt, max_attempts, e, delay
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        error_context.clone().log_error(&e);
                        return Err(e);
                    }
                }
                Err(e) => {
                    error_context.clone().log_error(&e);
                    return Err(e);
                }
            }
        }
    }

    /// Main streaming completion method with tool execution support.
    ///
    /// This is the core streaming loop that:
    /// 1. Handles context compaction/thinning before streaming
    /// 2. Streams chunks from the LLM provider
    /// 3. Detects and executes tool calls
    /// 4. Manages auto-continue logic for autonomous mode
    /// 5. Tracks timing and usage metrics
    pub(crate) async fn stream_completion_with_tools(
        &mut self,
        mut request: CompletionRequest,
        show_timing: bool,
    ) -> Result<TaskResult> {
        use tokio_stream::StreamExt;

        debug!("Starting stream_completion_with_tools");

        let mut full_response = String::new();
        let mut first_token_time: Option<Duration> = None;
        let stream_start = Instant::now();
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 400; // Prevent infinite loops
        let mut response_started = false;
        let mut any_tool_executed = false; // Track if ANY tool was executed across all iterations
        let mut auto_summary_attempts = 0; // Track auto-summary prompt attempts
        const MAX_AUTO_SUMMARY_ATTEMPTS: usize = 5; // Limit auto-summary retries (increased from 2 for better recovery)
        // 
        // Note: Session-level duplicate tracking was removed - we only prevent sequential duplicates (DUP IN CHUNK, DUP IN MSG)
        let mut turn_accumulated_usage: Option<g3_providers::Usage> = None; // Track token usage for timing footer

        // Check if we need to compact before starting
        if self.context_window.should_compact() {
            self.handle_pre_stream_compaction(&mut request).await?;
        }

        loop {
            iteration_count += 1;
            debug!("Starting iteration {}", iteration_count);
            if iteration_count > MAX_ITERATIONS {
                warn!("Maximum iterations reached, stopping stream");
                break;
            }

            // Add a small delay between iterations to prevent "model busy" errors
            if iteration_count > 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            // Get provider info for logging, then drop it to avoid borrow issues
            let (provider_name, provider_model) = {
                let provider = self.providers.get(None)?;
                (provider.name().to_string(), provider.model().to_string())
            };
            debug!("Got provider: {}", provider_name);

            // Create error context for detailed logging
            let last_prompt = request
                .messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, MessageRole::User))
                .map(|m| m.content.clone())
                .unwrap_or_else(|| "No user message found".to_string());

            let error_context = ErrorContext::new(
                "stream_completion".to_string(),
                provider_name.clone(),
                provider_model.clone(),
                last_prompt,
                self.session_id.clone(),
                self.context_window.used_tokens,
                self.quiet,
            )
            .with_request(
                serde_json::to_string(&request)
                    .unwrap_or_else(|_| "Failed to serialize request".to_string()),
            );

            // Log initial request details
            debug!("Starting stream with provider={}, model={}, messages={}, tools={}, max_tokens={:?}",
                provider_name,
                provider_model,
                request.messages.len(),
                request.tools.is_some(),
                request.max_tokens
            );

            // Try to get stream with retry logic
            let mut stream = match self.stream_with_retry(&request, &error_context).await {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to start stream: {}", e);
                    // Additional retry for "busy" errors on subsequent iterations
                    if iteration_count > 1 && e.to_string().contains("busy") {
                        warn!(
                            "Model busy on iteration {}, attempting one more retry in 500ms",
                            iteration_count
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                        match self.stream_with_retry(&request, &error_context).await {
                            Ok(s) => s,
                            Err(e2) => {
                                error!("Failed to start stream after retry: {}", e2);
                                error_context.clone().log_error(&e2);
                                return Err(e2);
                            }
                        }
                    } else {
                        return Err(e);
                    }
                }
            };

            // Write context window summary every time we send messages to LLM
            self.write_context_window_summary();

            let mut parser = StreamingToolParser::new();
            let mut current_response = String::new();
            let mut tool_executed = false;
            let mut chunks_received = 0;
            let mut raw_chunks: Vec<String> = Vec::new(); // Store raw chunks for debugging
            let mut _last_error: Option<String> = None;
            let mut accumulated_usage: Option<g3_providers::Usage> = None;
            let mut stream_stop_reason: Option<String> = None; // Track why the stream stopped

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        // Notify UI about SSE received (including pings)
                        self.ui_writer.notify_sse_received();

                        // Capture usage data if available
                        if let Some(ref usage) = chunk.usage {
                            accumulated_usage = Some(usage.clone());
                            turn_accumulated_usage = Some(usage.clone());
                            debug!(
                                "Received usage data - prompt: {}, completion: {}, total: {}",
                                usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                            );
                        }

                        // Store raw chunk for debugging (limit to first 20 and last 5)
                        if chunks_received < 20 || chunk.finished {
                            raw_chunks.push(format!(
                                "Chunk #{}: content={:?}, finished={}, tool_calls={:?}",
                                chunks_received + 1,
                                chunk.content,
                                chunk.finished,
                                chunk.tool_calls
                            ));
                        } else if raw_chunks.len() == 20 {
                            raw_chunks.push("... (chunks 21+ omitted for brevity) ...".to_string());
                        }

                        // Record time to first token
                        if first_token_time.is_none() && !chunk.content.is_empty() {
                            first_token_time = Some(stream_start.elapsed());
                            // Record in agent metrics
                            if let Some(ttft) = first_token_time {
                                self.first_token_times.push(ttft);
                            }
                        }

                        chunks_received += 1;
                        if chunks_received == 1 {
                            debug!(
                                "First chunk received: content_len={}, finished={}",
                                chunk.content.len(),
                                chunk.finished
                            );
                        }

                        // Process chunk with the new parser
                        let completed_tools = parser.process_chunk(&chunk);

                        // Handle completed tool calls - process all if multiple calls enabled
                        // Always process all tool calls - they will be executed after stream ends
                        let tools_to_process: Vec<ToolCall> = completed_tools;

                        // De-duplicate tool calls and track duplicates
                        let mut last_tool_in_chunk: Option<ToolCall> = None;
                        let mut deduplicated_tools: Vec<(ToolCall, Option<String>)> = Vec::new();

                        for tool_call in tools_to_process {
                            let mut duplicate_type = None;

                            // Check for IMMEDIATELY SEQUENTIAL duplicate in current chunk
                            // Only the immediately previous tool call counts as a duplicate
                            if let Some(ref last_tool) = last_tool_in_chunk {
                                if streaming::are_tool_calls_duplicate(last_tool, &tool_call) {
                                duplicate_type = Some("DUP IN CHUNK".to_string());
                                }
                            } else {
                                // Check for duplicate against previous message
                                duplicate_type = self.check_duplicate_in_previous_message(&tool_call);
                            }

                            // Track the last tool call for sequential duplicate detection
                            last_tool_in_chunk = Some(tool_call.clone());

                            deduplicated_tools.push((tool_call, duplicate_type));
                        }

                        // Process each tool call
                        for (tool_call, duplicate_type) in deduplicated_tools {
                            debug!("Processing completed tool call: {:?}", tool_call);
                            
                            // If it's a duplicate, log it and skip - don't set tool_executed!
                            // Setting tool_executed for duplicates would trigger auto-continue
                            // even when no actual tool execution occurred.
                            if let Some(dup_type) = &duplicate_type {
                                // Log the duplicate with red prefix
                                let prefixed_tool_name =
                                    format!("🟥 {} {}", tool_call.tool, dup_type);
                                let warning_msg = format!(
                                    "⚠️ Duplicate tool call detected ({}): Skipping execution of {} with args {}",
                                    dup_type,
                                    tool_call.tool,
                                    serde_json::to_string(&tool_call.args).unwrap_or_else(|_| "<unserializable>".to_string())
                                );

                                // Log to tool log with red prefix
                                let mut modified_tool_call = tool_call.clone();
                                modified_tool_call.tool = prefixed_tool_name;
                                debug!("{}", warning_msg);

                                // NOTE: Do NOT call parser.reset() here!
                                // Resetting the parser clears the entire text buffer, which would
                                // lose any subsequent (non-duplicate) tool calls that haven't been
                                // processed yet.
                                continue; // Skip execution of duplicate
                            }

                            // Check if we should auto-compact at 90% BEFORE executing the tool
                            // We need to do this before any borrows of self
                            if self.auto_compact && self.context_window.percentage_used() >= 90.0 {
                                // Set flag to trigger compaction after this turn completes
                                // We can't do it now due to borrow checker constraints
                                self.pending_90_compaction = true;
                            }

                            // Check if we should thin the context BEFORE executing the tool
                            if self.context_window.should_thin() {
                                let thin_summary = self.do_thin_context();
                                // Print the thinning summary
                                self.ui_writer.print_context_thinning(&thin_summary);
                            }

                            // Track what we've already displayed before getting new text
                            // This prevents re-displaying old content after tool execution
                            let already_displayed_chars = current_response.chars().count();

                            // Get the text content accumulated so far
                            let text_content = parser.get_text_content();

                            // Clean the content
                            let clean_content = streaming::clean_llm_tokens(&text_content);

                            // Store the raw content BEFORE filtering for the context window log
                            let raw_content_for_log = clean_content.clone();

                            // Filter out JSON tool calls from the display
                            let filtered_content =
                                self.ui_writer.filter_json_tool_calls(&clean_content);
                            let final_display_content = filtered_content.trim();

                            // Display any new content before tool execution
                            // We need to skip what was already shown (tracked in current_response)
                            // but also account for the fact that parser.text_buffer accumulates
                            // across iterations and is never cleared until reset()
                            let new_content =
                                if current_response.len() <= final_display_content.len() {
                                    // Only show content that hasn't been displayed yet
                                    final_display_content
                                        .chars()
                                        .skip(already_displayed_chars)
                                        .collect::<String>()
                                } else {
                                    // Nothing new to display
                                    String::new()
                                };

                            // Display any new text content
                            if !new_content.trim().is_empty()  {
                                #[allow(unused_assignments)]
                                if !response_started {
                                    self.ui_writer.print_agent_prompt();
                                    response_started = true;
                                }
                                self.ui_writer.print_agent_response(&new_content);
                                self.ui_writer.flush();
                                // Update current_response to track what we've displayed
                                current_response.push_str(&new_content);
                            }

                            // Execute the tool with formatted output

                            // Finish streaming markdown before showing tool output
                            self.ui_writer.finish_streaming_markdown();

                            // Tool call header
                            self.ui_writer.print_tool_header(&tool_call.tool, Some(&tool_call.args));
                            if let Some(args_obj) = tool_call.args.as_object() {
                                for (key, value) in args_obj {
                                    let value_str = streaming::format_tool_arg_value(
                                        &tool_call.tool,
                                        key,
                                        value,
                                    );
                                    self.ui_writer.print_tool_arg(key, &value_str);
                                }
                            }

                            // Check if this is a compact tool (file operations)
                            let is_compact_tool = matches!(tool_call.tool.as_str(), "read_file" | "write_file" | "str_replace" | "remember" | "take_screenshot" | "code_coverage" | "rehydrate");
                            
                            // Only print output header for non-compact tools
                            if !is_compact_tool {
                                self.ui_writer.print_tool_output_header();
                            }

                            // Clone working_dir to avoid borrow checker issues
                            let working_dir = self.working_dir.clone();
                            let exec_start = Instant::now();
                            // Add 8-minute timeout for tool execution
                            let tool_result = match tokio::time::timeout(
                                Duration::from_secs(8 * 60), // 8 minutes
                                // Use working_dir if set (from --codebase-fast-start)
                                self.execute_tool_in_dir(&tool_call, working_dir.as_deref()),
                            )
                            .await
                            {
                                Ok(result) => result?,
                                Err(_) => {
                                    warn!("Tool call {} timed out after 8 minutes", tool_call.tool);
                                    "❌ Tool execution timed out after 8 minutes".to_string()
                                }
                            };
                            let exec_duration = exec_start.elapsed();

                            // Track tool call metrics
                            let tool_success = !tool_result.contains("❌");
                            self.tool_call_metrics.push((
                                tool_call.tool.clone(),
                                exec_duration,
                                tool_success,
                            ));

                            // Display tool execution result with proper indentation
                            let compact_summary = self.format_tool_output(
                                &tool_call,
                                &tool_result,
                                tool_success,
                                is_compact_tool,
                            );

                            // Add the tool call and result to the context window using RAW unfiltered content
                            // This ensures the log file contains the true raw content including JSON tool calls
                            let tool_message = if !raw_content_for_log.trim().is_empty() {
                                Message::new(
                                    MessageRole::Assistant,
                                    format!(
                                        "{}\n\n{{\"tool\": \"{}\", \"args\": {}}}",
                                        raw_content_for_log.trim(),
                                        tool_call.tool,
                                        tool_call.args
                                    ),
                                )
                            } else {
                                // No text content before tool call, just include the tool call
                                Message::new(
                                    MessageRole::Assistant,
                                    format!(
                                        "{{\"tool\": \"{}\", \"args\": {}}}",
                                        tool_call.tool, tool_call.args
                                    ),
                                )
                            };
                            let mut result_message = {
                                let content = format!("Tool result: {}", tool_result);
                                
                                // Apply cache control every 10 tool calls (max 4 annotations)
                                let should_cache = self.tool_call_count > 0
                                    && self.tool_call_count % 10 == 0
                                    && self.count_cache_controls_in_history() < 4;
                                
                                if should_cache {
                                    let provider = self.providers.get(None)?;
                                    if let Some(cache_config) = self.get_provider_cache_control() {
                                        Message::with_cache_control_validated(
                                            MessageRole::User,
                                            content,
                                            cache_config,
                                            provider,
                                        )
                                    } else {
                                        Message::new(MessageRole::User, content)
                                    }
                                } else {
                                    Message::new(MessageRole::User, content)
                                }
                            };

                            // Attach any pending images to the result message
                            // (images loaded via read_image tool)
                            if !self.pending_images.is_empty() {
                                result_message.images = std::mem::take(&mut self.pending_images);
                            }

                            // Track tokens before adding messages
                            let tokens_before = self.context_window.used_tokens;

                            self.context_window.add_message(tool_message);
                            self.context_window.add_message(result_message);

                            // Closure marker with timing
                            let tokens_delta = self.context_window.used_tokens.saturating_sub(tokens_before);
                            
                            // Use compact format for file operations, normal format for others
                            if let Some(summary) = compact_summary {
                                self.ui_writer.print_tool_compact(
                                    &tool_call.tool,
                                    &summary,
                                    &streaming::format_duration(exec_duration),
                                    tokens_delta,
                                    self.context_window.percentage_used(),
                                );
                            } else {
                                self.ui_writer
                                    .print_tool_timing(&streaming::format_duration(exec_duration),
                                        tokens_delta,
                                        self.context_window.percentage_used());
                            }
                            self.ui_writer.print_agent_prompt();

                            // Update the request with the new context for next iteration
                            request.messages = self.context_window.conversation_history.clone();

                            // Ensure tools are included for native providers in subsequent iterations
                            let provider_for_tools = self.providers.get(None)?;
                            if provider_for_tools.has_native_tool_calling() {
                                let mut tool_config = tool_definitions::ToolConfig::new(
                                        self.config.webdriver.enabled,
                                        self.config.computer_control.enabled,
                                    );
                                // Exclude research tool for scout agent to prevent recursion
                                if self.agent_name.as_deref() == Some("scout") {
                                    tool_config = tool_config.with_research_excluded();
                                }
                                request.tools = Some(tool_definitions::create_tool_definitions(tool_config));
                            }

                            // DO NOT add final_display_content to full_response here!
                            // The content was already displayed during streaming and added to current_response.
                            // Adding it again would cause duplication when the agent message is printed.
                            // The only time we should add to full_response is:
                            // 1. At the end when no tools were executed
                            // 2. At the end when no tools were executed (handled in the "no tool executed" branch)

                            tool_executed = true;
                            any_tool_executed = true; // Track across all iterations

                            // Reset auto-continue attempts after successful tool execution
                            // This gives the LLM fresh attempts since it's making progress
                            auto_summary_attempts = 0;


                            // Reset the JSON tool call filter state after each tool execution
                            // This ensures the filter doesn't stay in suppression mode for subsequent streaming content
                            self.ui_writer.reset_json_filter();

                            // Only reset parser if there are no more unexecuted tool calls in the buffer
                            // This handles the case where the LLM emits multiple tool calls in one response
                            if parser.has_unexecuted_tool_call() {
                                debug!("Parser still has unexecuted tool calls, not resetting buffer");
                                // Mark current tool as consumed so we don't re-detect it
                                parser.mark_tool_calls_consumed();
                            } else {
                                // Reset parser for next iteration - this clears the text buffer
                                parser.reset();
                            }

                            // Clear current_response for next iteration to prevent buffered text
                            // from being incorrectly displayed after tool execution
                            current_response.clear();
                            // Reset response_started flag for next iteration
                            response_started = false;

                            // Continue processing - don't break mid-stream
                        } // End of for loop processing each tool call

                        // Note: We no longer break mid-stream after tool execution.
                        // All tool calls are collected and executed after the stream ends.

                        // If no tool calls were completed, continue streaming normally
                        if !tool_executed {
                            let clean_content = streaming::clean_llm_tokens(&chunk.content);

                            if !clean_content.is_empty() {
                                let filtered_content =
                                    self.ui_writer.filter_json_tool_calls(&clean_content);

                                if !filtered_content.is_empty() {
                                    if !response_started {
                                        self.ui_writer.print_agent_prompt();
                                        response_started = true;
                                    }

                                    self.ui_writer.print_agent_response(&filtered_content);
                                    self.ui_writer.flush();
                                    current_response.push_str(&filtered_content);

                                    // Mark parser buffer as consumed up to current position
                                    // This prevents tool-call-like patterns in displayed text
                                    // from triggering false positives in has_unexecuted_tool_call()
                                    parser.mark_tool_calls_consumed();
                                }
                            }
                        }

                        if chunk.finished {
                            debug!("Stream finished: tool_executed={}, current_response_len={}, full_response_len={}, chunks_received={}",
                                tool_executed, current_response.len(), full_response.len(), chunks_received);
                            
                            // Capture the stop reason from the final chunk
                            if let Some(ref reason) = chunk.stop_reason {
                                debug!("Stream stop_reason: {}", reason);
                                stream_stop_reason = Some(reason.clone());
                            }

                            // Stream finished - check if we should continue or return
                            if !tool_executed {
                                // No tools were executed in this iteration
                                // Check if we got any meaningful response at all
                                // We need to check the parser's text buffer as well, since the LLM
                                // might have responded with text but no tool calls
                                let text_content = parser.get_text_content();
                                let has_text_response = !text_content.trim().is_empty()
                                    || !current_response.trim().is_empty();

                                // Don't re-add text from parser buffer if we already displayed it
                                // The parser buffer contains ALL accumulated text, but current_response
                                // already has what was displayed during streaming
                                if current_response.is_empty() && !text_content.trim().is_empty() {
                                    // Only use parser text if we truly have no response
                                    // This should be rare - only if streaming failed to display anything
                                    debug!("Warning: Using parser buffer text as fallback - this may duplicate output");
                                    // Extract only the undisplayed portion from parser buffer
                                    // Parser buffer accumulates across iterations, so we need to be careful
                                    let clean_text = streaming::clean_llm_tokens(&text_content);

                                    let filtered_text =
                                        self.ui_writer.filter_json_tool_calls(&clean_text);

                                    // Only use this if we truly have nothing else
                                    if !filtered_text.trim().is_empty() && full_response.is_empty()
                                    {
                                        debug!(
                                            "Using filtered parser text as last resort: {} chars",
                                            filtered_text.len()
                                        );
                                        // Note: This assignment is currently unused but kept for potential future use
                                        let _ = filtered_text;
                                    }
                                }

                                if !has_text_response && full_response.is_empty() {
                                    streaming::log_stream_error(
                                        iteration_count,
                                        &provider_name,
                                        &provider_model,
                                        chunks_received,
                                        &parser,
                                        &request,
                                        &self.context_window,
                                        self.session_id.as_deref(),
                                        &raw_chunks,
                                    );

                                    // No response received - this is an error condition
                                    warn!("Stream finished without any content or tool calls");
                                    warn!("Chunks received: {}", chunks_received);
                                    return Err(anyhow::anyhow!(
                                        "No response received from the model. The model may be experiencing issues or the request may have been malformed."
                                    ));
                                }

                                // If tools were executed in previous iterations,
                                // break to let the outer loop's auto-continue logic handle it
                                if any_tool_executed  {
                                    debug!("Tools were executed, continuing - breaking to auto-continue");
                                    // IMPORTANT: Save any text response to context window before breaking
                                    // This ensures text displayed after tool execution is not lost
                                    if !current_response.trim().is_empty() {
                                        debug!("Saving current_response ({} chars) to context before auto-continue", current_response.len());
                                        let assistant_msg = Message::new(
                                            MessageRole::Assistant,
                                            current_response.clone(),
                                        );
                                        self.context_window.add_message(assistant_msg);
                                    }
                                    
                                    // NOTE: We intentionally do NOT set full_response here.
                                    // The content was already displayed during streaming.
                                    // Setting full_response would cause duplication when the
                                    // function eventually returns.
                                    // Context window is updated separately via add_message().
                                    break;
                                }

                                // Set full_response to empty to avoid duplication in return value
                                // (content was already displayed during streaming)
                                full_response = String::new();

                                // Finish the streaming markdown formatter before returning
                                self.ui_writer.finish_streaming_markdown();

                                // Save context window BEFORE returning
                                self.save_context_window("completed");
                                let _ttft =
                                    first_token_time.unwrap_or_else(|| stream_start.elapsed());

                                // Add timing if needed
                                let final_response = if show_timing {
                                    let turn_tokens = turn_accumulated_usage.as_ref().map(|u| u.total_tokens);
                                    let timing_footer = streaming::format_timing_footer(
                                        stream_start.elapsed(),
                                        _ttft,
                                        turn_tokens,
                                        self.context_window.percentage_used(),
                                    );
                                    format!(
                                        "{}\n\n{}",
                                        full_response,
                                        timing_footer
                                    )
                                } else {
                                    full_response
                                };

                                // Dehydrate context - the function extracts the summary from context itself
                                self.dehydrate_context();

                                return Ok(TaskResult::new(
                                    final_response,
                                    self.context_window.clone(),
                                ));
                            }
                            break; // Tool was executed, break to continue outer loop
                        }
                    }
                    Err(e) => {
                        // Capture detailed streaming error information
                        let error_msg = e.to_string();
                        let error_details = format!(
                            "Streaming error at chunk {}: {}",
                            chunks_received + 1,
                            error_msg
                        );

                        error!("Error type: {}", std::any::type_name_of_val(&e));
                        error!("Parser state at error: text_buffer_len={}, has_incomplete={}, message_stopped={}",
                            parser.text_buffer_len(), parser.has_incomplete_tool_call(), parser.is_message_stopped());

                        // Store the error for potential logging later
                        _last_error = Some(error_details.clone());

                        // Check if this is a recoverable connection error
                        let is_connection_error = streaming::is_connection_error(&error_msg);

                        if is_connection_error {
                            warn!(
                                "Connection error at chunk {}, treating as end of stream",
                                chunks_received + 1
                            );
                            // If we have any content or tool calls, treat this as a graceful end
                            if chunks_received > 0
                                && (!parser.get_text_content().is_empty()
                                    || parser.has_unexecuted_tool_call())
                            {
                                warn!("Stream terminated unexpectedly but we have content, continuing");
                                break; // Break to process what we have
                            }
                        }

                        if tool_executed {
                            error!("{}", error_details);
                            warn!("Stream error after tool execution, attempting to continue");
                            break; // Break to outer loop to start new stream
                        } else {
                            // Log raw chunks before failing
                            error!("Fatal streaming error. Raw chunks received before error:");
                            for chunk_str in raw_chunks.iter().take(10) {
                                error!("  {}", chunk_str);
                            }
                            return Err(e);
                        }
                    }
                }
            }

            // Update context window with actual usage if available
            if let Some(usage) = accumulated_usage {
                debug!("Updating context window with actual usage from stream");
                self.context_window.update_usage_from_response(&usage);
            } else {
                // Fall back to estimation if no usage data was provided
                debug!("No usage data from stream, using estimation");
                let estimated_tokens = ContextWindow::estimate_tokens(&current_response);
                self.context_window.add_streaming_tokens(estimated_tokens);
            }

            // If we get here and no tool was executed, we're done
            if !tool_executed {
                // IMPORTANT: Do NOT add parser text_content here!
                // The text has already been displayed during streaming via current_response.
                // The parser buffer accumulates ALL text and would cause duplication.
                debug!("Stream completed without tool execution. Response already displayed during streaming.");
                debug!(
                    "Current response length: {}, Full response length: {}",
                    current_response.len(),
                    full_response.len()
                );

                let has_response = !current_response.is_empty() || !full_response.is_empty();

                // Check if the response is essentially empty (just whitespace or timing lines)
                // This detects cases where the LLM outputs nothing substantive
                let response_text = if !current_response.is_empty() {
                    &current_response
                } else {
                    &full_response
                };
                let is_empty_response = streaming::is_empty_response(response_text);

                // Check if there's an incomplete tool call in the buffer
                let has_incomplete_tool_call = parser.has_incomplete_tool_call();

                // Check if there's a complete but unexecuted tool call in the buffer
                let has_unexecuted_tool_call = parser.has_unexecuted_tool_call();

                // Log when we detect unexecuted or incomplete tool calls for debugging
                if has_incomplete_tool_call {
                    debug!("Detected incomplete tool call in buffer (buffer_len={}, consumed_up_to={})",
                        parser.text_buffer_len(), parser.text_buffer_len());
                }
                if has_unexecuted_tool_call {
                    debug!("Detected unexecuted tool call in buffer - this may indicate a parsing issue");
                    warn!("Unexecuted tool call detected in buffer after stream ended");
                }
                
                // Check if the response was truncated due to max_tokens
                let was_truncated_by_max_tokens = stream_stop_reason.as_deref() == Some("max_tokens");
                if was_truncated_by_max_tokens {
                    debug!("Response was truncated due to max_tokens limit");
                    warn!("LLM response was cut off due to max_tokens limit - will auto-continue");
                }

                // Auto-continue if tools were executed and we are in autonomous mode
                // OR if the LLM emitted an incomplete tool call (truncated JSON)
                // OR if the LLM emitted a complete tool call that wasn't executed
                // OR if the response was truncated due to max_tokens
                // This ensures we don't return control when the LLM clearly intended to call a tool
                // Note: We removed the redundant condition (any_tool_executed && is_empty_response)
                // because it's already covered by (any_tool_executed )
                // Auto-continue is only enabled in autonomous mode - in interactive mode,
                // the user may be asking questions and we should return control to them
                let should_auto_continue = self.is_autonomous && ((any_tool_executed ) 
                    || has_incomplete_tool_call 
                    || has_unexecuted_tool_call
                    || was_truncated_by_max_tokens);
                if should_auto_continue {
                    if auto_summary_attempts < MAX_AUTO_SUMMARY_ATTEMPTS {
                        auto_summary_attempts += 1;
                        if has_incomplete_tool_call {
                            warn!(
                                "LLM emitted incomplete tool call ({} iterations, auto-continue attempt {}/{})",
                                iteration_count, auto_summary_attempts, MAX_AUTO_SUMMARY_ATTEMPTS
                            );
                            self.ui_writer.print_context_status(
                                "\n🔄 Model emitted incomplete tool call. Auto-continuing...\n"
                            );
                        } else if has_unexecuted_tool_call {
                            warn!(
                                "LLM emitted unexecuted tool call ({} iterations, auto-continue attempt {}/{})",
                                iteration_count, auto_summary_attempts, MAX_AUTO_SUMMARY_ATTEMPTS
                            );
                            self.ui_writer.print_context_status(
                                "\n🔄 Model emitted tool call that wasn't executed. Auto-continuing...\n"
                            );
                        } else if is_empty_response {
                            warn!(
                                "LLM emitted empty/trivial response ({} iterations, auto-continue attempt {}/{})",
                                iteration_count, auto_summary_attempts, MAX_AUTO_SUMMARY_ATTEMPTS
                            );
                            self.ui_writer.print_context_status(
                                "\n🔄 Model emitted empty response. Auto-continuing...\n"
                            );
                        } else {
                            warn!(
                                "LLM stopped after executing tools ({} iterations, auto-continue attempt {}/{})",
                                iteration_count, auto_summary_attempts, MAX_AUTO_SUMMARY_ATTEMPTS
                            );
                            self.ui_writer.print_context_status(
                                "\n🔄 Model stopped without providing summary. Auto-continuing...\n"
                            );
                        }
                        
                        // Add any text response to context before prompting for continuation
                        if has_response {
                            let response_text = if !current_response.is_empty() {
                                current_response.clone()
                            } else {
                                full_response.clone()
                            };
                            if !response_text.trim().is_empty() {
                                let assistant_msg = Message::new(
                                    MessageRole::Assistant,
                                    response_text.trim().to_string(),
                                );
                                self.context_window.add_message(assistant_msg);
                            }
                        }
                        
                        // Add a follow-up message asking for continuation
                        let continue_prompt = if has_incomplete_tool_call {
                            Message::new(
                                MessageRole::User,
                                "Your previous response was cut off mid-tool-call. Please complete the tool call and continue.".to_string(),
                            )
                        } else {
                            Message::new(
                                MessageRole::User,
                                "Please continue until you are done. Provide a summary when complete.".to_string(),
                            )
                        };
                        self.context_window.add_message(continue_prompt);
                        request.messages = self.context_window.conversation_history.clone();
                        
                        // Continue the loop
                        continue;
                    } else {
                        // Max attempts reached, give up gracefully
                        warn!(
                            "Max auto-continue attempts ({}) reached after {} iterations. Conditions: any_tool_executed={}, has_incomplete={}, has_unexecuted={}, is_empty_response={}",
                            MAX_AUTO_SUMMARY_ATTEMPTS,
                            iteration_count,
                            any_tool_executed,
                            
                            has_incomplete_tool_call,
                            has_unexecuted_tool_call,
                            is_empty_response
                        );
                        self.ui_writer.print_agent_response(
                            &format!("\n⚠️ The model stopped without providing a summary after {} auto-continue attempts.\n", MAX_AUTO_SUMMARY_ATTEMPTS)
                        );
                    }
                } else if has_response {
                    // Only set full_response if it's empty (first iteration without tools)
                    // This prevents duplication when the agent responds
                    // NOTE: We intentionally do NOT set full_response here anymore.
                    // The content was already displayed during streaming via print_agent_response().
                    // Setting full_response would cause the CLI to print it again.
                    // We only need full_response for the context window (handled separately).
                    debug!(
                        "Response already streamed, not setting full_response. current_response: {} chars",
                        current_response.len()
                    );
                }

                let _ttft = first_token_time.unwrap_or_else(|| stream_start.elapsed());

                // Add the RAW unfiltered response to context window before returning.
                // This ensures the log contains the true raw content including any JSON.
                // Note: We check current_response, not full_response, because full_response
                // may be empty to avoid display duplication (content was already streamed).
                if !current_response.trim().is_empty() {
                    // Get the raw text from the parser (before filtering)
                    let raw_text = parser.get_text_content();
                    let raw_clean = streaming::clean_llm_tokens(&raw_text);

                    if !raw_clean.trim().is_empty() {
                        let assistant_message = Message::new(MessageRole::Assistant, raw_clean);
                        self.context_window.add_message(assistant_message);
                    }
                }

                // Save context window BEFORE returning
                self.save_context_window("completed");
                
                // Add timing if needed
                let final_response = if show_timing {
                    let turn_tokens = turn_accumulated_usage.as_ref().map(|u| u.total_tokens);
                    let timing_footer = streaming::format_timing_footer(
                        stream_start.elapsed(),
                        _ttft,
                        turn_tokens,
                        self.context_window.percentage_used(),
                    );
                    format!(
                        "{}\n\n{}",
                        full_response,
                        timing_footer
                    )
                } else {
                    full_response
                };

                // Finish streaming markdown before returning
                self.ui_writer.finish_streaming_markdown();

                // Dehydrate context - the function extracts the summary from context itself
                self.dehydrate_context();

                return Ok(TaskResult::new(final_response, self.context_window.clone()));
            }

            // Continue the loop to start a new stream with updated context
        }

        // If we exit the loop due to max iterations
        let _ttft = first_token_time.unwrap_or_else(|| stream_start.elapsed());

        // Add timing if needed
        let final_response = if show_timing {
            let turn_tokens = turn_accumulated_usage.as_ref().map(|u| u.total_tokens);
            let timing_footer = streaming::format_timing_footer(
                stream_start.elapsed(),
                _ttft,
                turn_tokens,
                self.context_window.percentage_used(),
            );
            format!(
                "{}\n\n{}",
                full_response,
                timing_footer
            )
        } else {
            full_response
        };

        // Dehydrate context - the function extracts the summary from context itself
        self.dehydrate_context();

        Ok(TaskResult::new(final_response, self.context_window.clone()))
    }

    /// Handle pre-stream compaction if context window is near capacity.
    async fn handle_pre_stream_compaction(&mut self, request: &mut CompletionRequest) -> Result<()> {
        // First try thinning if we are at capacity, don't call the LLM for compaction (might fail)
        if self.context_window.percentage_used() > 90.0 && self.context_window.should_thin() {
            self.ui_writer.print_context_status(&format!(
                "\n🥒 Context window at {}%. Trying thinning first...",
                self.context_window.percentage_used() as u32
            ));

            let thin_summary = self.do_thin_context();
            self.ui_writer.print_context_thinning(&thin_summary);

            // Check if thinning was sufficient
            if !self.context_window.should_compact() {
                self.ui_writer.print_context_status(
                    "✅ Thinning resolved capacity issue. Continuing...\n",
                );
                return Ok(());
            } else {
                self.ui_writer.print_context_status(
                    "⚠️ Thinning insufficient. Proceeding with compaction...\n",
                );
            }
        }

        // Only proceed with compaction if still needed after thinning
        if self.context_window.should_compact() {
            // Notify user about compaction
            self.ui_writer.print_context_status(&format!(
                "\n🗜️ Context window reaching capacity ({}%). Compacting...",
                self.context_window.percentage_used() as u32
            ));

            let provider = self.providers.get(None)?;
            let provider_name = provider.name().to_string();
            let _ = provider; // Release borrow early

            // Extract the latest user message from the request (not context_window)
            let latest_user_msg = request
                .messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, MessageRole::User))
                .map(|m| m.content.clone());

            let compaction_config = CompactionConfig {
                provider_name: &provider_name,
                latest_user_msg,
            };

            let result = perform_compaction(
                &self.providers,
                &mut self.context_window,
                &self.config,
                compaction_config,
                &self.ui_writer,
                &mut self.thinning_events,
            ).await?;

            if result.success {
                self.ui_writer.print_context_status(
                    "✅ Context compacted successfully. Continuing...\n",
                );
                self.compaction_events.push(result.chars_saved);

                // Update the request with new context
                request.messages = self.context_window.conversation_history.clone();
            } else {
                self.ui_writer.print_context_status("⚠️ Unable to compact context. Consider starting a new session if you continue to see errors.\n");
                // Don't continue with the original request if compaction failed
                // as we're likely at token limit
                return Err(anyhow::anyhow!("Context window at capacity and compaction failed. Please start a new session."));
            }
        }

        Ok(())
    }

    /// Format tool output for display, returning a compact summary if applicable.
    fn format_tool_output(
        &self,
        tool_call: &ToolCall,
        tool_result: &str,
        tool_success: bool,
        is_compact_tool: bool,
    ) -> Option<String> {
        let output_lines: Vec<&str> = tool_result.lines().collect();

        // Check if UI wants full output (machine mode) or truncated (human mode)
        let wants_full = self.ui_writer.wants_full_output();

        const MAX_LINES: usize = 5;
        const MAX_LINE_WIDTH: usize = 80;
        let output_len = output_lines.len();

        // Skip printing content for todo tools - they already print their content
        let is_todo_tool = tool_call.tool == "todo_read" || tool_call.tool == "todo_write";

        if is_compact_tool {
            // For failed compact tools, show truncated error message
            if !tool_success {
                let error_msg = streaming::truncate_for_display(tool_result, 60);
                Some(error_msg)
            } else {
                // Generate appropriate summary based on tool type
                match tool_call.tool.as_str() {
                    "read_file" => Some(streaming::format_read_file_summary(output_len, tool_result.len())),
                    "write_file" => {
                        // The tool result already contains the formatted summary
                        // Format: "✅ wrote N lines | M chars"
                        Some(streaming::format_write_file_result(tool_result))
                    }
                    "str_replace" => {
                        // Parse insertions/deletions from result
                        // Result format: "✅ +N insertions | -M deletions"
                        let (ins, del) = parse_diff_stats(tool_result);
                        Some(streaming::format_str_replace_summary(ins, del))
                    }
                    "remember" => {
                        // Extract size from result like "Memory updated. Size: 1.2k"
                        Some(streaming::format_remember_summary(tool_result))
                    }
                    "take_screenshot" => {
                        // Extract path from result
                        Some(streaming::format_screenshot_summary(tool_result))
                    }
                    "code_coverage" => {
                        // Show coverage summary
                        Some(streaming::format_coverage_summary(tool_result))
                    }
                    "rehydrate" => {
                        // Show fragment info
                        Some(streaming::format_rehydrate_summary(tool_result))
                    }
                    _ => Some("✅ completed".to_string())
                }
            }
        } else if is_todo_tool {
            // Skip - todo tools print their own content
            None
        } else {
            let max_lines_to_show = if wants_full { output_len } else { MAX_LINES };

            for (idx, line) in output_lines.iter().enumerate() {
                if !wants_full && idx >= max_lines_to_show {
                    break;
                }
                let clipped_line = streaming::truncate_line(line, MAX_LINE_WIDTH, !wants_full);
                self.ui_writer.update_tool_output_line(&clipped_line);
            }

            if !wants_full && output_len > MAX_LINES {
                self.ui_writer.print_tool_output_summary(output_len);
            }
            None
        }
    }
}
