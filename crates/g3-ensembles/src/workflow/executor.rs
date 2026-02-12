//! Workflow executor - runs workflows and manages execution.

use super::{node::NodeResult, Node, Workflow, WorkflowState};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Outcome of a workflow execution.
#[derive(Debug, Clone)]
pub enum WorkflowOutcome {
    /// Workflow completed successfully
    Completed(WorkflowState),
    /// Workflow was rejected at a checkpoint
    Rejected {
        node_id: String,
        reason: String,
        state: WorkflowState,
    },
    /// Workflow failed with an error
    Failed {
        error: String,
        state: WorkflowState,
    },
    /// Workflow hit max iterations
    MaxIterationsReached(WorkflowState),
}

impl WorkflowOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, WorkflowOutcome::Completed(_))
    }
    
    pub fn state(&self) -> &WorkflowState {
        match self {
            WorkflowOutcome::Completed(s) => s,
            WorkflowOutcome::Rejected { state, .. } => state,
            WorkflowOutcome::Failed { state, .. } => state,
            WorkflowOutcome::MaxIterationsReached(state) => state,
        }
    }
}

/// Trait for executing nodes (to be implemented by integration layer).
#[async_trait::async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Execute a node with the given input
    async fn execute(
        &self,
        node: &Node,
        input: HashMap<String, serde_json::Value>,
    ) -> Result<NodeResult>;
}

/// Workflow executor.
pub struct WorkflowExecutor<E: NodeExecutor> {
    workflow: Workflow,
    state: WorkflowState,
    node_executor: E,
}

impl<E: NodeExecutor + std::marker::Sync> WorkflowExecutor<E> {
    /// Create a new workflow executor
    pub fn new(workflow: Workflow, node_executor: E) -> Self {
        let state = WorkflowState::new(&workflow.name);
        Self {
            workflow,
            state,
            node_executor,
        }
    }
    
    /// Create executor with existing state (for resume)
    pub fn with_state(workflow: Workflow, state: WorkflowState, node_executor: E) -> Self {
        Self {
            workflow,
            state,
            node_executor,
        }
    }
    
    /// Run the workflow to completion
    pub async fn run(mut self, initial_request: String) -> Result<WorkflowOutcome> {
        // Initialize state with request
        self.state = WorkflowState::with_request(&self.workflow.name, &initial_request);
        
        let mut current_node = self.workflow.entrypoint.clone();
        info!("Starting workflow '{}' from node '{}'", self.workflow.name, current_node);
        
        while current_node != "DONE" {
            // Check max iterations
            if self.state.iteration >= self.workflow.max_iterations {
                warn!("Workflow hit max iterations ({})", self.workflow.max_iterations);
                return Ok(WorkflowOutcome::MaxIterationsReached(self.state));
            }
            
            // Get the node
            let node = self.workflow.nodes.get(&current_node).cloned()
                .ok_or_else(|| anyhow!("Node '{}' not found", current_node))?;
            
            // Check if checkpoint
            if self.workflow.is_checkpoint(&current_node) {
                match self.wait_for_approval(&current_node).await? {
                    ApprovalDecision::Approved => {},
                    ApprovalDecision::Rejected { reason } => {
                        return Ok(WorkflowOutcome::Rejected {
                            node_id: current_node,
                            reason,
                            state: self.state,
                        });
                    }
                    ApprovalDecision::Reroute { target } => {
                        current_node = target;
                        continue;
                    }
                }
            }
            
            // Execute the node
            self.state.start_node(&current_node);
            let start = Instant::now();
            
            let input = self.map_inputs(&node)?;
            let result = self.node_executor.execute(&node, input).await;
            
            let duration = start.elapsed();
            
            match result {
                Ok(node_result) if node_result.success => {
                    self.state.complete_node(true, &node_result.output);
                    self.map_outputs(&node, &node_result)?;
                    
                    // Find next node
                    current_node = self.find_next_node(&current_node)?;
                    self.state.increment_iteration();
                    
                    info!("Node '{}' completed in {:?}", current_node, duration);
                }
                Ok(node_result) => {
                    self.state.complete_node(false, node_result.error.as_deref().unwrap_or("Unknown error"));
                    
                    if node.config.critical {
                        return Ok(WorkflowOutcome::Failed {
                            error: node_result.error.unwrap_or_else(|| "Node failed".to_string()),
                            state: self.state,
                        });
                    } else {
                        // Non-critical, continue to next node
                        warn!("Non-critical node '{}' failed, continuing", current_node);
                        current_node = self.find_next_node(&current_node)?;
                        self.state.increment_iteration();
                    }
                }
                Err(e) => {
                    self.state.complete_node(false, &e.to_string());
                    
                    if node.config.critical {
                        return Ok(WorkflowOutcome::Failed {
                            error: e.to_string(),
                            state: self.state,
                        });
                    } else {
                        warn!("Non-critical node '{}' errored: {}, continuing", current_node, e);
                        current_node = self.find_next_node(&current_node)?;
                        self.state.increment_iteration();
                    }
                }
            }
        }
        
        self.state.complete();
        info!("Workflow '{}' completed successfully", self.workflow.name);
        Ok(WorkflowOutcome::Completed(self.state))
    }
    
