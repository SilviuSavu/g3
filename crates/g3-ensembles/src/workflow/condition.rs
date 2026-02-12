//! Conditional edge evaluation for workflow routing.
//!
//! Conditions determine which edge to follow from a node based on workflow state.

use super::WorkflowState;
use serde::{Deserialize, Serialize};

/// A condition that determines if an edge should be taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Always take this edge
    Always,
    /// Never take this edge
    Never,
    /// Take if state key equals value
    Equals {
        key: String,
        value: serde_json::Value,
    },
    /// Take if state key is true
    IsTrue {
        key: String,
    },
    /// Take if state key is false
    IsFalse {
        key: String,
    },
    /// Take if state key exists
    Exists {
        key: String,
    },
    /// Take if state key matches regex
    Matches {
        key: String,
        pattern: String,
    },
    /// Take if state key is greater than value
    GreaterThan {
        key: String,
        value: f64,
    },
    /// Take if state key is less than value
    LessThan {
        key: String,
        value: f64,
    },
    /// Take if state key contains value (for arrays/strings)
    Contains {
        key: String,
        value: serde_json::Value,
    },
    /// Take if all nested conditions are true
    All(Vec<Condition>),
    /// Take if any nested condition is true
    Any(Vec<Condition>),
    /// Negate a condition
    Not(Box<Condition>),
}

impl Condition {
    /// Create an always-true condition
    pub fn always() -> Self {
        Condition::Always
    }
    
    /// Create an always-false condition
    pub fn never() -> Self {
        Condition::Never
    }
    
    /// Create an equality condition
    pub fn equals(key: impl Into<String>, value: serde_json::Value) -> Self {
        Condition::Equals {
            key: key.into(),
            value,
        }
    }
    
    /// Create an is-true condition
    pub fn is_true(key: impl Into<String>) -> Self {
        Condition::IsTrue { key: key.into() }
    }
    
    /// Create an is-false condition
    pub fn is_false(key: impl Into<String>) -> Self {
        Condition::IsFalse { key: key.into() }
    }
    
    /// Create an exists condition
    pub fn exists(key: impl Into<String>) -> Self {
        Condition::Exists { key: key.into() }
    }
    
    /// Create a matches condition
    pub fn matches(key: impl Into<String>, pattern: impl Into<String>) -> Self {
        Condition::Matches {
            key: key.into(),
            pattern: pattern.into(),
        }
    }
    
    /// Create a greater-than condition
    pub fn gt(key: impl Into<String>, value: f64) -> Self {
        Condition::GreaterThan {
            key: key.into(),
            value,
        }
    }
    
    /// Create a less-than condition
    pub fn lt(key: impl Into<String>, value: f64) -> Self {
        Condition::LessThan {
            key: key.into(),
            value,
        }
    }
    
    /// Create a contains condition
    pub fn contains(key: impl Into<String>, value: serde_json::Value) -> Self {
        Condition::Contains {
            key: key.into(),
            value,
        }
    }
    
    /// Combine with AND
    pub fn and(self, other: Condition) -> Self {
        match (self, other) {
            (Condition::All(mut left), Condition::All(right)) => {
                left.extend(right);
                Condition::All(left)
            }
            (Condition::All(mut left), right) => {
                left.push(right);
                Condition::All(left)
            }
            (left, Condition::All(right)) => {
                let mut all = vec![left];
                all.extend(right);
                Condition::All(all)
            }
            (left, right) => Condition::All(vec![left, right]),
        }
    }
    
    /// Combine with OR
    pub fn or(self, other: Condition) -> Self {
        match (self, other) {
            (Condition::Any(mut left), Condition::Any(right)) => {
                left.extend(right);
                Condition::Any(left)
            }
            (Condition::Any(mut left), right) => {
                left.push(right);
                Condition::Any(left)
            }
            (left, Condition::Any(right)) => {
                let mut any = vec![left];
                any.extend(right);
                Condition::Any(any)
            }
            (left, right) => Condition::Any(vec![left, right]),
        }
    }
    
    /// Negate this condition
    pub fn not(self) -> Self {
        Condition::Not(Box::new(self))
    }
    
