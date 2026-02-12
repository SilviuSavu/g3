//! Human-in-the-loop approval gates for workflows.
//!
//! Provides mechanisms for workflows to pause at checkpoints and wait for
//! human approval before continuing. Supports timeouts, feedback, and rerouting.

use super::{CheckpointManager, Workflow, WorkflowState};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, info, warn};

/// Default timeout for approval (30 minutes).
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 30 * 60;

/// Maximum timeout (24 hours).
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 24 * 60 * 60;

/// Configuration for an approval gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGateConfig {
    /// Timeout in seconds before default action is taken.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    
    /// Default action when timeout occurs.
    #[serde(default)]
    pub timeout_action: TimeoutAction,
    
    /// Whether to allow rerouting on rejection.
    #[serde(default = "default_true")]
    pub allow_reroute: bool,
    
    /// Message to display when requesting approval.
    #[serde(default)]
    pub prompt_message: Option<String>,
    
    /// Custom instructions for the approver.
    #[serde(default)]
    pub instructions: Option<String>,
}

fn default_timeout() -> u64 {
    DEFAULT_APPROVAL_TIMEOUT_SECS
}

fn default_true() -> bool {
    true
}

impl Default for ApprovalGateConfig {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
            timeout_action: TimeoutAction::Approve,
            allow_reroute: true,
            prompt_message: None,
            instructions: None,
        }
    }
}

/// Action to take when approval times out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeoutAction {
    /// Automatically approve after timeout.
    Approve,
    /// Automatically reject after timeout.
    Reject,
    /// Fail the workflow on timeout.
    Fail,
}

impl Default for TimeoutAction {
    fn default() -> Self {
        TimeoutAction::Approve
    }
}

/// An approval request pending human review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique request ID.
    pub id: String,
    /// Workflow ID.
    pub workflow_id: String,
    /// Workflow name.
    pub workflow_name: String,
    /// Node where approval is requested.
    pub node_id: String,
    /// Current workflow state summary.
    pub state_summary: ApprovalStateSummary,
    /// Configuration for this approval gate.
    pub config: ApprovalGateConfig,
    /// When the request was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional message explaining what needs approval.
    pub message: Option<String>,
}

/// Summary of workflow state for approval display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalStateSummary {
    /// Number of nodes completed.
    pub nodes_completed: usize,
    /// Current iteration count.
    pub iteration: usize,
    /// Brief status of each completed node.
    pub node_history: Vec<NodeHistoryEntry>,
    /// Key outputs that might inform the decision.
    pub key_outputs: std::collections::HashMap<String, String>,
}

/// History entry for a completed node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHistoryEntry {
    pub node_id: String,
    pub success: bool,
    pub summary: String,
}

/// Response to an approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    /// ID of the request being responded to.
    pub request_id: String,
    /// The decision made.
    pub decision: ApprovalDecision,
    /// Timestamp of the response.
    pub responded_at: chrono::DateTime<chrono::Utc>,
}

/// Decision on an approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Approved - continue with workflow.
    Approved,
    /// Rejected with feedback.
    Rejected {
        reason: String,
    },
    /// Reroute to a different node.
    Reroute {
        target: String,
        reason: String,
    },
}

impl ApprovalDecision {
    /// Check if this is an approval.
    pub fn is_approved(&self) -> bool {
        matches!(self, ApprovalDecision::Approved)
    }
    
    /// Check if this is a rejection.
    pub fn is_rejected(&self) -> bool {
        matches!(self, ApprovalDecision::Rejected { .. })
    }
    
    /// Check if this is a reroute.
    pub fn is_reroute(&self) -> bool {
        matches!(self, ApprovalDecision::Reroute { .. })
    }
}

/// Internal representation of a pending approval.
pub struct PendingApproval {
    request: ApprovalRequest,
    responder: oneshot::Sender<ApprovalResponse>,
}

/// Manager for approval gates.
/// 
/// Coordinates between workflow executors (which request approvals) and
/// the CLI/UI (which provides approvals). Uses channels for async communication.
pub struct ApprovalGate {
    /// Configuration for this gate.
    config: ApprovalGateConfig,
    /// Channel for receiving approval requests from workflows.
    request_tx: mpsc::Sender<PendingApproval>,
    /// Channel for the approval handler to receive requests.
    request_rx: Arc<Mutex<mpsc::Receiver<PendingApproval>>>,
    /// Checkpoint manager for state persistence.
    checkpoint_manager: Option<Arc<CheckpointManager>>,
}

impl ApprovalGate {
    /// Create a new approval gate with default configuration.
    pub fn new() -> Self {
        Self::with_config(ApprovalGateConfig::default())
    }
    
