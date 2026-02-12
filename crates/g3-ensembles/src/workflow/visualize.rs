//! Workflow visualization utilities.
//!
//! Generates visual representations of workflows for debugging and monitoring.
//! Supports Mermaid diagrams for markdown rendering.

use crate::workflow::{Workflow, WorkflowState, AgentRole};

/// Generate a Mermaid flowchart diagram from a workflow.
///
/// Example output:
/// ```mermaid
/// flowchart TD
///     start[Start] --> analyze[Analyze]
///     analyze -->|found bugs| fix[Fix Bugs]
///     analyze -->|clean| done[Done]
///     fix --> test[Test]
///     test -->|pass| done
///     test -->|fail| fix
/// ```
pub fn to_mermaid(workflow: &Workflow) -> String {
    let mut output = String::new();
    output.push_str("flowchart TD\n");
    
    // Add nodes
    for (node_id, node) in &workflow.nodes {
        let label = escape_mermaid_label(node_id);
        let role_icon = role_icon(&node.agent_role);
        output.push_str(&format!(
            "    {}[\"{} {}\"]\n",
            sanitize_id(node_id),
            role_icon,
            label
        ));
    }
    
    // Add DONE node
    output.push_str(&format!(
        "    {}((\"Done\"))\n",
        sanitize_id("DONE")
    ));
    
    // Add edges
    for edge in &workflow.edges {
        let from = sanitize_id(&edge.from);
        let to = sanitize_id(&edge.to);
        
        if let Some(ref condition) = edge.condition {
            let label = escape_mermaid_label(&condition.description());
            output.push_str(&format!(
                "    {} -->|{}| {}\n",
                from, label, to
            ));
        } else {
            output.push_str(&format!(
                "    {} --> {}\n",
                from, to
            ));
        }
    }
    
    // Mark entrypoint
    output.push_str(&format!(
        "    style {} fill:#e1f5fe,stroke:#01579b\n",
        sanitize_id(&workflow.entrypoint)
    ));
    
    output
}

/// Generate a Mermaid diagram showing workflow execution state.
///
/// Highlights completed nodes in green, failed in red, current in yellow.
pub fn to_mermaid_with_state(workflow: &Workflow, state: &WorkflowState) -> String {
    let mut output = to_mermaid(workflow);
    
    // Style completed nodes
    let completed: std::collections::HashSet<_> = state
        .history
        .iter()
        .filter(|s| s.success)
        .map(|s| s.node_id.as_str())
        .collect();
    
    let failed: std::collections::HashSet<_> = state
        .history
        .iter()
        .filter(|s| !s.success)
        .map(|s| s.node_id.as_str())
        .collect();
    
    for node_id in &completed {
        let sanitized = sanitize_id(node_id);
        output.push_str(&format!(
            "    style {} fill:#c8e6c9,stroke:#2e7d32\n",
            sanitized
        ));
    }
    
    for node_id in &failed {
        let sanitized = sanitize_id(node_id);
        output.push_str(&format!(
            "    style {} fill:#ffcdd2,stroke:#c62828\n",
            sanitized
        ));
    }
    
    // Style current node
    if let Some(ref current) = state.current_node {
        let sanitized = sanitize_id(current);
        output.push_str(&format!(
            "    style {} fill:#fff9c4,stroke:#f9a825\n",
            sanitized
        ));
    }
    
    output
}

/// Generate a text-based workflow visualization (for terminals).
pub fn to_ascii(workflow: &Workflow) -> String {
    let mut output = String::new();
    output.push_str(&format!("Workflow: {}\n", workflow.name));
    output.push_str(&"=".repeat(40 + workflow.name.len()));
    output.push_str("\n\n");
    
    // List nodes
    output.push_str("Nodes:\n");
    for (node_id, node) in &workflow.nodes {
        let role = node.agent_role.name();
        output.push_str(&format!(
            "  - {} ({})\n",
            node_id, role
        ));
    }
    
    if workflow.nodes.is_empty() {
        output.push_str("  (no nodes)\n");
    }
    
    output.push_str("\nEdges:\n");
    for edge in &workflow.edges {
        if let Some(ref condition) = edge.condition {
            output.push_str(&format!(
                "  {} --[{}]--> {}\n",
                edge.from,
                condition.description(),
                edge.to
            ));
        } else {
            output.push_str(&format!(
                "  {} --> {}\n",
                edge.from, edge.to
            ));
        }
    }
    
    if workflow.edges.is_empty() {
        output.push_str("  (no edges)\n");
    }
    
    // Show entrypoint
    output.push_str(&format!("\nEntrypoint: {}\n", workflow.entrypoint));
    
    output
}

/// Generate a summary of the workflow execution state.
pub fn state_summary(state: &WorkflowState) -> String {
    let mut output = String::new();
    
    output.push_str(&format!(
        "Workflow: {} ({})\n",
        state.workflow_name, state.workflow_id
    ));
    output.push_str(&format!(
        "Status: {}\n",
        if state.is_completed() { "completed" } else { "in progress" }
    ));
    
    if let Some(ref current) = state.current_node {
        output.push_str(&format!("Current: {}\n", current));
    }
    
    output.push_str(&format!("Iteration: {}\n", state.iteration));
    output.push_str(&format!("Steps: {}\n", state.history.len()));
    
    // Show history
    if !state.history.is_empty() {
        output.push_str("\nHistory:\n");
        for (i, step) in state.history.iter().enumerate() {
            let status = if step.success { "✓" } else { "✗" };
            output.push_str(&format!(
                "  {}. {} {} - {}\n",
                i + 1,
                status,
                step.node_id,
                step.summary.chars().take(50).collect::<String>()
            ));
        }
    }
    
    output
}

