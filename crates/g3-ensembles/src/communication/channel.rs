//! Agent-to-agent communication channels.
//!
//! Provides typed message passing with:
//! - Async send/receive
//! - Message acknowledgments
//! - Retry policies for delivery failures
//! - High-throughput support (1000+ msg/sec)

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, warn};

/// Default channel capacity.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Maximum retry attempts for message delivery.
pub const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Default message timeout.
pub const DEFAULT_MESSAGE_TIMEOUT_MS: u64 = 5000;

/// Configuration for agent channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Maximum number of messages in the channel buffer.
    #[serde(default = "default_capacity")]
    pub capacity: usize,
    
    /// Timeout for send operations (milliseconds).
    #[serde(default = "default_send_timeout")]
    pub send_timeout_ms: u64,
    
    /// Timeout for receive operations (milliseconds).
    #[serde(default = "default_recv_timeout")]
    pub recv_timeout_ms: u64,
    
    /// Whether to require acknowledgment for messages.
    #[serde(default)]
    pub require_ack: bool,
    
    /// Retry policy for failed deliveries.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
}

fn default_capacity() -> usize { DEFAULT_CHANNEL_CAPACITY }
fn default_send_timeout() -> u64 { DEFAULT_MESSAGE_TIMEOUT_MS }
fn default_recv_timeout() -> u64 { DEFAULT_MESSAGE_TIMEOUT_MS }

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CHANNEL_CAPACITY,
            send_timeout_ms: DEFAULT_MESSAGE_TIMEOUT_MS,
            recv_timeout_ms: DEFAULT_MESSAGE_TIMEOUT_MS,
            require_ack: false,
            retry_policy: RetryPolicy::default(),
        }
    }
}

/// Retry policy for message delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    #[serde(default = "default_max_retries")]
    pub max_attempts: u32,
    
    /// Initial delay between retries (milliseconds).
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    
    /// Multiplier for exponential backoff.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    
    /// Maximum delay between retries (milliseconds).
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
}

fn default_max_retries() -> u32 { MAX_RETRY_ATTEMPTS }
fn default_initial_delay() -> u64 { 100 }
fn default_backoff_multiplier() -> f64 { 2.0 }
fn default_max_delay() -> u64 { 5000 }

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_RETRY_ATTEMPTS,
            initial_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_delay_ms: 5000,
        }
    }
}

impl RetryPolicy {
    /// Calculate the delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = self.initial_delay_ms as f64
            * self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = delay_ms.min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(delay_ms)
    }
}

/// A typed message sent between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID.
    pub id: String,
    /// Sender agent ID.
    pub from: String,
    /// Recipient agent ID.
    pub to: String,
    /// Message type/subject.
    pub message_type: String,
    /// Message payload (JSON).
    pub payload: serde_json::Value,
    /// When the message was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional correlation ID for request/response patterns.
    pub correlation_id: Option<String>,
    /// Priority (higher = more important).
    #[serde(default)]
    pub priority: u8,
}

impl Message {
    /// Create a new message.
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        message_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from: from.into(),
            to: to.into(),
            message_type: message_type.into(),
            payload,
            created_at: chrono::Utc::now(),
            correlation_id: None,
            priority: 0,
        }
    }
    
    /// Create a reply to this message.
    pub fn reply(&self, payload: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from: self.to.clone(),
            to: self.from.clone(),
            message_type: format!("reply:{}", self.message_type),
            payload,
            created_at: chrono::Utc::now(),
            correlation_id: Some(self.id.clone()),
            priority: self.priority,
        }
    }
    
    /// Deserialize the payload as a specific type.
    pub fn payload_as<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        T::deserialize(self.payload.clone())
            .map_err(|e| anyhow!("Failed to deserialize message payload: {}", e))
    }
}

/// Delivery status for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    /// Message was delivered successfully.
    Delivered,
    /// Message is pending delivery.
    Pending,
    /// Delivery failed after all retries.
    Failed,
    /// Message was rejected by the recipient.
    Rejected,
}

/// Internal channel state for an agent.
struct AgentChannelState {
    /// Receiver for incoming messages.
    receiver: mpsc::Receiver<Message>,
    /// Sender for outgoing messages (to the broker).
    broker_sender: mpsc::Sender<Message>,
}

