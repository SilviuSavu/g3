//! DAG-based parallel task execution.
//!
//! Provides parallel execution of independent workflow nodes with:
//! - Dependency resolution
//! - Circular dependency detection
//! - Configurable max parallelism
//! - Resource limit enforcement

use crate::workflow::{Node, Workflow, WorkflowState, AgentRole};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, info, warn};

/// Maximum number of parallel tasks by default.
pub const DEFAULT_MAX_PARALLELISM: usize = 4;

/// Configuration for parallel execution.
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Maximum number of tasks to run in parallel.
    pub max_parallelism: usize,
    /// Whether to continue on task failure.
    pub continue_on_failure: bool,
    /// Timeout for individual tasks (seconds).
    pub task_timeout_secs: u64,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_parallelism: DEFAULT_MAX_PARALLELISM,
            continue_on_failure: false,
            task_timeout_secs: 300,
        }
    }
}

/// A node in the DAG with its dependencies.
#[derive(Debug, Clone)]
pub struct DagNode {
    /// Node ID.
    pub id: String,
    /// Node definition.
    pub node: Node,
    /// IDs of nodes this node depends on.
    pub dependencies: HashSet<String>,
    /// IDs of nodes that depend on this node.
    pub dependents: HashSet<String>,
}

/// DAG representation of a workflow.
#[derive(Debug)]
pub struct Dag {
    /// Nodes indexed by ID.
    pub nodes: HashMap<String, DagNode>,
    /// Entry point node IDs.
    pub entrypoints: Vec<String>,
    /// Maximum depth of the DAG.
    pub max_depth: usize,
}

impl Dag {
    /// Build a DAG from a workflow.
    pub fn from_workflow(workflow: &Workflow) -> Result<Self> {
        let mut nodes: HashMap<String, DagNode> = HashMap::new();
        
        // Add all nodes
        for (node_id, node) in &workflow.nodes {
            nodes.insert(node_id.clone(), DagNode {
                id: node_id.clone(),
                node: node.clone(),
                dependencies: HashSet::new(),
                dependents: HashSet::new(),
            });
        }
        
        // Add dependencies from edges
        for edge in &workflow.edges {
            // edge.to depends on edge.from
            if let Some(dep_node) = nodes.get_mut(&edge.to) {
                dep_node.dependencies.insert(edge.from.clone());
            }
            if let Some(from_node) = nodes.get_mut(&edge.from) {
                from_node.dependents.insert(edge.to.clone());
            }
        }
        
        // Validate - check for cycles
        validate_dag(&nodes)?;
        
        // Find entrypoints (nodes with no dependencies)
        let entrypoints: Vec<String> = nodes
            .values()
            .filter(|n| n.dependencies.is_empty())
            .map(|n| n.id.clone())
            .collect();
        
        // Calculate max depth
        let max_depth = calculate_max_depth(&nodes, &entrypoints);
        
        Ok(Self {
            nodes,
            entrypoints,
            max_depth,
        })
    }
    
    /// Get execution levels (nodes at each level can run in parallel).
    pub fn execution_levels(&self) -> Vec<Vec<String>> {
        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();
        let mut remaining: HashSet<String> = self.nodes.keys().cloned().collect();
        
        while !remaining.is_empty() {
            // Find all nodes whose dependencies are satisfied
            let ready: Vec<String> = remaining
                .iter()
                .filter(|id| {
                    let node = &self.nodes[*id];
                    node.dependencies.iter().all(|dep| completed.contains(dep))
                })
                .cloned()
                .collect();
            
            if ready.is_empty() {
                // Should not happen if DAG is valid
                warn!("No ready nodes but remaining: {:?}", remaining);
                break;
            }
            
            for id in &ready {
                completed.insert(id.clone());
                remaining.remove(id);
            }
            
            levels.push(ready);
        }
        
        levels
    }
}

