//! Descriptor types for DyTopo agent communication.

use serde::{Deserialize, Serialize};

/// Raw descriptor pair from an agent's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptorPair {
    pub agent_id: String,
    pub query: String,
    pub key: String,
}

/// Descriptors with computed embedding vectors.
#[derive(Debug, Clone)]
pub struct EmbeddedDescriptors {
    pub agent_id: String,
    pub query_vec: Vec<f32>,
    pub key_vec: Vec<f32>,
}

/// Complete output from an agent's round execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoundOutput {
    pub public_msg: String,
    pub private_msg: String,
    pub query: String,
    pub key: String,
}

/// JSON structure expected at the end of agent responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptorBlock {
    pub public: String,
    pub private: String,
    pub query: String,
    pub key: String,
}

impl DescriptorBlock {
    pub fn parse_from_response(response: &str) -> Option<Self> {
        let mut last_json_block = None;
        let mut search_from = 0;
        while let Some(start) = response[search_from..].find("```json") {
            let abs_start = search_from + start + 7;
            if let Some(end) = response[abs_start..].find("```") {
                let abs_end = abs_start + end;
                let json_str = response[abs_start..abs_end].trim();
                last_json_block = Some(json_str.to_string());
                search_from = abs_end + 3;
            } else {
                break;
            }
        }
        if last_json_block.is_none() {
            if let Some(start) = response.rfind('{') {
                if let Some(end) = response[start..].rfind('}') {
                    let json_str = &response[start..=start + end];
                    last_json_block = Some(json_str.to_string());
                }
            }
        }
        let json_str = last_json_block?;
        serde_json::from_str(&json_str).ok()
    }

    pub fn into_round_output(self) -> AgentRoundOutput {
        AgentRoundOutput {
            public_msg: self.public,
            private_msg: self.private,
            query: self.query,
            key: self.key,
        }
    }

    pub fn into_descriptor_pair(self, agent_id: String) -> DescriptorPair {
        DescriptorPair { agent_id, query: self.query, key: self.key }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_descriptor_block_from_fenced_json() {
        let response = r#"I analyzed the code.

```json
{
  "public": "Found 3 bugs in auth module",
  "private": "The session handler has a race condition at line 42",
  "query": "Need review of database connection pooling",
  "key": "Auth module analysis and bug identification"
}
```"#;
        let block = DescriptorBlock::parse_from_response(response).unwrap();
        assert_eq!(block.public, "Found 3 bugs in auth module");
        assert_eq!(block.query, "Need review of database connection pooling");
    }

    #[test]
    fn test_parse_descriptor_block_from_raw_json() {
        let response = r#"Done.
{"public": "Done", "private": "Details", "query": "Need help", "key": "Can offer testing"}"#;
        let block = DescriptorBlock::parse_from_response(response).unwrap();
        assert_eq!(block.public, "Done");
    }

    #[test]
    fn test_parse_last_json_block_when_multiple() {
        let response = r#"First:
```json
{"public": "old", "private": "old", "query": "old", "key": "old"}
```
Updated:
```json
{"public": "new", "private": "new", "query": "new", "key": "new"}
```"#;
        let block = DescriptorBlock::parse_from_response(response).unwrap();
        assert_eq!(block.public, "new");
    }

    #[test]
    fn test_parse_returns_none_for_invalid() {
        assert!(DescriptorBlock::parse_from_response("no json here").is_none());
        assert!(DescriptorBlock::parse_from_response("").is_none());
    }

    #[test]
    fn test_into_round_output() {
        let block = DescriptorBlock {
            public: "pub".into(), private: "priv".into(),
            query: "q".into(), key: "k".into(),
        };
        let output = block.into_round_output();
        assert_eq!(output.public_msg, "pub");
        assert_eq!(output.private_msg, "priv");
    }

    #[test]
    fn test_into_descriptor_pair() {
        let block = DescriptorBlock {
            public: "pub".into(), private: "priv".into(),
            query: "q".into(), key: "k".into(),
        };
        let pair = block.into_descriptor_pair("agent1".into());
        assert_eq!(pair.agent_id, "agent1");
        assert_eq!(pair.query, "q");
    }
}