    /// Map workflow state to node input
    fn map_inputs(&self, node: &Node) -> Result<HashMap<String, serde_json::Value>> {
        let mut input = HashMap::new();
        
        for (state_key, agent_key) in &node.config.input_mapping {
            if let Some(value) = self.state.get(state_key) {
                input.insert(agent_key.clone(), value.clone());
            }
        }
        
        // Always include the initial request
        input.insert("request".to_string(), serde_json::json!(&self.state.initial_request));
        
        Ok(input)
    }
    
    /// Map node output to workflow state
    fn map_outputs(&mut self, node: &Node, result: &NodeResult) -> Result<()> {
        // Store the output in state
        self.state.set(format!("{}.output", node.id), serde_json::json!(&result.output));
        
        // Store metadata
        for (key, value) in &result.metadata {
            self.state.set(format!("{}.{}", node.id, key), value.clone());
        }
        
        Ok(())
    }
    
    /// Find the next node based on edge conditions
    fn find_next_node(&self, current: &str) -> Result<String> {
        let edges = self.workflow.get_outgoing_edges(current);
        
        if edges.is_empty() {
            return Err(anyhow!("No outgoing edges from node '{}'", current));
        }
        
        // Find first edge whose condition evaluates to true
        for edge in edges {
            let should_take = match &edge.condition {
                Some(cond) => cond.evaluate(&self.state),
                None => true,
            };
            
            if should_take {
                debug!("Taking edge {} -> {}", edge.from, edge.to);
                return Ok(edge.to.clone());
            }
        }
        
        Err(anyhow!("No condition matched for edges from node '{}'", current))
    }
    
    /// Wait for human approval at a checkpoint (placeholder - to be implemented)
    async fn wait_for_approval(&self, node_id: &str) -> Result<ApprovalDecision> {
        info!("Waiting for approval at checkpoint '{}'", node_id);
        // TODO: Implement actual approval mechanism (I3)
        // For now, auto-approve
        Ok(ApprovalDecision::Approved)
    }
}

/// Approval decision from human-in-the-loop
#[derive(Debug, Clone)]
pub enum ApprovalDecision {
    Approved,
    Rejected { reason: String },
    Reroute { target: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{AgentRole, Condition, Node, WorkflowBuilder};
    use serde_json::json;
    
    struct MockExecutor;
    
    #[async_trait::async_trait]
    impl NodeExecutor for MockExecutor {
        async fn execute(
            &self,
            node: &Node,
            input: HashMap<String, serde_json::Value>,
        ) -> Result<NodeResult> {
            // Mock execution - just return success
            Ok(NodeResult::success(&node.id, format!("Mock output for {}", node.id)))
        }
    }
    
    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let workflow = WorkflowBuilder::new("test")
            .node("a", AgentRole::Researcher)
            .node("b", AgentRole::Coder)
            .edge("a", "b")
            .edge("b", "DONE")
            .entrypoint("a")
            .build()
            .unwrap();
        
        let executor = WorkflowExecutor::new(workflow, MockExecutor);
        let result = executor.run("Test request".to_string()).await.unwrap();
        
        assert!(result.is_success());
        assert_eq!(result.state().history.len(), 2);
    }
    
    #[tokio::test]
    async fn test_conditional_routing() {
        let workflow = WorkflowBuilder::new("test")
            .node("check", AgentRole::Tester)
            .node("pass", AgentRole::Coder)
            .node("fail", AgentRole::Researcher)
            .edge_if("check", "pass", Condition::is_true("success"))
            .edge_if("check", "fail", Condition::is_false("success"))
            .edge("pass", "DONE")
            .edge("fail", "DONE")
            .entrypoint("check")
            .build()
            .unwrap();
        
        let executor = WorkflowExecutor::new(workflow, MockExecutor);
        let result = executor.run("Test".to_string()).await.unwrap();
        
        assert!(result.is_success());
    }
}
