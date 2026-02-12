//! Workflow builder - fluent API for constructing workflows.

use super::{AgentRole, Condition, Edge, Node, Workflow};
use anyhow::{anyhow, Result};
use std::collections::HashSet;

/// Builder for creating workflows with a fluent API.
pub struct WorkflowBuilder {
    name: String,
    description: String,
    nodes: Vec<Node>,
    edges: Vec<EdgeBuilder>,
    entrypoint: Option<String>,
    checkpoints: HashSet<String>,
    max_iterations: usize,
}

/// Builder for an edge with optional condition.
struct EdgeBuilder {
    from: String,
    to: String,
    condition: Option<Condition>,
}

impl WorkflowBuilder {
    /// Create a new workflow builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            entrypoint: None,
            checkpoints: HashSet::new(),
            max_iterations: 100,
        }
    }
    
    /// Set workflow description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
    
    /// Add a node with an agent role
    pub fn node(mut self, id: impl Into<String>, role: AgentRole) -> Self {
        self.nodes.push(Node::new(id, role));
        self
    }
    
    /// Add a node with custom configuration
    pub fn node_with_config(mut self, node: Node) -> Self {
        self.nodes.push(node);
        self
    }
    
    /// Add an unconditional edge
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push(EdgeBuilder {
            from: from.into(),
            to: to.into(),
            condition: None,
        });
        self
    }
    
    /// Add a conditional edge
    pub fn edge_if(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        condition: Condition,
    ) -> Self {
        self.edges.push(EdgeBuilder {
            from: from.into(),
            to: to.into(),
            condition: Some(condition),
        });
        self
    }
    
    /// Set the entrypoint node
    pub fn entrypoint(mut self, node_id: impl Into<String>) -> Self {
        self.entrypoint = Some(node_id.into());
        self
    }
    
    /// Mark a node as a checkpoint (requires human approval)
    pub fn checkpoint(mut self, node_id: impl Into<String>) -> Self {
        self.checkpoints.insert(node_id.into());
        self
    }
    
    /// Set maximum iterations
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
    
    /// Build and validate the workflow
    pub fn build(self) -> Result<Workflow> {
        // Create the workflow
        let mut workflow = Workflow::new(&self.name);
        workflow.description = self.description;
        workflow.max_iterations = self.max_iterations;
        workflow.checkpoints = self.checkpoints;
        
        // Add nodes
        for node in self.nodes {
            workflow.nodes.insert(node.id.clone(), node);
        }
        
        // Add edges
        for edge_builder in self.edges {
            workflow.edges.push(Edge {
                from: edge_builder.from,
                to: edge_builder.to,
                condition: edge_builder.condition,
            });
        }
        
        // Set entrypoint
        workflow.entrypoint = self.entrypoint
            .ok_or_else(|| anyhow!("Workflow must have an entrypoint"))?
            .into();
        
        // Validate
        let errors = workflow.validate()?;
        if !errors.is_empty() {
            let error_messages: Vec<_> = errors.iter().map(|e| e.to_string()).collect();
            return Err(anyhow!("Workflow validation failed:\n{}", error_messages.join("\n  ")));
        }
        
        Ok(workflow)
    }
}

/// Convenience methods for common workflow patterns
impl WorkflowBuilder {
    /// Create a simple linear workflow (analyze -> implement -> test)
    pub fn linear_linear(name: impl Into<String>, _task: impl Into<String>) -> Result<Workflow> {
        Self::new(name)
            .node("analyze", AgentRole::Researcher)
            .node("implement", AgentRole::Coder)
            .node("test", AgentRole::Tester)
            .edge("analyze", "implement")
            .edge("implement", "test")
            .edge("test", "DONE")
            .entrypoint("analyze")
            .build()
    }
    
    /// Create a workflow with review loop
    pub fn with_review(name: impl Into<String>) -> Result<Workflow> {
        Self::new(name)
            .node("analyze", AgentRole::Researcher)
            .node("implement", AgentRole::Coder)
            .node("test", AgentRole::Tester)
            .node("review", AgentRole::Reviewer)
            .edge("analyze", "implement")
            .edge("implement", "test")
            .edge("test", "review")
            .edge_if("review", "DONE", Condition::is_true("approved"))
            .edge_if("review", "implement", Condition::is_false("approved"))
            .entrypoint("analyze")
            .checkpoint("review")
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_simple_workflow() {
        let workflow = WorkflowBuilder::new("test")
            .node("start", AgentRole::Researcher)
            .node("end", AgentRole::Coder)
            .edge("start", "end")
            .edge("end", "DONE")
            .entrypoint("start")
            .build()
            .unwrap();
        
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 2);
        assert_eq!(workflow.entrypoint, "start");
    }
    
    #[test]
    fn test_conditional_workflow() {
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
        
        assert_eq!(workflow.nodes.len(), 3);
        assert_eq!(workflow.edges.len(), 4);
        assert!(workflow.edges[0].condition.is_some());
    }
    
    #[test]
    fn test_missing_entrypoint() {
        let result = WorkflowBuilder::new("test")
            .node("a", AgentRole::Coder)
            .build();
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_invalid_edge() {
        let result = WorkflowBuilder::new("test")
            .node("a", AgentRole::Coder)
            .edge("a", "nonexistent")
            .entrypoint("a")
            .build();
        
        assert!(result.is_err());
    }
}