    /// Evaluate the condition against workflow state
    pub fn evaluate(&self, state: &WorkflowState) -> bool {
        match self {
            Condition::Always => true,
            Condition::Never => false,
            
            Condition::Equals { key, value } => {
                state.get(key).map(|v| v == value).unwrap_or(false)
            }
            
            Condition::IsTrue { key } => {
                state.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
            }
            
            Condition::IsFalse { key } => {
                state.get(key).and_then(|v| v.as_bool()).map(|b| !b).unwrap_or(true)
            }
            
            Condition::Exists { key } => {
                state.contains(key)
            }
            
            Condition::Matches { key, pattern } => {
                if let Some(value) = state.get(key) {
                    if let Some(s) = value.as_str() {
                        if let Ok(re) = regex::Regex::new(pattern) {
                            return re.is_match(s);
                        }
                    }
                }
                false
            }
            
            Condition::GreaterThan { key, value } => {
                state.get(key)
                    .and_then(|v| v.as_f64())
                    .map(|v| v > *value)
                    .unwrap_or(false)
            }
            
            Condition::LessThan { key, value } => {
                state.get(key)
                    .and_then(|v| v.as_f64())
                    .map(|v| v < *value)
                    .unwrap_or(false)
            }
            
            Condition::Contains { key, value } => {
                if let Some(state_value) = state.get(key) {
                    // Check if array contains value
                    if let Some(arr) = state_value.as_array() {
                        return arr.contains(value);
                    }
                    // Check if string contains substring
                    if let (Some(s), Some(substr)) = (state_value.as_str(), value.as_str()) {
                        return s.contains(substr);
                    }
                }
                false
            }
            
            Condition::All(conditions) => {
                conditions.iter().all(|c| c.evaluate(state))
            }
            
            Condition::Any(conditions) => {
                conditions.iter().any(|c| c.evaluate(state))
            }
            
            Condition::Not(inner) => {
                !inner.evaluate(state)
            }
        }
    }
    
    /// Get a human-readable description of this condition
    pub fn description(&self) -> String {
        match self {
            Condition::Always => "always".to_string(),
            Condition::Never => "never".to_string(),
            Condition::Equals { key, value } => format!("{} == {}", key, value),
            Condition::IsTrue { key } => format!("{} is true", key),
            Condition::IsFalse { key } => format!("{} is false", key),
            Condition::Exists { key } => format!("{} exists", key),
            Condition::Matches { key, pattern } => format!("{} =~ /{}/", key, pattern),
            Condition::GreaterThan { key, value } => format!("{} > {}", key, value),
            Condition::LessThan { key, value } => format!("{} < {}", key, value),
            Condition::Contains { key, value } => format!("{} contains {}", key, value),
            Condition::All(conditions) => {
                let parts: Vec<_> = conditions.iter().map(|c| c.description()).collect();
                format!("({})", parts.join(" AND "))
            }
            Condition::Any(conditions) => {
                let parts: Vec<_> = conditions.iter().map(|c| c.description()).collect();
                format!("({})", parts.join(" OR "))
            }
            Condition::Not(inner) => format!("NOT ({})", inner.description()),
        }
    }
}

impl Default for Condition {
    fn default() -> Self {
        Condition::Always
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_condition_always() {
        let state = WorkflowState::new("test");
        assert!(Condition::always().evaluate(&state));
    }
    
    #[test]
    fn test_condition_never() {
        let state = WorkflowState::new("test");
        assert!(!Condition::never().evaluate(&state));
    }
    
    #[test]
    fn test_condition_equals() {
        let mut state = WorkflowState::new("test");
        state.set("status", json!("complete"));
        
        let cond = Condition::equals("status", json!("complete"));
        assert!(cond.evaluate(&state));
        
        let cond = Condition::equals("status", json!("pending"));
        assert!(!cond.evaluate(&state));
    }
    
    #[test]
    fn test_condition_is_true() {
        let mut state = WorkflowState::new("test");
        state.set("passed", json!(true));
        state.set("failed", json!(false));
        
        assert!(Condition::is_true("passed").evaluate(&state));
        assert!(!Condition::is_true("failed").evaluate(&state));
        assert!(!Condition::is_true("missing").evaluate(&state));
    }
    
    #[test]
    fn test_condition_and_or() {
        let mut state = WorkflowState::new("test");
        state.set("a", json!(true));
        state.set("b", json!(false));
        
        let and_cond = Condition::is_true("a").and(Condition::is_true("b"));
        assert!(!and_cond.evaluate(&state));
        
        let or_cond = Condition::is_true("a").or(Condition::is_true("b"));
        assert!(or_cond.evaluate(&state));
    }
    
    #[test]
    fn test_condition_not() {
        let mut state = WorkflowState::new("test");
        state.set("flag", json!(true));
        
        let cond = Condition::is_true("flag").not();
        assert!(!cond.evaluate(&state));
    }
    
    #[test]
    fn test_condition_greater_than() {
        let mut state = WorkflowState::new("test");
        state.set("count", json!(10));
        
        assert!(Condition::gt("count", 5.0).evaluate(&state));
        assert!(!Condition::gt("count", 15.0).evaluate(&state));
    }
    
    #[test]
    fn test_condition_contains() {
        let mut state = WorkflowState::new("test");
        state.set("tags", json!(["rust", "async", "tokio"]));
        state.set("message", json!("Hello, World!"));
        
        assert!(Condition::contains("tags", json!("rust")).evaluate(&state));
        assert!(!Condition::contains("tags", json!("python")).evaluate(&state));
        assert!(Condition::contains("message", json!("World")).evaluate(&state));
    }
}
