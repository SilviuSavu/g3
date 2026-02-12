//! G3 Ensembles - Multi-agent ensemble functionality
//!
//! This crate provides functionality for running multiple G3 agents in coordination,
//! enabling parallel development across different architectural modules.
//! 
//! ## Workflow Engine
//! 
//! The `workflow` module provides a LangGraph-style DAG-based orchestration system
//! for coordinating multiple specialized agents with conditional routing.

pub mod dytopo;
pub mod flock;
pub mod status;
pub mod workflow;
mod tests;

/// Re-export main types for convenience
pub use dytopo::{DyTopoConfig, DyTopoCoordinator};
pub use flock::{FlockConfig, FlockMode};
pub use status::{FlockStatus, SegmentStatus};
pub use workflow::{Workflow, WorkflowBuilder, WorkflowState};