/// Validate that the DAG has no cycles.
pub fn validate_dag(nodes: &HashMap<String, DagNode>) -> Result<()> {
    // Use Kahn's algorithm to detect cycles
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut visited = 0;
    
    // Calculate in-degrees
    for (id, node) in nodes {
        in_degree.insert(id.clone(), node.dependencies.len());
        if node.dependencies.is_empty() {
            queue.push_back(id.clone());
        }
    }
    
    // Process nodes
    while let Some(id) = queue.pop_front() {
        visited += 1;
        
        if let Some(node) = nodes.get(&id) {
            for dependent in &node.dependents {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
    }
    
    if visited != nodes.len() {
        // Find the cycle
        let remaining: Vec<_> = nodes.keys()
            .filter(|id| in_degree.get(*id).copied().unwrap_or(0) > 0)
            .collect();
        
        return Err(anyhow!(
            "Circular dependencies detected among nodes: {:?}",
            remaining
        ));
    }
    
    Ok(())
}

fn calculate_max_depth(nodes: &HashMap<String, DagNode>, entrypoints: &[String]) -> usize {
    let mut depths: HashMap<String, usize> = HashMap::new();
    let mut max_depth = 0;
    
    // BFS from entrypoints
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    for entry in entrypoints {
        queue.push_back((entry.clone(), 0));
    }
    
    while let Some((id, depth)) = queue.pop_front() {
        if let Some(existing) = depths.get(&id) {
            if *existing >= depth {
                continue; // Already visited with equal or greater depth
            }
        }
        
        depths.insert(id.clone(), depth);
        max_depth = max_depth.max(depth);
        
        if let Some(node) = nodes.get(&id) {
            for dependent in &node.dependents {
                queue.push_back((dependent.clone(), depth + 1));
            }
        }
    }
    
    max_depth
}

/// Result of executing a DAG level.
#[derive(Debug)]
pub struct LevelResult {
    /// Level number.
    pub level: usize,
    /// Results for each node in the level.
    pub results: HashMap<String, Result<String>>,
    /// Whether all nodes succeeded.
    pub all_succeeded: bool,
}

/// DAG executor for parallel workflow execution.
pub struct DagExecutor {
    /// DAG to execute.
    dag: Dag,
    /// Parallel execution configuration.
    config: ParallelConfig,
    /// Semaphore for limiting parallelism.
    semaphore: Arc<Semaphore>,
    /// Shared state.
    state: Arc<Mutex<WorkflowState>>,
}

impl DagExecutor {
    /// Create a new DAG executor.
    pub fn new(dag: Dag, config: ParallelConfig, state: WorkflowState) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_parallelism));
        Self {
            dag,
            config,
            semaphore,
            state: Arc::new(Mutex::new(state)),
        }
    }
    
    /// Execute the DAG level by level.
    pub async fn execute(&self) -> Result<Vec<LevelResult>> {
        let levels = self.dag.execution_levels();
        let mut results = Vec::new();
        
        info!(
            "Executing DAG with {} levels, max parallelism {}",
            levels.len(),
            self.config.max_parallelism
        );
        
        for (level_idx, level_nodes) in levels.iter().enumerate() {
            info!("Level {}: {} nodes", level_idx, level_nodes.len());
            
            let level_result = self.execute_level(level_idx, level_nodes).await;
            
            if !level_result.all_succeeded && !self.config.continue_on_failure {
                results.push(level_result);
                return Err(anyhow!("Level {} failed, stopping execution", level_idx));
            }
            
            results.push(level_result);
        }
        
        // Mark workflow as completed
        self.state.lock().await.complete();
        
        Ok(results)
    }
    
    async fn execute_level(&self, level: usize, nodes: &[String]) -> LevelResult {
        let mut results: HashMap<String, Result<String>> = HashMap::new();
        let mut all_succeeded = true;
        
        // Execute all nodes in parallel
        let mut handles = Vec::new();
        
        for node_id in nodes {
            let node_id = node_id.clone();
            let semaphore = self.semaphore.clone();
            let state = self.state.clone();
            let timeout_secs = self.config.task_timeout_secs;
            
            let handle = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = semaphore.acquire().await.unwrap();
                
                // Simulate node execution
                debug!("Executing node: {}", node_id);
                
                // Record start in state
                {
                    let mut state = state.lock().await;
                    state.start_node(&node_id);
                }
                
                // Simulate work (in real implementation, this would call the agent)
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    simulate_node_execution(&node_id)
                ).await;
                
                let (success, output) = match result {
                    Ok(Ok(output)) => (true, output),
                    Ok(Err(e)) => (false, e.to_string()),
                    Err(_) => (false, "Timeout".to_string()),
                };
                
                // Record completion in state
                {
                    let mut state = state.lock().await;
                    state.complete_node(success, &output);
                }
                
                (node_id, if success { Ok(output) } else { Err(anyhow!(output)) })
            });
            
            handles.push(handle);
        }
        
        // Collect results
        for handle in handles {
            let (node_id, result) = handle.await.unwrap();
            if result.is_err() {
                all_succeeded = false;
            }
            results.insert(node_id, result);
        }
        
        LevelResult {
            level,
            results,
            all_succeeded,
        }
    }
    
    /// Get the execution plan (levels).
    pub fn execution_plan(&self) -> Vec<Vec<String>> {
        self.dag.execution_levels()
    }
}

