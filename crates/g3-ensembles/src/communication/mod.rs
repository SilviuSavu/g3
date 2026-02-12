//! Inter-agent communication protocol.
//!
//! Provides typed message passing between agents in a workflow, replacing
//! file-based mailboxes with in-memory channels for better performance.

mod channel;

pub use channel::{AgentChannel, ChannelConfig, Message};
