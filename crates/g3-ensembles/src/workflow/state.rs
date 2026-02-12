//! Workflow state management.
//!
//! Workflow state is passed between nodes and persists across the workflow execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use uuid::Uuid;

/// Execution step record for history tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    /// Node ID that was executed
    pub node_id: String,
    /// When execution started
    pub started_at: DateTime<Utc>,
    /// When execution completed
    pub completed_at: Option<DateTime<Utc>>,
    /// Whether the step succeeded
    pub success: bool,
    /// Brief summary of what happened
    pub summary: String,
}

/// Workflow state - passed between nodes during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Unique workflow instance ID
    pub workflow_id: String,
    /// Workflow name
    pub workflow_name: String,
    /// Arbitrary state data
    pub data: HashMap<String, JsonValue>,
    /// Execution history
    pub history: Vec<ExecutionStep>,
    /// Current node being executed (if any)
    pub current_node: Option<String>,
    /// Number of iterations (edge traversals)
    pub iteration: usize,
    /// When the workflow started
    pub started_at: DateTime<Utc>,
    /// When the workflow completed (if finished)
    pub completed_at: Option<DateTime<Utc>>,
    /// Initial user request
    pub initial_request: String,
}

impl WorkflowState {
    /// Create a new workflow state
    pub fn new(workflow_name: impl Into<String>) -> Self {
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            workflow_name: workflow_name.into(),
            data: HashMap::new(),
            history: Vec::new(),
            current_node: None,
            iteration: 0,
            started_at: Utc::now(),
            completed_at: None,
            initial_request: String::new(),
        }
    }
    
    /// Create state with initial request
    pub fn with_request(workflow_name: impl Into<String>, request: impl Into<String>) -> Self {
        let mut state = Self::new(workflow_name);
        state.initial_request = request.into();
        state.set("user_request", serde_json::json!(&state.initial_request));
        state
    }
    
    /// Get a value from state
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.data.get(key)
    }
    
    /// Get a value as a specific type
    pub fn get_as<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.data.get(key).and_then(|v| T::deserialize(v.clone()).ok())
    }
    
    /// Set a value in state
    pub fn set(&mut self, key: impl Into<String>, value: JsonValue) {
        self.data.insert(key.into(), value);
    }
    
    /// Set a typed value
    pub fn set_typed<T: Serialize>(&mut self, key: impl Into<String>, value: &T) {
        if let Ok(json) = serde_json::to_value(value) {
            self.data.insert(key.into(), json);
        }
    }
    
    /// Check if a key exists
    pub fn contains(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
    
    /// Remove a key
    pub fn remove(&mut self, key: &str) -> Option<JsonValue> {
        self.data.remove(key)
    }
    
    /// Clear all state data
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    /// Record the start of a node execution
    pub fn start_node(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.current_node = Some(id.clone());
        self.history.push(ExecutionStep {
            node_id: id,
            started_at: Utc::now(),
            completed_at: None,
            success: false,
            summary: String::new(),
        });
    }
    
    /// Complete the current node execution
    pub fn complete_node(&mut self, success: bool, summary: impl Into<String>) {
        if let Some(step) = self.history.last_mut() {
            step.completed_at = Some(Utc::now());
            step.success = success;
            step.summary = summary.into();
        }
        self.current_node = None;
    }
    
    /// Increment iteration counter
    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
    }
    
    /// Mark workflow as completed
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }
    
    /// Check if workflow is completed
    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }
    
    /// Get total execution time in milliseconds
    pub fn duration_ms(&self) -> i64 {
        let end = self.completed_at.unwrap_or_else(Utc::now);
        (end - self.started_at).num_milliseconds()
    }
    
    /// Get a summary of the current state
    pub fn summary(&self) -> StateSummary {
        StateSummary {
            workflow_id: self.workflow_id.clone(),
            workflow_name: self.workflow_name.clone(),
            current_node: self.current_node.clone(),
            iteration: self.iteration,
            total_steps: self.history.len(),
            successful_steps: self.history.iter().filter(|s| s.success).count(),
            duration_ms: self.duration_ms(),
            is_completed: self.is_completed(),
        }
    }
    
    /// Merge data from another state (for aggregating parallel results)
    pub fn merge(&mut self, other: &WorkflowState, prefix: Option<&str>) {
        for (key, value) in &other.data {
            let final_key = match prefix {
                Some(p) => format!("{}.{}", p, key),
                None => key.clone(),
            };
            self.data.insert(final_key, value.clone());
        }
    }
}

/// Summary of workflow state (for logging/display)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSummary {
    pub workflow_id: String,
    pub workflow_name: String,
    pub current_node: Option<String>,
    pub iteration: usize,
    pub total_steps: usize,
    pub successful_steps: usize,
    pub duration_ms: i64,
    pub is_completed: bool,
}

impl std::fmt::Display for StateSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Workflow: {} ({})", self.workflow_name, self.workflow_id)?;
        writeln!(f, "  Status: {}", if self.is_completed { "completed" } else { "in progress" })?;
        if let Some(ref node) = self.current_node {
            writeln!(f, "  Current node: {}", node)?;
        }
        writeln!(f, "  Iteration: {}", self.iteration)?;
        writeln!(f, "  Steps: {}/{} successful", self.successful_steps, self.total_steps)?;
        writeln!(f, "  Duration: {}ms", self.duration_ms)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_state_basic_operations() {
        let mut state = WorkflowState::new("test");
        
        state.set("foo", json!(42));
        state.set("bar", json!("hello"));
        
        assert_eq!(state.get("foo"), Some(&json!(42)));
        assert_eq!(state.get("bar"), Some(&json!("hello")));
        assert!(state.contains("foo"));
        assert!(!state.contains("missing"));
        
        state.remove("foo");
        assert!(!state.contains("foo"));
    }
    
    #[test]
    fn test_state_typed_operations() {
        let mut state = WorkflowState::new("test");
        
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestData {
            count: i32,
            name: String,
        }
        
        let data = TestData { count: 42, name: "test".into() };
        state.set_typed("data", &data);
        
        let retrieved: TestData = state.get_as("data").unwrap();
        assert_eq!(data, retrieved);
    }
    
    #[test]
    fn test_state_node_tracking() {
        let mut state = WorkflowState::new("test");
        
        state.start_node("analyze");
        assert_eq!(state.current_node, Some("analyze".to_string()));
        assert_eq!(state.history.len(), 1);
        
        state.complete_node(true, "Analysis complete");
        assert_eq!(state.current_node, None);
        assert!(state.history[0].success);
        assert!(state.history[0].completed_at.is_some());
    }
    
    #[test]
    fn test_state_summary() {
        let mut state = WorkflowState::new("test_workflow");
        state.start_node("node1");
        state.complete_node(true, "Done");
        state.start_node("node2");
        state.complete_node(false, "Failed");
        
        let summary = state.summary();
        assert_eq!(summary.total_steps, 2);
        assert_eq!(summary.successful_steps, 1);
        assert!(!summary.is_completed);
    }
    
    #[test]
    fn test_state_merge() {
        let mut state1 = WorkflowState::new("test1");
        let mut state2 = WorkflowState::new("test2");
        
        state1.set("a", json!(1));
        state2.set("b", json!(2));
        state2.set("c", json!(3));
        
        state1.merge(&state2, Some("sub"));
        
        assert_eq!(state1.get("a"), Some(&json!(1)));
        assert_eq!(state1.get("sub.b"), Some(&json!(2)));
        assert_eq!(state1.get("sub.c"), Some(&json!(3)));
    }
}