/// Simulate node execution (placeholder for actual agent execution).
async fn simulate_node_execution(node_id: &str) -> Result<String> {
    // Simulate some work
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    Ok(format!("Completed node: {}", node_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{WorkflowBuilder, Edge};
    
    fn create_linear_workflow() -> Workflow {
        WorkflowBuilder::new("linear")
            .node("a", AgentRole::Researcher)
            .node("b", AgentRole::Coder)
            .node("c", AgentRole::Tester)
            .edge("a", "b")
            .edge("b", "c")
            .edge("c", "DONE")
            .entrypoint("a")
            .build()
            .unwrap()
    }
    
    fn create_parallel_workflow() -> Workflow {
        WorkflowBuilder::new("parallel")
            .node("start", AgentRole::Custom("orchestrator".to_string()))
            .node("a", AgentRole::Researcher)
            .node("b", AgentRole::Coder)
            .node("c", AgentRole::Tester)
            .node("end", AgentRole::Deployer)
            .edge("start", "a")
            .edge("start", "b")
            .edge("start", "c")
            .edge("a", "end")
            .edge("b", "end")
            .edge("c", "end")
            .edge("end", "DONE")
            .entrypoint("start")
            .build()
            .unwrap()
    }
    
    fn create_diamond_workflow() -> Workflow {
        WorkflowBuilder::new("diamond")
            .node("start", AgentRole::Custom("orchestrator".to_string()))
            .node("left", AgentRole::Researcher)
            .node("right", AgentRole::Coder)
            .node("end", AgentRole::Tester)
            .edge("start", "left")
            .edge("start", "right")
            .edge("left", "end")
            .edge("right", "end")
            .edge("end", "DONE")
            .entrypoint("start")
            .build()
            .unwrap()
    }
    
    #[test]
    fn test_dag_linear_workflow() {
        let workflow = create_linear_workflow();
        let dag = Dag::from_workflow(&workflow).unwrap();
        
        assert_eq!(dag.entrypoints, vec!["a"]);
        assert_eq!(dag.max_depth, 3);
        
        let levels = dag.execution_levels();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1], vec!["b"]);
        assert_eq!(levels[2], vec!["c"]);
    }
    
    #[test]
    fn test_dag_parallel_workflow() {
        let workflow = create_parallel_workflow();
        let dag = Dag::from_workflow(&workflow).unwrap();
        
        // Start should be the only entrypoint
        assert_eq!(dag.entrypoints, vec!["start"]);
        
        let levels = dag.execution_levels();
        assert_eq!(levels.len(), 3);
        
        // Level 0: start
        assert_eq!(levels[0], vec!["start"]);
        
        // Level 1: a, b, c (in some order - all parallel)
        assert_eq!(levels[1].len(), 3);
        assert!(levels[1].contains(&"a".to_string()));
        assert!(levels[1].contains(&"b".to_string()));
        assert!(levels[1].contains(&"c".to_string()));
        
        // Level 2: end
        assert_eq!(levels[2], vec!["end"]);
    }
    
    #[test]
    fn test_dag_diamond_workflow() {
        let workflow = create_diamond_workflow();
        let dag = Dag::from_workflow(&workflow).unwrap();
        
        let levels = dag.execution_levels();
        assert_eq!(levels.len(), 3);
        
        // Level 0: start
        assert_eq!(levels[0], vec!["start"]);
        
        // Level 1: left and right (parallel)
        assert_eq!(levels[1].len(), 2);
        
        // Level 2: end
        assert_eq!(levels[2], vec!["end"]);
    }
    
    #[test]
    fn test_validate_no_cycle() {
        let workflow = create_parallel_workflow();
        let result = Dag::from_workflow(&workflow);
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_dag_executor_linear() {
        let workflow = create_linear_workflow();
        let dag = Dag::from_workflow(&workflow).unwrap();
        let state = WorkflowState::with_request("linear", "test");
        
        let executor = DagExecutor::new(dag, ParallelConfig::default(), state);
        let results = executor.execute().await.unwrap();
        
        // Should have 3 levels
        assert_eq!(results.len(), 3);
        
        // All levels should succeed
        for result in &results {
            assert!(result.all_succeeded);
        }
    }
    
    #[tokio::test]
    async fn test_dag_executor_parallel() {
        let workflow = create_parallel_workflow();
        let dag = Dag::from_workflow(&workflow).unwrap();
        let state = WorkflowState::with_request("parallel", "test");
        
        let config = ParallelConfig {
            max_parallelism: 3,
            ..Default::default()
        };
        
        let executor = DagExecutor::new(dag, config, state);
        
        // Verify execution plan
        let plan = executor.execution_plan();
        assert_eq!(plan.len(), 3);
        
        // Execute
        let results = executor.execute().await.unwrap();
        assert_eq!(results.len(), 3);
    }
    
    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert_eq!(config.max_parallelism, DEFAULT_MAX_PARALLELISM);
        assert!(!config.continue_on_failure);
        assert_eq!(config.task_timeout_secs, 300);
    }
    
    #[test]
    fn test_execution_levels_order() {
        // Create a more complex workflow
        let workflow = WorkflowBuilder::new("complex")
            .node("a", AgentRole::Researcher)
            .node("b", AgentRole::Coder)
            .node("c", AgentRole::Tester)
            .node("d", AgentRole::Reviewer)
            .node("e", AgentRole::Deployer)
            .edge("a", "b")
            .edge("a", "c")
            .edge("b", "d")
            .edge("c", "d")
            .edge("d", "e")
            .edge("e", "DONE")
            .entrypoint("a")
            .build()
            .unwrap();
        
        let dag = Dag::from_workflow(&workflow).unwrap();
        let levels = dag.execution_levels();
        
        // Level 0: a
        // Level 1: b, c (parallel, both depend only on a)
        // Level 2: d (depends on both b and c)
        // Level 3: e
        
        assert_eq!(levels.len(), 4);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1].len(), 2);
        assert_eq!(levels[2], vec!["d"]);
        assert_eq!(levels[3], vec!["e"]);
    }
}
