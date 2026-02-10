//! DyTopo: Dynamic Topology Routing for multi-agent collaboration.

pub mod channel_ui_writer;
pub mod coordinator;
pub mod descriptor;
pub mod manager;
pub mod message;
pub mod topology;
pub mod worker;

pub use coordinator::{DyTopoConfig, DyTopoCoordinator};
pub use topology::TopologyGraph;
