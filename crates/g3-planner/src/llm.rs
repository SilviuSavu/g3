//! LLM integration for planning mode
//!
//! This module provides LLM-based functionality for:
//! - Requirements refinement
//! - Generating requirements summaries
//! - Generating git commit messages

use anyhow::{anyhow, Context, Result};
use g3_config::Config;
use g3_core::project::Project;
use g3_core::Agent;
use g3_providers::{CompletionRequest, LLMProvider, Message, MessageRole};

use crate::prompts;

/// Create an LLM provider for the planner based on config
pub async fn create_planner_provider(
    config_path: Option<&str>,
) -> Result<Box<dyn LLMProvider>> {
    // Load configuration
    let config = Config::load(config_path)
        .context("Failed to load configuration")?;
    
    // Get planner provider reference (or default)
    let provider_ref = config.get_planner_provider();
    
    // If no explicit planner provider, notify user about fallback
    if config.providers.planner.is_none() {
        let msg = "Note: No 'planner' provider specified in config. Using default_provider '{provider}' for planning mode."
            .replace("{provider}", provider_ref);
        println!("ℹ️  {}", msg);
    }
    
    // Parse the provider reference
    let (provider_type, config_name) = Config::parse_provider_reference(provider_ref)?;
    
    // Create the appropriate provider
    match provider_type.as_str() {
        "anthropic" => {
            let anthropic_config = config
                .get_anthropic_config(&config_name)
                .ok_or_else(|| anyhow!("Anthropic config '{}' not found", config_name))?;
            
            let provider = g3_providers::AnthropicProvider::new_with_name(
                format!("anthropic.{}", config_name),
                anthropic_config.api_key.clone(),
                Some(anthropic_config.model.clone()),
                anthropic_config.max_tokens,
                anthropic_config.temperature,
                anthropic_config.cache_config.clone(),
                anthropic_config.enable_1m_context,
                anthropic_config.thinking_budget_tokens,
            )?;
            Ok(Box::new(provider))
        }
        "openai" => {
            let openai_config = config
                .get_openai_config(&config_name)
                .ok_or_else(|| anyhow!("OpenAI config '{}' not found", config_name))?;
            
            let provider = g3_providers::OpenAIProvider::new_with_name(
                format!("openai.{}", config_name),
                openai_config.api_key.clone(),
                Some(openai_config.model.clone()),
                openai_config.base_url.clone(),
                openai_config.max_tokens,
                openai_config.temperature,
            )?;
            Ok(Box::new(provider))
        }
        "databricks" => {
            let databricks_config = config
                .get_databricks_config(&config_name)
                .ok_or_else(|| anyhow!("Databricks config '{}' not found", config_name))?;
            
            let provider = if let Some(token) = &databricks_config.token {
                g3_providers::DatabricksProvider::from_token_with_name(
                    format!("databricks.{}", config_name),
                    databricks_config.host.clone(),
                    token.clone(),
                    databricks_config.model.clone(),
                    databricks_config.max_tokens,
                    databricks_config.temperature,
                )?
            } else {
                g3_providers::DatabricksProvider::from_oauth_with_name(
                    format!("databricks.{}", config_name),
                    databricks_config.host.clone(),
                    databricks_config.model.clone(),
                    databricks_config.max_tokens,
                    databricks_config.temperature,
                )
                .await?
            };
            Ok(Box::new(provider))
        }
        _ => {
            Err(anyhow!(
                "Unsupported provider type '{}' for planner. Supported: anthropic, openai, databricks",
                provider_type
            ))
        }
    }
}