/// Broker that routes messages between agents.
pub struct MessageBroker {
    /// Map of agent ID to their message sender.
    agents: Arc<RwLock<HashMap<String, mpsc::Sender<Message>>>>,
    /// Configuration.
    config: ChannelConfig,
}

impl MessageBroker {
    /// Create a new message broker.
    pub fn new(config: ChannelConfig) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }
    
    /// Register an agent with the broker.
    /// Returns a sender for sending messages to this agent.
    pub async fn register_agent(&self, agent_id: &str) -> AgentChannel {
        let (tx, rx) = mpsc::channel(self.config.capacity);
        
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id.to_string(), tx);
        }
        
        debug!("Registered agent: {}", agent_id);
        
        AgentChannel {
            agent_id: agent_id.to_string(),
            receiver: rx,
            broker: self.agents.clone(),
            config: self.config.clone(),
        }
    }
    
    /// Unregister an agent from the broker.
    pub async fn unregister_agent(&self, agent_id: &str) {
        let mut agents = self.agents.write().await;
        agents.remove(agent_id);
        debug!("Unregistered agent: {}", agent_id);
    }
    
    /// Send a message from one agent to another.
    pub async fn deliver(&self, message: Message) -> Result<DeliveryStatus> {
        let agents = self.agents.read().await;
        
        if let Some(sender) = agents.get(&message.to) {
            // Try to send with retry
            for attempt in 0..self.config.retry_policy.max_attempts {
                match sender.try_send(message.clone()) {
                    Ok(()) => {
                        debug!(
                            "Delivered message {} from {} to {}",
                            message.id, message.from, message.to
                        );
                        return Ok(DeliveryStatus::Delivered);
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!(
                            "Channel full for agent {}, attempt {}/{}",
                            message.to, attempt + 1, self.config.retry_policy.max_attempts
                        );
                        if attempt + 1 < self.config.retry_policy.max_attempts {
                            tokio::time::sleep(self.config.retry_policy.delay_for_attempt(attempt)).await;
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        warn!("Channel closed for agent {}", message.to);
                        return Ok(DeliveryStatus::Failed);
                    }
                }
            }
            Ok(DeliveryStatus::Failed)
        } else {
            debug!("Agent {} not found for message delivery", message.to);
            Ok(DeliveryStatus::Rejected)
        }
    }
    
    /// Get the number of registered agents.
    pub async fn agent_count(&self) -> usize {
        self.agents.read().await.len()
    }
}

/// Communication channel for a single agent.
pub struct AgentChannel {
    /// This agent's ID.
    agent_id: String,
    /// Receiver for incoming messages.
    receiver: mpsc::Receiver<Message>,
    /// Reference to the broker for routing.
    broker: Arc<RwLock<HashMap<String, mpsc::Sender<Message>>>>,
    /// Channel configuration.
    config: ChannelConfig,
}

