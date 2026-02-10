//! Topology construction from embedded descriptors.

use std::collections::HashMap;
use super::descriptor::EmbeddedDescriptors;

#[derive(Debug, Clone)]
pub struct TopologyEdge {
    pub from: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct TopologyGraph {
    pub incoming: HashMap<String, Vec<TopologyEdge>>,
}

impl TopologyGraph {
    pub fn senders_to(&self, agent_id: &str) -> &[TopologyEdge] {
        self.incoming.get(agent_id).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn edge_count(&self) -> usize {
        self.incoming.values().map(|v| v.len()).sum()
    }
    pub fn agent_count(&self) -> usize {
        self.incoming.len()
    }
}

pub struct TopologyBuilder {
    pub tau_edge: f32,
    pub k_in: usize,
}

impl TopologyBuilder {
    pub fn new(tau_edge: f32, k_in: usize) -> Self {
        Self { tau_edge, k_in }
    }

    pub fn build(&self, descriptors: &[EmbeddedDescriptors]) -> TopologyGraph {
        let n = descriptors.len();
        let mut incoming: HashMap<String, Vec<TopologyEdge>> = HashMap::new();
        for d in descriptors {
            incoming.insert(d.agent_id.clone(), Vec::new());
        }
        for i in 0..n {
            let mut candidates: Vec<TopologyEdge> = Vec::new();
            for j in 0..n {
                if i == j { continue; }
                let sim = cosine_similarity(&descriptors[i].query_vec, &descriptors[j].key_vec);
                if sim > self.tau_edge {
                    candidates.push(TopologyEdge { from: descriptors[j].agent_id.clone(), score: sim });
                }
            }
            candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(self.k_in);
            incoming.insert(descriptors[i].agent_id.clone(), candidates);
        }
        TopologyGraph { incoming }
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let mut dot = 0.0f32;
    let mut mag_a = 0.0f32;
    let mut mag_b = 0.0f32;
    for (ai, bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        mag_a += ai * ai;
        mag_b += bi * bi;
    }
    let mag = (mag_a * mag_b).sqrt();
    if mag == 0.0 { 0.0 } else { dot / mag }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }
    #[test]
    fn test_cosine_similarity_orthogonal() {
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }
    #[test]
    fn test_cosine_similarity_opposite() {
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
    }
    #[test]
    fn test_cosine_similarity_zero_vector() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[0.0, 0.0]), 0.0);
    }
    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }
    #[test]
    fn test_cosine_similarity_mismatched_len() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
    }
    #[test]
    fn test_topology_builder_basic() {
        let descriptors = vec![
            EmbeddedDescriptors { agent_id: "a".into(), query_vec: vec![1.0, 0.0, 0.0], key_vec: vec![0.0, 1.0, 0.0] },
            EmbeddedDescriptors { agent_id: "b".into(), query_vec: vec![0.0, 1.0, 0.0], key_vec: vec![1.0, 0.0, 0.0] },
        ];
        let graph = TopologyBuilder::new(0.5, 3).build(&descriptors);
        assert_eq!(graph.senders_to("a").len(), 1);
        assert_eq!(graph.senders_to("a")[0].from, "b");
    }
    #[test]
    fn test_topology_builder_tau_threshold() {
        let descriptors = vec![
            EmbeddedDescriptors { agent_id: "a".into(), query_vec: vec![1.0, 0.1, 0.0], key_vec: vec![0.0, 0.0, 1.0] },
            EmbeddedDescriptors { agent_id: "b".into(), query_vec: vec![0.0, 0.0, 1.0], key_vec: vec![0.5, 0.5, 0.0] },
        ];
        let graph = TopologyBuilder::new(0.9, 3).build(&descriptors);
        assert_eq!(graph.senders_to("a").len(), 0);
    }
    #[test]
    fn test_topology_no_self_edges() {
        let descriptors = vec![
            EmbeddedDescriptors { agent_id: "a".into(), query_vec: vec![1.0, 0.0], key_vec: vec![1.0, 0.0] },
        ];
        let graph = TopologyBuilder::new(0.0, 3).build(&descriptors);
        assert_eq!(graph.senders_to("a").len(), 0);
    }
    #[test]
    fn test_topology_graph_edge_count() {
        let descriptors = vec![
            EmbeddedDescriptors { agent_id: "a".into(), query_vec: vec![1.0, 0.0], key_vec: vec![0.0, 1.0] },
            EmbeddedDescriptors { agent_id: "b".into(), query_vec: vec![0.0, 1.0], key_vec: vec![1.0, 0.0] },
        ];
        let graph = TopologyBuilder::new(0.5, 3).build(&descriptors);
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.agent_count(), 2);
    }
}