    /// Create an approval gate with custom configuration.
    pub fn with_config(config: ApprovalGateConfig) -> Self {
        let (request_tx, request_rx) = mpsc::channel(16);
        
        Self {
            config,
            request_tx,
            request_rx: Arc::new(Mutex::new(request_rx)),
            checkpoint_manager: None,
        }
    }
    
    /// Set the checkpoint manager for persisting approval state.
    pub fn with_checkpoints(mut self, manager: Arc<CheckpointManager>) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }
    
    /// Get the configuration.
    pub fn config(&self) -> &ApprovalGateConfig {
        &self.config
    }
    
    /// Request approval for a workflow at a checkpoint.
    /// 
    /// This method is called by the workflow executor when it reaches a checkpoint.
    /// It will block until:
    /// - A response is received from the approval handler, OR
    /// - The timeout expires
    pub async fn request_approval(
        &self,
        workflow: &Workflow,
        state: &WorkflowState,
        node_id: &str,
        message: Option<String>,
    ) -> Result<ApprovalDecision> {
        let request_id = uuid::Uuid::new_v4().to_string();
        
        // Build the approval request
        let request = ApprovalRequest {
            id: request_id.clone(),
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            node_id: node_id.to_string(),
            state_summary: Self::build_state_summary(state),
            config: self.config.clone(),
            created_at: chrono::Utc::now(),
            message: message.or_else(|| self.config.prompt_message.clone()),
        };
        
        info!(
            "Requesting approval for workflow '{}' at node '{}'",
            workflow.name, node_id
        );
        
        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();
        
        // Send the request
        let pending = PendingApproval {
            request,
            responder: response_tx,
        };
        
        self.request_tx.send(pending).await.map_err(|_| {
            anyhow!("Approval gate channel closed")
        })?;
        
        // Wait for response with timeout
        let timeout_duration = Duration::from_secs(self.config.timeout_secs);
        
        match tokio::time::timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => {
                info!(
                    "Received approval response for request {}: {:?}",
                    request_id, response.decision
                );
                Ok(response.decision)
            }
            Ok(Err(_)) => {
                Err(anyhow!("Approval response channel closed unexpectedly"))
            }
            Err(_) => {
                warn!(
                    "Approval request {} timed out after {} seconds, taking default action: {:?}",
                    request_id, self.config.timeout_secs, self.config.timeout_action
                );
                Ok(self.timeout_decision())
            }
        }
    }
    
    /// Get the receiver for approval requests.
    /// 
    /// The holder of this receiver is responsible for handling approval requests
    /// and sending responses. This is typically the CLI or UI layer.
    pub fn request_receiver(&self) -> Arc<Mutex<mpsc::Receiver<PendingApproval>>> {
        self.request_rx.clone()
    }
    
    /// Respond to an approval request.
    /// 
    /// This is a convenience method for the approval handler to send a response.
    pub async fn respond(&self, request: ApprovalRequest, decision: ApprovalDecision) -> Result<()> {
        // Note: This is a simplified version. In practice, you'd need to track
        // the responder channels by request ID. For now, this is handled internally.
        debug!(
            "Recording approval decision for request {}: {:?}",
            request.id, decision
        );
        Ok(())
    }
    
    /// Build a state summary for an approval request.
    fn build_state_summary(state: &WorkflowState) -> ApprovalStateSummary {
        let node_history: Vec<NodeHistoryEntry> = state
            .history
            .iter().map(|entry| NodeHistoryEntry {
                node_id: entry.node_id.clone(),
                success: entry.success,
                summary: entry.summary.chars().take(200).collect(),
            })
            .collect();
        
        // Extract key outputs (simplified - in practice, you'd have specific keys)
        let key_outputs = std::collections::HashMap::new();
        
        ApprovalStateSummary {
            nodes_completed: state.history.len(),
            iteration: state.iteration,
            node_history,
            key_outputs,
        }
    }
    
    /// Get the decision to take on timeout.
    fn timeout_decision(&self) -> ApprovalDecision {
        match self.config.timeout_action {
            TimeoutAction::Approve => ApprovalDecision::Approved,
            TimeoutAction::Reject => ApprovalDecision::Rejected {
                reason: "Approval timed out".to_string(),
            },
            TimeoutAction::Fail => ApprovalDecision::Rejected {
                reason: "Approval timed out - workflow failed".to_string(),
            },
        }
    }
}

