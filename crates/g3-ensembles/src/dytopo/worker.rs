//! Worker agent wrapper for DyTopo rounds.

use anyhow::Result;
use tokio::sync::mpsc;

use g3_config::Config;
use g3_core::Agent;
use g3_core::get_agent_system_prompt;

use super::channel_ui_writer::ChannelUiWriter;
use super::descriptor::{AgentRoundOutput, DescriptorBlock};
use super::message::Message;

const DESCRIPTOR_PROMPT: &str = r#"

IMPORTANT: At the END of your response, you MUST output a JSON block with your collaboration descriptors. This is required for multi-agent coordination.

```json
{
  "public": "Your public message visible to the coordinator summarizing your work",
  "private": "Your detailed analysis/work to share with relevant peers",
  "query": "Brief description of what information or help you need from other agents",
  "key": "Brief description of what expertise or output you can offer to other agents"
}
```
"#;

pub struct AgentWorker {
    pub agent_id: String,
    agent: Agent<ChannelUiWriter>,
    _receiver: mpsc::UnboundedReceiver<String>,
}

impl AgentWorker {
    pub async fn new(agent_id: String, persona_prompt: &str, config: Config) -> Result<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let ui_writer = ChannelUiWriter::new(sender);
        let persona_with_descriptors = format!("{}\n{}", persona_prompt, DESCRIPTOR_PROMPT);
        let system_prompt = get_agent_system_prompt(&persona_with_descriptors, true);
        let agent = Agent::new_with_custom_prompt(config, ui_writer, system_prompt, None).await?;
        Ok(Self { agent_id, agent, _receiver: receiver })
    }

    pub async fn run_round(
        &mut self,
        round_goal: &str,
        incoming_messages: &[Message],
        round: usize,
    ) -> Result<AgentRoundOutput> {
        let mut prompt_parts = vec![format!("## Round {} Task\n\n{}", round, round_goal)];
        if !incoming_messages.is_empty() {
            prompt_parts.push(super::message::format_inbox(incoming_messages));
        }
        let full_prompt = prompt_parts.join("\n\n");

        // Clear buffer before executing
        self.agent.ui_writer().take_buffer();

        let result = self.agent.execute_task_with_timing(
            &full_prompt, None, false, false, false, false, None,
        ).await?;

        // Use buffer which captures ALL streaming output (result.response may be empty in autonomous mode)
        let buffer = self.agent.ui_writer().take_buffer();
        let response = if buffer.is_empty() { &result.response } else { &buffer };

        match DescriptorBlock::parse_from_response(response) {
            Some(block) => Ok(block.into_round_output()),
            None => {
                tracing::warn!("Worker '{}' did not produce valid descriptor block, using fallback", self.agent_id);
                Ok(AgentRoundOutput {
                    public_msg: response.clone(),
                    private_msg: response.clone(),
                    query: String::new(),
                    key: String::new(),
                })
            }
        }
    }
}