// Helper functions

fn sanitize_id(id: &str) -> String {
    // Mermaid IDs must be valid identifiers
    id.replace("-", "_")
        .replace(" ", "_")
        .replace(".", "_")
}

fn escape_mermaid_label(label: &str) -> String {
    // Escape special characters in labels
    label
        .replace("\"", "&#34;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
}

fn role_icon(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Researcher => "🔍",
        AgentRole::Planner => "📋",
        AgentRole::Coder => "💻",
        AgentRole::Tester => "🧪",
        AgentRole::Reviewer => "👀",
        AgentRole::Deployer => "🚀",
        AgentRole::Custom(_) => "⚙️",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{WorkflowBuilder, Condition};
    use serde_json::json;
    
    fn create_test_workflow() -> Workflow {
        WorkflowBuilder::new("test-workflow")
            .node("start", AgentRole::Custom("orchestrator".to_string()))
            .node("analyze", AgentRole::Researcher)
            .node("implement", AgentRole::Coder)
            .node("test", AgentRole::Tester)
            .node("deploy", AgentRole::Deployer)
            .edge("start", "analyze")
            .edge("analyze", "implement")
            .edge("implement", "test")
            .edge_if(
                "test",
                "deploy",
                Condition::equals("test_result", json!("pass")),
            )
            .edge_if(
                "test",
                "DONE",
                Condition::equals("test_result", json!("skip")),
            )
            .edge("deploy", "DONE")
            .entrypoint("start")
            .build()
            .unwrap()
    }
    
    #[test]
    fn test_to_mermaid_basic() {
        let workflow = create_test_workflow();
        let mermaid = to_mermaid(&workflow);
        
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("DONE"));
    }
    
    #[test]
    fn test_to_mermaid_with_edges() {
        let workflow = create_test_workflow();
        let mermaid = to_mermaid(&workflow);
        
        // Check basic edge
        assert!(mermaid.contains("start --> analyze"));
    }
    
    #[test]
    fn test_to_mermaid_entrypoint_styled() {
        let workflow = create_test_workflow();
        let mermaid = to_mermaid(&workflow);
        
        // Entrypoint should be styled
        assert!(mermaid.contains("style start fill:#e1f5fe"));
    }
    
    #[test]
    fn test_to_mermaid_with_state() {
        let workflow = create_test_workflow();
        let mut state = WorkflowState::with_request("test-workflow", "test request");
        
        // Simulate some execution
        state.start_node("start");
        state.complete_node(true, "Started");
        state.start_node("analyze");
        
        let mermaid = to_mermaid_with_state(&workflow, &state);
        
        // Completed node should be green
        assert!(mermaid.contains("style start fill:#c8e6c9"));
        // Current node should be yellow
        assert!(mermaid.contains("style analyze fill:#fff9c4"));
    }
    
    #[test]
    fn test_to_ascii() {
        let workflow = create_test_workflow();
        let ascii = to_ascii(&workflow);
        
        assert!(ascii.contains("Workflow: test-workflow"));
        assert!(ascii.contains("Nodes:"));
        assert!(ascii.contains("Edges:"));
        assert!(ascii.contains("Entrypoint: start"));
    }
    
    #[test]
    fn test_state_summary() {
        let mut state = WorkflowState::with_request("test-workflow", "test request");
        state.start_node("start");
        state.complete_node(true, "Started workflow");
        
        let summary = state_summary(&state);
        
        assert!(summary.contains("Workflow: test-workflow"));
        assert!(summary.contains("Steps: 1"));
        assert!(summary.contains("History:"));
    }
    
    #[test]
    fn test_large_workflow_performance() {
        // Create a workflow with 50+ nodes
        let mut builder = WorkflowBuilder::new("large-workflow");
        
        // Create 60 nodes
        for i in 0..60 {
            let role = match i % 6 {
                0 => AgentRole::Researcher,
                1 => AgentRole::Planner,
                2 => AgentRole::Coder,
                3 => AgentRole::Tester,
                4 => AgentRole::Reviewer,
                _ => AgentRole::Deployer,
            };
            builder = builder.node(format!("node_{}", i), role);
        }
        
        // Connect them in a chain
        builder = builder.edge("node_0", "node_1");
        for i in 1..59 {
            builder = builder.edge(
                format!("node_{}", i),
                format!("node_{}", i + 1)
            );
        }
        builder = builder.edge("node_59", "DONE");
        builder = builder.entrypoint("node_0");
        
        let workflow = builder.build().unwrap();
        
        // Measure time to generate visualization
        let start = std::time::Instant::now();
        let mermaid = to_mermaid(&workflow);
        let duration = start.elapsed();
        
        // Should be fast (< 10ms for 50+ nodes)
        assert!(duration.as_millis() < 10);
        
        // Should contain all nodes
        assert!(mermaid.contains("node_0"));
        assert!(mermaid.contains("node_59"));
    }
}
