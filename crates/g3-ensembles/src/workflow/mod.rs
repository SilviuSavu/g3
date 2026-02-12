//! Workflow orchestration module - LangGraph-style DAG-based multi-agent workflows.
//!
//! This module provides a graph-based workflow engine for orchestrating multiple
//! specialized agents. It supports:
//! - Conditional edges between nodes
//! - State management across execution
//! - Human-in-the-loop checkpoints
//! - Parallel execution of independent nodes

pub mod builder;
pub mod checkpoint;
pub mod condition;
pub mod executor;
pub mod node;
pub mod state;

pub use builder::WorkflowBuilder;
pub use condition::{Condition, Condition as EdgeCondition};
pub use checkpoint::{Checkpoint, CheckpointManager, CheckpointMeta, CheckpointError};
pub use executor::{WorkflowExecutor, WorkflowOutcome};
pub use node::{Node, NodeConfig};
pub use state::WorkflowState;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// A workflow edge connecting two nodes with an optional condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source node ID
    pub from: String,
    /// Target node ID (or "DONE" for terminal)
    pub to: String,
    /// Optional condition for this edge to be taken
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

impl Edge {
    /// Create an unconditional edge
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            condition: None,
        }
    }
    
    /// Create a conditional edge
    pub fn conditional(
        from: impl Into<String>,
        to: impl Into<String>,
        condition: Condition,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            condition: Some(condition),
        }
    }
}

/// Agent role definition for workflow nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentRole {
    /// Researches and analyzes codebase
    Researcher,
    /// Plans implementation approach
    Planner,
    /// Implements code changes
    Coder,
    /// Runs tests and validates
    Tester,
    /// Reviews code for quality
    Reviewer,
    /// Deploys and manages infrastructure
    Deployer,
    /// Custom role with name
    Custom(String),
}

impl AgentRole {
    /// Get the role name as a string
    pub fn name(&self) -> &str {
        match self {
            AgentRole::Researcher => "researcher",
            AgentRole::Planner => "planner",
            AgentRole::Coder => "coder",
            AgentRole::Tester => "tester",
            AgentRole::Reviewer => "reviewer",
            AgentRole::Deployer => "deployer",
            AgentRole::Custom(name) => name,
        }
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A workflow definition - a directed graph of nodes connected by edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow identifier
    pub id: String,
    /// Human-readable workflow name
    pub name: String,
    /// Description of what this workflow does
    #[serde(default)]
    pub description: String,
    /// All nodes in the workflow
    pub nodes: HashMap<String, Node>,
    /// All edges connecting nodes
    pub edges: Vec<Edge>,
    /// Entry point node ID
    pub entrypoint: String,
    /// Nodes that require human approval before continuing
    #[serde(default)]
    pub checkpoints: HashSet<String>,
    /// Maximum iterations to prevent infinite loops
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    100
}

impl Workflow {
    /// Create a new workflow with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            entrypoint: String::new(),
            checkpoints: HashSet::new(),
            max_iterations: 100,
        }
    }
    
    /// Validate the workflow structure
    pub fn validate(&self) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();
        
        // Check entrypoint exists
        if !self.nodes.contains_key(&self.entrypoint) {
            errors.push(ValidationError::MissingEntrypoint {
                entrypoint: self.entrypoint.clone(),
            });
        }
        
        // Check all edge references are valid
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) && edge.from != "DONE" {
                errors.push(ValidationError::InvalidEdgeSource {
                    edge: format!("{} -> {}", edge.from, edge.to),
                    node: edge.from.clone(),
                });
            }
            if !self.nodes.contains_key(&edge.to) && edge.to != "DONE" {
                errors.push(ValidationError::InvalidEdgeTarget {
                    edge: format!("{} -> {}", edge.from, edge.to),
                    node: edge.to.clone(),
                });
            }
        }
        
        // Check for cycles (simple DFS)
        if let Some(cycle) = self.detect_cycle() {
            errors.push(ValidationError::CycleDetected { cycle });
        }
        
        // Check for unreachable nodes
        let reachable = self.find_reachable_nodes();
        for node_id in self.nodes.keys() {
            if !reachable.contains(node_id) {
                errors.push(ValidationError::UnreachableNode {
                    node: node_id.clone(),
                });
            }
        }
        
        Ok(errors)
    }
    
    /// Detect if there's a cycle in the workflow
    fn detect_cycle(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();
        
        for node_id in self.nodes.keys() {
            if self.detect_cycle_dfs(node_id, &mut visited, &mut rec_stack, &mut path) {
                return Some(path);
            }
        }
        
        None
    }
    
    fn detect_cycle_dfs(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if rec_stack.contains(node_id) {
            // Found cycle - extract the cycle portion
            if let Some(start) = path.iter().position(|n| n == node_id) {
                *path = path[start..].to_vec();
            }
            path.push(node_id.to_string());
            return true;
        }
        
        if visited.contains(node_id) {
            return false;
        }
        
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());
        
        // Find all edges from this node
        for edge in self.edges.iter().filter(|e| e.from == node_id) {
            if edge.to != "DONE" && self.detect_cycle_dfs(&edge.to, visited, rec_stack, path) {
                return true;
            }
        }
        
        rec_stack.remove(node_id);
        path.pop();
        false
    }
    
    /// Find all nodes reachable from the entrypoint
    fn find_reachable_nodes(&self) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut queue = vec![self.entrypoint.clone()];
        
        while let Some(node_id) = queue.pop() {
            if reachable.insert(node_id.clone()) {
                for edge in self.edges.iter().filter(|e| e.from == node_id) {
                    if edge.to != "DONE" && !reachable.contains(&edge.to) {
                        queue.push(edge.to.clone());
                    }
                }
            }
        }
        
        reachable
    }
    
    /// Get outgoing edges from a node
    pub fn get_outgoing_edges(&self, node_id: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.from == node_id).collect()
    }
    
    /// Check if a node is a checkpoint
    pub fn is_checkpoint(&self, node_id: &str) -> bool {
        self.checkpoints.contains(node_id)
    }
}

