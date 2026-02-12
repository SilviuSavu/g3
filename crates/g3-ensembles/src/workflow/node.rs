//! Workflow node definitions.
//!
//! A node represents a single step in the workflow, executed by a specific agent role.

use super::AgentRole;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a workflow node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Custom prompt template for this node
    #[serde(default)]
    pub prompt: Option<String>,
    /// Maximum turns for this node
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    /// Timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Input mappings: workflow state key -> agent input key
    #[serde(default)]
    pub input_mapping: HashMap<String, String>,
    /// Output mappings: agent output key -> workflow state key
    #[serde(default)]
    pub output_mapping: HashMap<String, String>,
    /// Retry count on failure
    #[serde(default)]
    pub retry_count: usize,
    /// Whether to fail the workflow if this node fails
    #[serde(default = "default_critical")]
    pub critical: bool,
}

fn default_max_turns() -> usize { 10 }
fn default_timeout_secs() -> u64 { 300 }
fn default_critical() -> bool { true }

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            prompt: None,
            max_turns: default_max_turns(),
            timeout_secs: default_timeout_secs(),
            input_mapping: HashMap::new(),
            output_mapping: HashMap::new(),
            retry_count: 0,
            critical: default_critical(),
        }
    }
}

/// A node in the workflow graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique node identifier
    pub id: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Agent role that executes this node
    pub agent_role: AgentRole,
    /// Node configuration
    #[serde(default)]
    pub config: NodeConfig,
    /// Custom agent persona file to use
    #[serde(default)]
    pub persona: Option<String>,
}

impl Node {
    /// Create a new node with the given ID and agent role
    pub fn new(id: impl Into<String>, agent_role: AgentRole) -> Self {
        Self {
            id: id.into(),
            description: String::new(),
            agent_role,
            config: NodeConfig::default(),
            persona: None,
        }
    }
    
    /// Add a description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
    
    /// Set a custom prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.prompt = Some(prompt.into());
        self
    }
    
    /// Add an input mapping
    pub fn input(mut self, state_key: impl Into<String>, agent_key: impl Into<String>) -> Self {
        self.config.input_mapping.insert(state_key.into(), agent_key.into());
        self
    }
    
    /// Add an output mapping
    pub fn output(mut self, agent_key: impl Into<String>, state_key: impl Into<String>) -> Self {
        self.config.output_mapping.insert(agent_key.into(), state_key.into());
        self
    }
    
    /// Set max turns
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.config.max_turns = max_turns;
        self
    }
    
    /// Set timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.config.timeout_secs = timeout_secs;
        self
    }
    
    /// Set retry count
    pub fn with_retry(mut self, retry_count: usize) -> Self {
        self.config.retry_count = retry_count;
        self
    }
    
    /// Mark as non-critical (workflow continues on failure)
    pub fn non_critical(mut self) -> Self {
        self.config.critical = false;
        self
    }
    
    /// Use a custom persona file
    pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
        self.persona = Some(persona.into());
        self
    }
}

/// Result of executing a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    /// Node ID that was executed
    pub node_id: String,
    /// Whether the node succeeded
    pub success: bool,
    /// Output from the agent
    pub output: String,
    /// Any error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub duration_ms: u64,
    /// Number of turns used
    pub turns_used: usize,
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl NodeResult {
    /// Create a successful result
    pub fn success(node_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            success: true,
            output: output.into(),
            error: None,
            duration_ms: 0,
            turns_used: 0,
            metadata: HashMap::new(),
        }
    }
    
    /// Create a failed result
    pub fn failure(node_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            success: false,
            output: String::new(),
            error: Some(error.into()),
            duration_ms: 0,
            turns_used: 0,
            metadata: HashMap::new(),
        }
    }
    
    /// Set duration
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }
    
    /// Set turns used
    pub fn with_turns(mut self, turns: usize) -> Self {
        self.turns_used = turns;
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_builder() {
        let node = Node::new("analyze", AgentRole::Researcher)
            .with_description("Analyze the codebase")
            .input("task", "user_request")
            .output("analysis", "research_findings")
            .with_max_turns(5)
            .with_timeout(60);
        
        assert_eq!(node.id, "analyze");
        assert_eq!(node.agent_role, AgentRole::Researcher);
        assert_eq!(node.config.input_mapping.get("task"), Some(&"user_request".to_string()));
        assert_eq!(node.config.output_mapping.get("analysis"), Some(&"research_findings".to_string()));
        assert_eq!(node.config.max_turns, 5);
        assert_eq!(node.config.timeout_secs, 60);
    }
    
    #[test]
    fn test_node_result() {
        let result = NodeResult::success("test", "Analysis complete")
            .with_duration(1500)
            .with_turns(3)
            .with_metadata("files_analyzed", serde_json::json!(42));
        
        assert!(result.success);
        assert_eq!(result.output, "Analysis complete");
        assert_eq!(result.duration_ms, 1500);
        assert_eq!(result.turns_used, 3);
        assert_eq!(result.metadata.get("files_analyzed"), Some(&serde_json::json!(42)));
    }
}