impl Default for ApprovalGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for the CLI to handle approval requests.
/// 
/// This struct wraps the approval gate receiver and provides a convenient
/// interface for the CLI to wait for and respond to approval requests.
pub struct ApprovalHandler {
    receiver: Arc<Mutex<mpsc::Receiver<PendingApproval>>>,
}

impl ApprovalHandler {
    /// Create an approval handler from an approval gate.
    pub fn from_gate(gate: &ApprovalGate) -> Self {
        Self {
            receiver: gate.request_receiver(),
        }
    }
    
    /// Wait for the next approval request.
    /// 
    /// Returns None if the channel is closed.
    pub async fn wait_for_request(&self) -> Option<ApprovalRequest> {
        let mut rx = self.receiver.lock().await;
        let pending = rx.recv().await?;
        // Store the responder for later use
        // For now, just return the request
        Some(pending.request)
    }
    
    /// Wait for an approval request with a timeout.
    pub async fn wait_for_request_timeout(
        &self,
        timeout: Duration,
    ) -> Option<ApprovalRequest> {
        let fut = self.wait_for_request();
        tokio::time::timeout(timeout, fut).await.ok().flatten()
    }
    
    /// Respond to an approval request.
    /// 
    /// This is a placeholder - in practice, you'd need to track responders.
    pub async fn respond(&self, _request_id: &str, _decision: ApprovalDecision) -> Result<()> {
        // In a full implementation, this would look up the responder by request_id
        // and send the response through the oneshot channel.
        Ok(())
    }
}

/// CLI-friendly approval interface.
/// 
/// Provides methods for the CLI to display approval requests and collect responses.
pub struct CliApprovalInterface {
    gate: ApprovalGate,
}

impl CliApprovalInterface {
    /// Create a new CLI approval interface.
    pub fn new(config: ApprovalGateConfig) -> Self {
        Self {
            gate: ApprovalGate::with_config(config),
        }
    }
    
    /// Get the underlying approval gate.
    pub fn gate(&self) -> &ApprovalGate {
        &self.gate
    }
    
    /// Format an approval request for display.
    pub fn format_request(request: &ApprovalRequest) -> String {
        let mut output = String::new();
        
        output.push_str(&format!(
            "╭{}╮\n",
            "─".repeat(58)
        ));
        output.push_str(&format!(
            "│ {:56} │\n",
            format!("Approval Request: {}", request.workflow_name)
        ));
        output.push_str(&format!(
            "├{}┤\n",
            "─".repeat(58)
        ));
        output.push_str(&format!(
            "│ {:56} │\n",
            format!("Node: {}", request.node_id)
        ));
        output.push_str(&format!(
            "│ {:56} │\n",
            format!("Progress: {} nodes completed, iteration {}",
                request.state_summary.nodes_completed,
                request.state_summary.iteration
            )
        ));
        
        if let Some(ref msg) = request.message {
            output.push_str(&format!(
                "├{}┤\n",
                "─".repeat(58)
            ));
            // Word wrap the message
            // Simple word wrap at 54 chars
            let mut line = String::new();
            for word in msg.split_whitespace() {
                if line.len() + word.len() + 1 > 54 {
                    output.push_str(&format!("│  {:<55}│\n", line));
                    line.clear();
                }
                if !line.is_empty() { line.push(' '); }
                line.push_str(word);
            }
            if !line.is_empty() {
                output.push_str(&format!("│  {:<55}│\n", line));
            }
        }
        
        output.push_str(&format!(
            "├{}┤\n",
            "─".repeat(58)
        ));
        output.push_str(&format!(
            "│ {:56} │\n",
            "Options: [A]pprove | [R]eject | [S]kip to node"
        ));
        output.push_str(&format!(
            "│ {:56} │\n",
            format!("Timeout: {} secs (default: {:?})",
                request.config.timeout_secs,
                request.config.timeout_action
            )
        ));
        output.push_str(&format!(
            "╰{}╯\n",
            "─".repeat(58)
        ));
        
        output
    }
    
