//! Message types and routing for DyTopo inter-agent communication.

use std::collections::HashMap;
use super::topology::TopologyGraph;

#[derive(Debug, Clone)]
pub struct Message {
    pub from: String,
    pub content: String,
    pub round: usize,
    pub relevance: f32,
}

pub fn route_messages(
    topology: &TopologyGraph,
    private_messages: &HashMap<String, String>,
    round: usize,
) -> HashMap<String, Vec<Message>> {
    let mut inboxes: HashMap<String, Vec<Message>> = HashMap::new();
    for (receiver_id, edges) in &topology.incoming {
        let mut inbox = Vec::new();
        for edge in edges {
            if let Some(content) = private_messages.get(&edge.from) {
                inbox.push(Message {
                    from: edge.from.clone(),
                    content: content.clone(),
                    round,
                    relevance: edge.score,
                });
            }
        }
        inbox.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
        inboxes.insert(receiver_id.clone(), inbox);
    }
    inboxes
}

pub fn format_inbox(messages: &[Message]) -> String {
    if messages.is_empty() { return String::new(); }
    let mut parts = vec!["## Messages from peers:\n".to_string()];
    for msg in messages {
        parts.push(format!("### From {} (relevance: {:.2}):\n{}\n", msg.from, msg.relevance, msg.content));
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dytopo::topology::{TopologyGraph, TopologyEdge};

    #[test]
    fn test_route_messages_basic() {
        let mut incoming = HashMap::new();
        incoming.insert("a".to_string(), vec![TopologyEdge { from: "b".to_string(), score: 0.9 }]);
        incoming.insert("b".to_string(), vec![]);
        let topology = TopologyGraph { incoming };
        let mut private_messages = HashMap::new();
        private_messages.insert("b".to_string(), "B's msg".to_string());
        let inboxes = route_messages(&topology, &private_messages, 1);
        assert_eq!(inboxes["a"].len(), 1);
        assert_eq!(inboxes["a"][0].from, "b");
        assert_eq!(inboxes["b"].len(), 0);
    }

    #[test]
    fn test_route_messages_sorted_by_relevance() {
        let mut incoming = HashMap::new();
        incoming.insert("a".to_string(), vec![
            TopologyEdge { from: "b".to_string(), score: 0.5 },
            TopologyEdge { from: "c".to_string(), score: 0.9 },
        ]);
        incoming.insert("b".to_string(), vec![]);
        incoming.insert("c".to_string(), vec![]);
        let topology = TopologyGraph { incoming };
        let mut pm = HashMap::new();
        pm.insert("b".to_string(), "from b".to_string());
        pm.insert("c".to_string(), "from c".to_string());
        let inboxes = route_messages(&topology, &pm, 0);
        assert_eq!(inboxes["a"][0].from, "c");
        assert_eq!(inboxes["a"][1].from, "b");
    }

    #[test]
    fn test_format_inbox_empty() {
        assert_eq!(format_inbox(&[]), "");
    }

    #[test]
    fn test_format_inbox_with_messages() {
        let msgs = vec![Message { from: "b".into(), content: "bug found".into(), round: 1, relevance: 0.85 }];
        let formatted = format_inbox(&msgs);
        assert!(formatted.contains("From b"));
        assert!(formatted.contains("bug found"));
    }
}
