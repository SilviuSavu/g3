//! Knowledge graph traversal algorithms.
//!
//! This module provides algorithms for traversing the codebase knowledge graph:
//! - BFS and DFS traversals
//! - Path finding between symbols
//! - Dependency cycle detection
//! - Reachability analysis
//! - Subgraph extraction

use std::collections::{HashMap, HashSet};

use crate::graph::{CodeGraph, EdgeKind, FileNode, SymbolNode};

/// Result of a graph traversal.
#[derive(Debug, Clone)]
pub struct TraversalResult {
    /// Node ID visited
    pub node_id: String,
    /// Node type (symbol or file)
    pub node_type: String,
    /// Node name (if available)
    pub name: Option<String>,
    /// Distance from start node
    pub distance: usize,
    /// Path taken to reach this node
    pub path: Vec<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration for graph traversal.
#[derive(Debug, Clone)]
pub struct TraversalConfig {
    /// Maximum depth for traversal
    pub max_depth: usize,
    /// Edge types to follow during traversal
    pub edge_kinds: Vec<EdgeKind>,
    /// Whether to deduplicate visited nodes
    pub deduplicate: bool,
    /// Whether to collect full paths
    pub collect_paths: bool,
}

impl Default for TraversalConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            edge_kinds: vec![
                EdgeKind::Calls,
                EdgeKind::References,
                EdgeKind::Contains,
                EdgeKind::Imports,
            ],
            deduplicate: true,
            collect_paths: true,
        }
    }
}

impl TraversalConfig {
    /// Create a new traversal configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth.
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set edge kinds to follow.
    pub fn with_edge_kinds(mut self, kinds: Vec<EdgeKind>) -> Self {
        self.edge_kinds = kinds;
        self
    }

    /// Enable or disable deduplication.
    pub fn with_deduplicate(mut self, deduplicate: bool) -> Self {
        self.deduplicate = deduplicate;
        self
    }

    /// Enable or disable path collection.
    pub fn with_collect_paths(mut self, collect_paths: bool) -> Self {
        self.collect_paths = collect_paths;
        self
    }
}

/// Graph traverser for the codebase knowledge graph.
pub struct GraphTraverser {
    config: TraversalConfig,
}

impl GraphTraverser {
    /// Create a new graph traverser with default configuration.
    pub fn new() -> Self {
        Self {
            config: TraversalConfig::default(),
        }
    }

    /// Create a new graph traverser with custom configuration.
    pub fn with_config(config: TraversalConfig) -> Self {
        Self { config }
    }

