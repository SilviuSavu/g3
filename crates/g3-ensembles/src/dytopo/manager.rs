//! Manager agent for DyTopo halt decisions and round coordination.

use anyhow::Result;
use tokio::sync::mpsc;

use g3_config::Config;
use g3_core::Agent;
use g3_core::get_agent_system_prompt;

use super::channel_ui_writer::ChannelUiWriter;

const MANAGER_SYSTEM_PROMPT: &str = r#"You are a coordination manager for a multi-agent software engineering team.

Your role is to:
1. Evaluate progress reports from worker agents each round
2. Decide whether the task is complete or needs more rounds
3. Provide a refined goal for the next round if continuing

IMPORTANT: Do NOT use any tools. Just analyze the reports and output your decision.

At the END of your response, output a JSON decision block:

```json
{
  "halt": false,
  "reason": "Why you made this decision",
  "next_goal": "Refined goal for the next round (if not halting)"
}
```

Set "halt" to true when the task appears sufficiently complete."#;

#[derive(Debug, Clone)]
pub struct ManagerDecision {
    pub halt: bool,
    pub reason: String,
    pub next_goal: String,
}

pub struct ManagerAgent {
    agent: Agent<ChannelUiWriter>,
    _receiver: mpsc::UnboundedReceiver<String>,
}

impl ManagerAgent {
    pub async fn new(config: Config) -> Result<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let ui_writer = ChannelUiWriter::new(sender);
        let system_prompt = get_agent_system_prompt(MANAGER_SYSTEM_PROMPT, true);
        let agent = Agent::new_with_custom_prompt(config, ui_writer, system_prompt, None).await?;
        Ok(Self { agent, _receiver: receiver })
    }

    pub async fn evaluate_round(
        &mut self,
        round: usize,
        original_task: &str,
        public_messages: &[(String, String)],
    ) -> Result<ManagerDecision> {
        let mut prompt_parts = vec![
            format!("## Round {} Evaluation\n", round),
            format!("**Original task:** {}\n", original_task),
            "**Worker reports:**\n".to_string(),
        ];
        for (agent_id, msg) in public_messages {
            prompt_parts.push(format!("### {} reported:\n{}\n", agent_id, msg));
        }
        prompt_parts.push("\nEvaluate progress and decide: should we halt or continue? Output your JSON decision block.".to_string());
        let full_prompt = prompt_parts.join("\n");

        // Clear buffer before executing
        self.agent.ui_writer().take_buffer();

        let result = self.agent.execute_task_with_timing(
            &full_prompt, None, false, false, false, false, None,
        ).await?;

        // Use buffer which captures ALL streaming output
        let buffer = self.agent.ui_writer().take_buffer();
        let response = if buffer.is_empty() { result.response.clone() } else { buffer };
        parse_manager_decision(&response)
    }
}

pub fn parse_manager_decision(response: &str) -> Result<ManagerDecision> {
    #[derive(serde::Deserialize)]
    struct DecisionJson {
        halt: bool,
        reason: String,
        #[serde(default)]
        next_goal: String,
    }

    let mut json_str = None;
    if let Some(start) = response.rfind("```json") {
        let after = &response[start + 7..];
        if let Some(end) = after.find("```") {
            json_str = Some(after[..end].trim().to_string());
        }
    }
    if json_str.is_none() {
        if let Some(start) = response.rfind('{') {
            if let Some(end) = response[start..].rfind('}') {
                json_str = Some(response[start..=start + end].to_string());
            }
        }
    }
    if let Some(js) = json_str {
        if let Ok(decision) = serde_json::from_str::<DecisionJson>(&js) {
            return Ok(ManagerDecision {
                halt: decision.halt,
                reason: decision.reason,
                next_goal: decision.next_goal,
            });
        }
    }
    tracing::warn!("Could not parse manager decision, defaulting to continue");
    Ok(ManagerDecision {
        halt: false,
        reason: "Could not parse decision, continuing".to_string(),
        next_goal: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_halt_decision() {
        let response = r#"Analysis complete.
```json
{"halt": true, "reason": "Task completed", "next_goal": ""}
```"#;
        let d = parse_manager_decision(response).unwrap();
        assert!(d.halt);
        assert_eq!(d.reason, "Task completed");
    }

    #[test]
    fn test_parse_continue_decision() {
        let response = r#"More work needed.
```json
{"halt": false, "reason": "Tests not passing", "next_goal": "Fix the failing tests"}
```"#;
        let d = parse_manager_decision(response).unwrap();
        assert!(!d.halt);
        assert_eq!(d.next_goal, "Fix the failing tests");
    }

    #[test]
    fn test_parse_invalid_defaults_to_continue() {
        let d = parse_manager_decision("no json here").unwrap();
        assert!(!d.halt);
    }
}