    /// Parse a user's input into an approval decision.
    pub fn parse_decision(input: &str) -> Option<ApprovalDecision> {
        let trimmed = input.trim().to_lowercase();
        
        if trimmed == "a" || trimmed == "approve" || trimmed == "y" || trimmed == "yes" {
            Some(ApprovalDecision::Approved)
        } else if trimmed.starts_with("r ") || trimmed.starts_with("reject ") {
            let reason = trimmed
                .strip_prefix("r ")
                .or_else(|| trimmed.strip_prefix("reject "))
                .unwrap_or("Rejected by user");
            Some(ApprovalDecision::Rejected {
                reason: reason.to_string(),
            })
        } else if trimmed.starts_with("s ") || trimmed.starts_with("skip ") {
            let target = trimmed
                .strip_prefix("s ")
                .or_else(|| trimmed.strip_prefix("skip "))
                .unwrap_or("DONE");
            Some(ApprovalDecision::Reroute {
                target: target.to_string(),
                reason: "Skipped by user".to_string(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{AgentRole, WorkflowBuilder};
    
    fn create_test_workflow() -> Workflow {
        WorkflowBuilder::new("test")
            .node("start", AgentRole::Researcher)
            .node("middle", AgentRole::Coder)
            .node("end", AgentRole::Tester)
            .checkpoint("middle")
            .edge("start", "middle")
            .edge("middle", "end")
            .edge("end", "DONE")
            .entrypoint("start")
            .build()
            .unwrap()
    }
    
    fn create_test_state() -> WorkflowState {
        WorkflowState::with_request("test_workflow", "Test request")
    }
    
    #[test]
    fn test_approval_gate_config_default() {
        let config = ApprovalGateConfig::default();
        assert_eq!(config.timeout_secs, DEFAULT_APPROVAL_TIMEOUT_SECS);
        assert_eq!(config.timeout_action, TimeoutAction::Approve);
        assert!(config.allow_reroute);
    }
    
    #[test]
    fn test_approval_decision_helpers() {
        let approved = ApprovalDecision::Approved;
        assert!(approved.is_approved());
        assert!(!approved.is_rejected());
        assert!(!approved.is_reroute());
        
        let rejected = ApprovalDecision::Rejected { reason: "test".to_string() };
        assert!(!rejected.is_approved());
        assert!(rejected.is_rejected());
        assert!(!rejected.is_reroute());
        
        let reroute = ApprovalDecision::Reroute { 
            target: "done".to_string(),
            reason: "skip".to_string(),
        };
        assert!(!reroute.is_approved());
        assert!(!reroute.is_rejected());
        assert!(reroute.is_reroute());
    }
    
    #[test]
    fn test_timeout_decision() {
        let mut config = ApprovalGateConfig::default();
        
        config.timeout_action = TimeoutAction::Approve;
        let gate = ApprovalGate::with_config(config.clone());
        assert!(gate.timeout_decision().is_approved());
        
        config.timeout_action = TimeoutAction::Reject;
        let gate = ApprovalGate::with_config(config.clone());
        assert!(gate.timeout_decision().is_rejected());
        
        config.timeout_action = TimeoutAction::Fail;
        let gate = ApprovalGate::with_config(config);
        assert!(gate.timeout_decision().is_rejected());
    }
    
    #[test]
    fn test_parse_decision() {
        assert!(CliApprovalInterface::parse_decision("a").unwrap().is_approved());
        assert!(CliApprovalInterface::parse_decision("approve").unwrap().is_approved());
        assert!(CliApprovalInterface::parse_decision("y").unwrap().is_approved());
        assert!(CliApprovalInterface::parse_decision("yes").unwrap().is_approved());
        
        let rejected = CliApprovalInterface::parse_decision("r bad code").unwrap();
        assert!(rejected.is_rejected());
        
        let reroute = CliApprovalInterface::parse_decision("s end").unwrap();
        assert!(reroute.is_reroute());
        
        assert!(CliApprovalInterface::parse_decision("invalid").is_none());
    }
    
    #[test]
    fn test_format_request() {
        let workflow = create_test_workflow();
        let state = create_test_state();
        
        let request = ApprovalRequest {
            id: "test-id".to_string(),
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            node_id: "middle".to_string(),
            state_summary: ApprovalStateSummary {
                nodes_completed: 1,
                iteration: 1,
                node_history: vec![],
                key_outputs: std::collections::HashMap::new(),
            },
            config: ApprovalGateConfig::default(),
            created_at: chrono::Utc::now(),
            message: Some("Please review the code changes".to_string()),
        };
        
        let formatted = CliApprovalInterface::format_request(&request);
        assert!(formatted.contains("test"));
        assert!(formatted.contains("middle"));
        assert!(formatted.contains("Approve"));
    }
    
    #[tokio::test]
    async fn test_approval_request_with_timeout() {
        let mut config = ApprovalGateConfig::default();
        config.timeout_secs = 1; // 1 second timeout for testing
        
        let gate = ApprovalGate::with_config(config);
        let workflow = create_test_workflow();
        let state = create_test_state();
        
        // Request approval without any handler - should timeout
        let result = gate
            .request_approval(&workflow, &state, "middle", Some("Test message".to_string()))
            .await;
        
        // Should succeed with timeout decision
        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(decision.is_approved()); // Default timeout action
    }
}