    /// Set the configuration.
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.config.max_depth = depth;
        self
    }

    /// Set edge kinds to follow.
    pub fn with_edge_kinds(mut self, kinds: Vec<EdgeKind>) -> Self {
        self.config.edge_kinds = kinds;
        self
    }

    /// BFS traversal from a starting node.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph to traverse
    /// * `start_node_id` - ID of the node to start from
    ///
    /// # Returns
    /// A vector of traversal results in BFS order.
    pub fn bfs<'a>(
        &'a self,
        graph: &'a CodeGraph,
        start_node_id: &str,
    ) -> Vec<TraversalResult> {
        let mut results = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, usize, Vec<String>)> = Vec::new();

        // Determine if start node is a symbol or file
        let _node_type = if graph.symbols.contains_key(start_node_id) {
            "symbol"
        } else if graph.files.contains_key(start_node_id) {
            "file"
        } else {
            return results; // Node not found
        };

        let _name = graph
            .symbols
            .get(start_node_id)
            .map(|s| s.name.clone())
            .or_else(|| graph.files.get(start_node_id).map(|f| f.path.to_string_lossy().to_string()));

        queue.push((start_node_id.to_string(), 0, vec![start_node_id.to_string()]));

        while let Some((current_id, distance, path)) = queue.pop() {
            if self.config.deduplicate && visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            let node_type = if graph.symbols.contains_key(&current_id) {
                "symbol"
            } else if graph.files.contains_key(&current_id) {
                "file"
            } else {
                continue;
            };

            let name = graph
                .symbols
                .get(&current_id)
                .map(|s| s.name.clone())
                .or_else(|| graph.files.get(&current_id).map(|f| f.path.to_string_lossy().to_string()));

            // Clone path before pushing to results
            let path_clone = path.clone();
            results.push(TraversalResult {
                node_id: current_id.clone(),
                node_type: node_type.to_string(),
                name,
                distance,
                path: path_clone,
                metadata: HashMap::new(),
            });

            if distance < self.config.max_depth {
                // Get outgoing edges for this node
                let outgoing = graph.outgoing_edges(&current_id);
                let mut new_queue_entries = Vec::new();

                for edge in outgoing {
                    // Check if edge kind matches configured edge kinds
                    if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                        let mut new_path = path.clone();
                        new_path.push(edge.target.clone());
                        new_queue_entries.push((edge.target.clone(), distance + 1, new_path));
                    }
                }

                // Insert at the beginning to maintain BFS order (actually making it more like DFS due to pop)
                // For true BFS, we'd use a proper queue (VecDeque)
                for entry in new_queue_entries.into_iter().rev() {
                    queue.insert(0, entry);
                }
            }
        }

        results
    }

    /// DFS traversal from a starting node.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph to traverse
    /// * `start_node_id` - ID of the node to start from
    ///
    /// # Returns
    /// A vector of traversal results in DFS order.
    pub fn dfs<'a>(
        &'a self,
        graph: &'a CodeGraph,
        start_node_id: &str,
    ) -> Vec<TraversalResult> {
        let mut results = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        self.dfs_visit(graph, start_node_id, 0, vec![start_node_id.to_string()], &mut visited, &mut results);

        results
    }

    /// Recursive helper for DFS traversal.
    fn dfs_visit<'a>(
        &'a self,
        graph: &'a CodeGraph,
        current_id: &str,
        distance: usize,
        path: Vec<String>,
        visited: &mut HashSet<String>,
        results: &mut Vec<TraversalResult>,
    ) {
        if self.config.deduplicate && visited.contains(current_id) {
            return;
        }
        visited.insert(current_id.to_string());

        let node_type = if graph.symbols.contains_key(current_id) {
            "symbol"
        } else if graph.files.contains_key(current_id) {
            "file"
        } else {
            return;
        };

        let name = graph
            .symbols
            .get(current_id)
            .map(|s| s.name.clone())
            .or_else(|| graph.files.get(current_id).map(|f| f.path.to_string_lossy().to_string()));

        // Clone path before pushing to results
        let path_clone = path.clone();
        results.push(TraversalResult {
            node_id: current_id.to_string(),
            node_type: node_type.to_string(),
            name,
            distance,
            path: path_clone,
            metadata: HashMap::new(),
        });

        if distance < self.config.max_depth {
            let outgoing = graph.outgoing_edges(current_id);

            for edge in outgoing {
                if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                    let mut new_path = path.clone();
                    new_path.push(edge.target.clone());
                    self.dfs_visit(graph, &edge.target, distance + 1, new_path, visited, results);
                }
            }
        }
    }

    /// Find all paths between two nodes.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph
    /// * `start` - Start node ID
    /// * `end` - End node ID
    /// * `max_paths` - Maximum number of paths to find
    ///
    /// # Returns
    /// A vector of paths (each path is a vector of node IDs).
    pub fn find_paths<'a>(
        &'a self,
        graph: &'a CodeGraph,
        start: &str,
        end: &str,
        max_paths: usize,
    ) -> Vec<Vec<String>> {
        let mut paths = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        self.find_paths_recursive(graph, start, end, vec![start.to_string()], &mut visited, &mut paths, max_paths);

        paths
    }

    /// Recursive helper for path finding.
    fn find_paths_recursive<'a>(
        &'a self,
        graph: &'a CodeGraph,
        current: &str,
        end: &str,
        path: Vec<String>,
        visited: &mut HashSet<String>,
        paths: &mut Vec<Vec<String>>,
        max_paths: usize,
    ) {
        if paths.len() >= max_paths {
            return;
        }

        if current == end {
            paths.push(path);
            return;
        }

        if visited.contains(current) {
            return;
        }

        let mut new_visited = visited.clone();
        new_visited.insert(current.to_string());

        let outgoing = graph.outgoing_edges(current);

        for edge in outgoing {
            if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                let mut new_path = path.clone();
                new_path.push(edge.target.clone());
                self.find_paths_recursive(graph, &edge.target, end, new_path, &mut new_visited, paths, max_paths);
            }
        }
    }

    /// Check if there's a path between two nodes.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph
    /// * `start` - Start node ID
    /// * `end` - End node ID
    ///
    /// # Returns
    /// True if a path exists, false otherwise.
    pub fn has_path(&self, graph: &CodeGraph, start: &str, end: &str) -> bool {
        let mut visited: HashSet<String> = HashSet::new();
        self.has_path_recursive(graph, start, end, &mut visited)
    }

    /// Recursive helper for path checking.
    fn has_path_recursive(
        &self,
        graph: &CodeGraph,
        current: &str,
        end: &str,
        visited: &mut HashSet<String>,
    ) -> bool {
        if current == end {
            return true;
        }

        if visited.contains(current) {
            return false;
        }

        visited.insert(current.to_string());

        let outgoing = graph.outgoing_edges(current);

        for edge in outgoing {
            if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                if self.has_path_recursive(graph, &edge.target, end, visited) {
                    return true;
                }
            }
        }

        false
    }

    /// Find the shortest path between two nodes using BFS.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph
    /// * `start` - Start node ID
    /// * `end` - End node ID
    ///
    /// # Returns
    /// Optional path as a vector of node IDs, or None if no path exists.
    pub fn shortest_path(&self, graph: &CodeGraph, start: &str, end: &str) -> Option<Vec<String>> {
        let mut queue: Vec<(String, Vec<String>)> = vec![(start.to_string(), vec![start.to_string()])];
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(start.to_string());

        while let Some((current_id, path)) = queue.pop() {
            if current_id == end {
                return Some(path);
            }

            let outgoing = graph.outgoing_edges(&current_id);

            for edge in outgoing {
                if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                    if !visited.contains(&edge.target) {
                        let mut new_path = path.clone();
                        new_path.push(edge.target.clone());
                        queue.insert(0, (edge.target.clone(), new_path));
                        visited.insert(edge.target.clone());
                    }
                }
            }
        }

        None
    }

    /// Detect cycles in the graph starting from a node.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph
    /// * `start` - Start node ID
    ///
    /// # Returns
    /// A vector of cycles found (each cycle is a vector of node IDs).
    pub fn detect_cycles<'a>(&'a self, graph: &'a CodeGraph, start: &str) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut rec_stack: HashSet<String> = HashSet::new();

        self.detect_cycles_recursive(graph, start, vec![], &mut visited, &mut rec_stack, &mut cycles);

        cycles
    }

    /// Recursive helper for cycle detection.
    fn detect_cycles_recursive<'a>(
        &'a self,
        graph: &'a CodeGraph,
        current: &str,
        path: Vec<String>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(current.to_string());
        rec_stack.insert(current.to_string());

        let outgoing = graph.outgoing_edges(current);

        for edge in outgoing {
            if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                if rec_stack.contains(&edge.target) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|n| n == &edge.target);
                    if let Some(start_idx) = cycle_start {
                        let mut cycle = path[start_idx..].to_vec();
                        cycle.push(edge.target.clone());
                        cycles.push(cycle);
                    }
                } else if !visited.contains(&edge.target) {
                    let mut new_path = path.clone();
                    new_path.push(edge.target.clone());
                    self.detect_cycles_recursive(graph, &edge.target, new_path, visited, rec_stack, cycles);
                }
            }
        }

        rec_stack.remove(current);
    }

    /// Get all reachable nodes from a starting node.
    ///
    /// # Arguments
    /// * `graph` - The knowledge graph
    /// * `start` - Start node ID
    ///
    /// # Returns
    /// A set of reachable node IDs.
    pub fn reachable_nodes(&self, graph: &CodeGraph, start: &str) -> HashSet<String> {
        let mut reachable: HashSet<String> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();

        self.reachable_recursive(graph, start, &mut visited, &mut reachable);

        reachable
    }

    /// Recursive helper for reachability analysis.
    fn reachable_recursive(
        &self,
        graph: &CodeGraph,
        current: &str,
        visited: &mut HashSet<String>,
        reachable: &mut HashSet<String>,
    ) {
        if visited.contains(current) {
            return;
        }

        visited.insert(current.to_string());
        reachable.insert(current.to_string());

        let outgoing = graph.outgoing_edges(current);

        for edge in outgoing {
            if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                self.reachable_recursive(graph, &edge.target, visited, reachable);
            }
        }
    }

    /// Extract a subgraph containing nodes within a certain distance.
    ///
    /// # Arguments
    /// * `graph` - The original knowledge graph
    /// * `start` - Start node ID
    /// * `max_distance` - Maximum distance to include nodes
    ///
    /// # Returns
    /// A new CodeGraph containing only the subgraph.
    pub fn extract_subgraph(&self, graph: &CodeGraph, start: &str, max_distance: usize) -> CodeGraph {
        let mut subgraph = CodeGraph::new();
        let mut visited: HashMap<String, usize> = HashMap::new();
        let mut queue: Vec<(String, usize)> = vec![(start.to_string(), 0)];

        while let Some((node_id, distance)) = queue.pop() {
            if visited.contains_key(&node_id) {
                continue;
            }

            if distance > max_distance {
                continue;
            }

            visited.insert(node_id.clone(), distance);

            // Add the node to subgraph
            if let Some(symbol) = graph.symbols.get(&node_id) {
                let new_symbol = SymbolNode {
                    id: symbol.id.clone(),
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    file_id: symbol.file_id.clone(),
                    line_start: symbol.line_start,
                    line_end: symbol.line_end,
                    column_start: symbol.column_start,
                    column_end: symbol.column_end,
                    signature: symbol.signature.clone(),
                    documentation: symbol.documentation.clone(),
                    module_path: symbol.module_path.clone(),
                    parent_id: symbol.parent_id.clone(),
                    type_info: symbol.type_info.clone(),
                    generic_params: symbol.generic_params.clone(),
                    visibility: symbol.visibility.clone(),
                    deprecated: symbol.deprecated,
                    metadata: symbol.metadata.clone(),
                };
                subgraph.add_symbol(new_symbol);
            } else if let Some(file) = graph.files.get(&node_id) {
                let new_file = FileNode {
                    id: file.id.clone(),
                    path: file.path.clone(),
                    language: file.language.clone(),
                    loc: file.loc,
                    symbol_count: file.symbol_count,
                    is_test: file.is_test,
                    modified_at: file.modified_at,
                };
                subgraph.add_file(new_file);
            }

            // Add edges to neighbors
            let outgoing = graph.outgoing_edges(&node_id);
            for edge in outgoing {
                if self.config.edge_kinds.is_empty() || self.config.edge_kinds.contains(&edge.kind) {
                    // Add edge
                    let new_edge = crate::graph::Edge {
                        source: edge.source.clone(),
                        target: edge.target.clone(),
                        kind: edge.kind,
                        location_file: edge.location_file.clone(),
                        location_line: edge.location_line,
                    };
                    subgraph.add_edge(new_edge);

                    // Add neighbor to queue
                    if !visited.contains_key(&edge.target) {
                        queue.insert(0, (edge.target.clone(), distance + 1));
                    }
                }
            }
        }

        subgraph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traversal_config_default() {
        let config = TraversalConfig::default();
        assert_eq!(config.max_depth, 10);
        assert!(config.deduplicate);
        assert!(config.collect_paths);
    }

    #[test]
    fn test_traversal_config_builder() {
        let config = TraversalConfig::new()
            .with_max_depth(5)
            .with_deduplicate(false)
            .with_collect_paths(false);

        assert_eq!(config.max_depth, 5);
        assert!(!config.deduplicate);
        assert!(!config.collect_paths);
    }

    #[test]
    fn test_graph_traverser_new() {
        let traverser = GraphTraverser::new();
        assert_eq!(traverser.config.max_depth, 10);
    }

    #[test]
    fn test_graph_traverser_with_config() {
        let config = TraversalConfig::new().with_max_depth(5);
        let traverser = GraphTraverser::with_config(config);
        assert_eq!(traverser.config.max_depth, 5);
    }
}