impl AgentChannel {
    /// Get the agent ID.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    
    /// Send a message to another agent.
    pub async fn send(&mut self, message: Message) -> Result<DeliveryStatus> {
        if message.from != self.agent_id {
            return Err(anyhow!("Message 'from' must match agent ID"));
        }
        
        let broker = self.broker.read().await;
        
        if let Some(sender) = broker.get(&message.to) {
            for attempt in 0..self.config.retry_policy.max_attempts {
                match sender.try_send(message.clone()) {
                    Ok(()) => {
                        debug!(
                            "Agent {} sent message {} to {}",
                            self.agent_id, message.id, message.to
                        );
                        return Ok(DeliveryStatus::Delivered);
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!(
                            "Channel full for agent {}, attempt {}/{}",
                            message.to, attempt + 1, self.config.retry_policy.max_attempts
                        );
                        if attempt + 1 < self.config.retry_policy.max_attempts {
                            tokio::time::sleep(self.config.retry_policy.delay_for_attempt(attempt)).await;
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        warn!("Channel closed for agent {}", message.to);
                        return Ok(DeliveryStatus::Failed);
                    }
                }
            }
            Ok(DeliveryStatus::Failed)
        } else {
            debug!("Recipient agent {} not found", message.to);
            Ok(DeliveryStatus::Rejected)
        }
    }
    
    /// Receive the next message, waiting if necessary.
    pub async fn recv(&mut self) -> Option<Message> {
        self.receiver.recv().await
    }
    
    /// Try to receive a message without waiting.
    pub fn try_recv(&mut self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }
    
    /// Receive a message with a timeout.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<Message> {
        tokio::time::timeout(timeout, self.receiver.recv())
            .await
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_message_creation() {
        let msg = Message::new(
            "agent1",
            "agent2",
            "task_request",
            json!({"task": "analyze"}),
        );
        
        assert_eq!(msg.from, "agent1");
        assert_eq!(msg.to, "agent2");
        assert_eq!(msg.message_type, "task_request");
        assert!(msg.correlation_id.is_none());
    }
    
    #[test]
    fn test_message_reply() {
        let msg = Message::new(
            "agent1",
            "agent2",
            "task_request",
            json!({"task": "analyze"}),
        );
        
        let reply = msg.reply(json!({"status": "done"}));
        
        assert_eq!(reply.from, "agent2");
        assert_eq!(reply.to, "agent1");
        assert_eq!(reply.correlation_id, Some(msg.id));
        assert!(reply.message_type.starts_with("reply:"));
    }
    
    #[test]
    fn test_message_payload_deserialization() {
        #[derive(Deserialize)]
        struct TaskPayload {
            task: String,
        }
        
        let msg = Message::new(
            "agent1",
            "agent2",
            "task",
            json!({"task": "analyze"}),
        );
        
        let payload: TaskPayload = msg.payload_as().unwrap();
        assert_eq!(payload.task, "analyze");
    }
    
    #[test]
    fn test_retry_policy_delay() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_delay_ms: 1000,
        };
        
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
        // Would be 800 but capped at max_delay_ms
        assert_eq!(policy.delay_for_attempt(10), Duration::from_millis(1000));
    }
    
    #[tokio::test]
    async fn test_broker_registration() {
        let broker = MessageBroker::new(ChannelConfig::default());
        
        let _channel1 = broker.register_agent("agent1").await;
        let _channel2 = broker.register_agent("agent2").await;
        
        assert_eq!(broker.agent_count().await, 2);
        
        broker.unregister_agent("agent1").await;
        assert_eq!(broker.agent_count().await, 1);
    }
    
    #[tokio::test]
    async fn test_message_delivery() {
        let broker = MessageBroker::new(ChannelConfig::default());
        
        let mut channel1 = broker.register_agent("agent1").await;
        let mut channel2 = broker.register_agent("agent2").await;
        
        // Send message from agent1 to agent2
        let msg = Message::new("agent1", "agent2", "test", json!("hello"));
        let status = channel1.send(msg.clone()).await.unwrap();
        assert_eq!(status, DeliveryStatus::Delivered);
        
        // Agent2 receives the message
        let received = channel2.recv_timeout(Duration::from_millis(100)).await;
        assert!(received.is_some());
        let received = received.unwrap();
        assert_eq!(received.payload, json!("hello"));
    }
    
    #[tokio::test]
    async fn test_message_to_unknown_agent() {
        let broker = MessageBroker::new(ChannelConfig::default());
        
        let mut channel1 = broker.register_agent("agent1").await;
        
        // Try to send to non-existent agent
        let msg = Message::new("agent1", "unknown", "test", json!("hello"));
        let status = channel1.send(msg).await.unwrap();
        assert_eq!(status, DeliveryStatus::Rejected);
    }
    
    #[tokio::test]
    async fn test_high_throughput() {
        let broker = MessageBroker::new(ChannelConfig {
            capacity: 1000,
            ..Default::default()
        });
        
        let mut sender = broker.register_agent("sender").await;
        let mut receiver = broker.register_agent("receiver").await;
        
        // Send 100 messages
        let start = std::time::Instant::now();
        for i in 0..100 {
            let msg = Message::new("sender", "receiver", "test", json!(i));
            sender.send(msg).await.unwrap();
        }
        let send_time = start.elapsed();
        
        // Should complete in under 100ms (1000+ msg/sec)
        assert!(send_time < Duration::from_millis(100));
        
        // Receive all messages
        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 100);
    }
}