/// Validation errors for workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    /// Entrypoint node doesn't exist
    MissingEntrypoint {
        entrypoint: String,
    },
    /// Edge references non-existent source node
    InvalidEdgeSource {
        edge: String,
        node: String,
    },
    /// Edge references non-existent target node
    InvalidEdgeTarget {
        edge: String,
        node: String,
    },
    /// Cycle detected in workflow
    CycleDetected {
        cycle: Vec<String>,
    },
    /// Node is not reachable from entrypoint
    UnreachableNode {
        node: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingEntrypoint { entrypoint } => {
                write!(f, "Entrypoint '{}' does not exist", entrypoint)
            }
            ValidationError::InvalidEdgeSource { edge, node } => {
                write!(f, "Edge '{}' references non-existent source node '{}'", edge, node)
            }
            ValidationError::InvalidEdgeTarget { edge, node } => {
                write!(f, "Edge '{}' references non-existent target node '{}'", edge, node)
            }
            ValidationError::CycleDetected { cycle } => {
                write!(f, "Cycle detected: {}", cycle.join(" -> "))
            }
            ValidationError::UnreachableNode { node } => {
                write!(f, "Node '{}' is not reachable from entrypoint", node)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_workflow_validation_missing_entrypoint() {
        let mut workflow = Workflow::new("test");
        workflow.entrypoint = "nonexistent".to_string();
        workflow.nodes.insert("other".to_string(), Node::new("other", AgentRole::Coder));
        
        let errors = workflow.validate().unwrap();
        assert!(!errors.is_empty());
        assert!(matches!(errors[0], ValidationError::MissingEntrypoint { .. }));
    }
    
    #[test]
    fn test_workflow_cycle_detection() {
        let mut workflow = Workflow::new("cycle_test");
        workflow.entrypoint = "a".to_string();
        
        workflow.nodes.insert("a".to_string(), Node::new("a", AgentRole::Coder));
        workflow.nodes.insert("b".to_string(), Node::new("b", AgentRole::Coder));
        
        workflow.edges.push(Edge::new("a", "b"));
        workflow.edges.push(Edge::new("b", "a"));
        
        let errors = workflow.validate().unwrap();
        let has_cycle = errors.iter().any(|e| matches!(e, ValidationError::CycleDetected { .. }));
        assert!(has_cycle);
    }
    
    #[test]
    fn test_workflow_unreachable_node() {
        let mut workflow = Workflow::new("unreachable_test");
        workflow.entrypoint = "a".to_string();
        
        workflow.nodes.insert("a".to_string(), Node::new("a", AgentRole::Coder));
        workflow.nodes.insert("b".to_string(), Node::new("b", AgentRole::Coder)); // No edge to b
        
        let errors = workflow.validate().unwrap();
        let has_unreachable = errors.iter().any(|e| matches!(e, ValidationError::UnreachableNode { .. }));
        assert!(has_unreachable);
    }
}