/// Generate a summary of requirements for planner_history.txt
///
/// Uses the planner LLM to generate a concise summary of the requirements.
/// The summary is at most 5 lines, each at most 120 characters.
pub async fn generate_requirements_summary(
    provider: &dyn LLMProvider,
    requirements: &str,
) -> Result<String> {
    let prompt = prompts::GENERATE_REQUIREMENTS_SUMMARY_PROMPT
        .replace("{requirements}", requirements);

    let messages = vec![Message::new(MessageRole::User, prompt)];

    let request = CompletionRequest {
        messages,
        max_tokens: Some(500), // Summary should be short
        temperature: Some(0.3), // Low temperature for consistent output
        stream: false,
        tools: None,
    };

    let response = provider
        .complete(request)
        .await
        .context("Failed to generate requirements summary")?;

    // Clean up the response - ensure max 5 lines, each max 120 chars
    let summary = response
        .content
        .lines()
        .take(5)
        .map(|line| {
            if line.len() > 120 {
                format!("{}...", &line[..117])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(summary)
}

/// Generate a git commit message based on the requirements
///
/// Uses the planner LLM to generate a commit summary and description.
/// Returns (summary, description) tuple.
pub async fn generate_commit_message(
    provider: &dyn LLMProvider,
    requirements: &str,
    requirements_file: &str,
    todo_file: &str,
) -> Result<(String, String)> {
    let prompt = prompts::GENERATE_COMMIT_MESSAGE_PROMPT
        .replace("{requirements}", requirements)
        .replace("{requirements_file}", requirements_file)
        .replace("{todo_file}", todo_file);

    let messages = vec![Message::new(MessageRole::User, prompt)];

    let request = CompletionRequest {
        messages,
        max_tokens: Some(1000),
        temperature: Some(0.3),
        stream: false,
        tools: None,
    };

    let response = provider
        .complete(request)
        .await
        .context("Failed to generate commit message")?;

    // Parse the response using the existing parse_commit_message function
    Ok(crate::planner::parse_commit_message(&response.content))
}

/// A simple UiWriter implementation for planner output
/// Uses single-line status updates during LLM processing
#[derive(Clone)]
pub struct PlannerUiWriter {
    tool_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for PlannerUiWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl PlannerUiWriter {
    pub fn new() -> Self {
        Self {
            tool_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
    
    /// Clear the current line and print a status message
    fn print_status_line(&self, message: &str) {
        use std::io::Write;
        // Use carriage return to overwrite previous line, pad to 80 chars to clear old content
        print!("\r{:<80}", message);
        std::io::stdout().flush().ok();
    }
}

impl g3_core::ui_writer::UiWriter for PlannerUiWriter {
    fn print(&self, message: &str) {
        println!("{}", message);
    }
    
    fn println(&self, message: &str) {
        println!("{}", message);
    }
    
    fn print_inline(&self, message: &str) {
        print!("{}", message);
    }
    
    fn print_system_prompt(&self, _prompt: &str) {}
    
    fn print_context_status(&self, message: &str) {
        println!("📊 {}", message);
    }
    
    fn print_context_thinning(&self, message: &str) {
        println!("🗜️  {}", message);
    }
    
    fn print_tool_header(&self, tool_name: &str) {
        // Increment tool count and show on single line
        let count = self.tool_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        // Clear the "Thinking..." line and print tool header on new line
        print!("\r{:<80}\n", ""); // Clear status line
        println!("🔧 [{}] {}", count, tool_name);
    }
    
    fn print_tool_arg(&self, _key: &str, _value: &str) {}
    fn print_tool_output_header(&self) {}
    fn update_tool_output_line(&self, _line: &str) {}
    fn print_tool_output_line(&self, _line: &str) {}
    fn print_tool_output_summary(&self, _hidden_count: usize) {}
    fn print_tool_timing(&self, _duration_str: &str) {}
    
    fn print_agent_prompt(&self) {
        // Clear any status line before agent response
        print!("\r{:<80}\n", "");
    }
    
    fn print_agent_response(&self, content: &str) {
        // Display non-tool text messages from LLM
        if !content.trim().is_empty() {
            print!("{}", content);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }
    
    fn notify_sse_received(&self) {
        // Show "Thinking..." status on single line
        self.print_status_line("💭 Thinking...");
    }
    
    fn flush(&self) {
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
    
    fn prompt_user_yes_no(&self, _message: &str) -> bool {
        true // Default to yes for automated planner
    }
    
    fn prompt_user_choice(&self, _message: &str, _options: &[&str]) -> usize {
        0 // Default to first option
    }
    
    fn print_final_output(&self, summary: &str) {
        println!("\n📝 Final Output:\n{}", summary);
    }
}

/// Call LLM to refine requirements using a full Agent with tool execution
pub async fn call_refinement_llm_with_tools(
    config: &Config,
    codepath: &str,
) -> Result<String> {
    // Build system message with codepath context
    let system_prompt = prompts::REFINE_REQUIREMENTS_SYSTEM_PROMPT
        .replace("<codepath>", codepath);

    // Build user message
    let user_message = build_refinement_user_message(codepath);

    // Create agent with planner config
    let planner_config = config.for_planner()?;
    let ui_writer = PlannerUiWriter::new();
    
    // Create project pointing to codepath as workspace
    let workspace = std::path::PathBuf::from(codepath);
    let project = Project::new(workspace.clone());
    project.ensure_workspace_exists()?;
    project.enter_workspace()?;
    
    // Create agent - not autonomous mode, just regular agent with tools
    let mut agent = Agent::new_with_readme_and_quiet(
        planner_config,
        ui_writer,
        Some(system_prompt),
        false, // not quiet
    )
    .await?;
    
    // Execute the refinement task
    // The agent will have access to tools and execute them
    let task = user_message;
    
    let result = agent
        .execute_task_with_timing(&task, None, false, false, false, true, None)
        .await
        .context("Failed to call refinement LLM")?;
    
    println!("📝 Refinement complete");
    
    Ok(result.response)
}

/// Build the user message for requirements refinement
///
/// This message instructs the LLM to read the codebase and refine requirements.
pub fn build_refinement_user_message(codepath: &str) -> String {
    format!(
        r#"Please refine the requirements for the codebase at: {codepath}

Before making suggestions, please:
1. Read the codebase structure using shell commands like `ls`, `find`, or `tree`
2. Read `{codepath}/g3-plan/planner_history.txt` to understand past planning activities
3. Read any `{codepath}/g3-plan/completed_requirements_*.md` files to see what was implemented before
4. Read `{codepath}/g3-plan/new_requirements.md` which contains the requirements to refine

After understanding the context, update the `{codepath}/g3-plan/new_requirements.md` file by prepending
your refined requirements under the heading `{{{{CURRENT REQUIREMENTS}}}}`.

Use final_output when you are done to indicate completion."#,
        codepath = codepath
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_refinement_user_message() {
        let msg = build_refinement_user_message("/test/project");
        assert!(msg.contains("/test/project"));
        assert!(msg.contains("planner_history.txt"));
        assert!(msg.contains("new_requirements.md"));
        assert!(msg.contains("{{CURRENT REQUIREMENTS}}"));
    }
}
