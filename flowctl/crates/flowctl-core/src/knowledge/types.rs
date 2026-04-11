//! Knowledge types — Learning, Pattern, Methodology.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Layer 1: Learning — atomic experience from a single execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    pub id: String,
    pub goal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub kind: LearningKind,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub use_count: u32,
}

/// Kind of learning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningKind {
    Success,
    Failure,
    Discovery,
    Pitfall,
}

/// Layer 2: Pattern — distilled knowledge from multiple learnings.
/// Has confidence score and decay lifecycle (Compound Engineering pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub description: String,
    pub approach: String,
    #[serde(default)]
    pub anti_patterns: Vec<String>,
    #[serde(default)]
    pub source_learnings: Vec<String>,
    pub confidence: f64,
    pub freshness: DateTime<Utc>,
    #[serde(default = "default_decay_days")]
    pub decay_days: u32,
    #[serde(default)]
    pub use_count: u32,
}

fn default_decay_days() -> u32 {
    90
}

/// Layer 3: Methodology — core rules (arscontexta pattern: agent-modifiable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Methodology {
    #[serde(default)]
    pub rules: Vec<MethodRule>,
    pub last_revised: DateTime<Utc>,
    #[serde(default)]
    pub revision_trigger: String,
}

/// A single methodology rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodRule {
    pub id: String,
    pub rule: String,
    pub rationale: String,
    #[serde(default)]
    pub active: bool,
}

/// Combined search result across all three layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeResult {
    pub patterns: Vec<Pattern>,
    pub learnings: Vec<Learning>,
    pub rules: Vec<MethodRule>,
}

impl Learning {
    pub fn new(goal_id: &str, node_id: Option<&str>, kind: LearningKind, content: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            id: format!("l-{}-{seq}", Utc::now().timestamp_millis()),
            goal_id: goal_id.to_string(),
            node_id: node_id.map(String::from),
            kind,
            content: content.to_string(),
            tags: Vec::new(),
            created_at: Utc::now(),
            verified: false,
            use_count: 0,
        }
    }
}

impl Pattern {
    pub fn new(name: &str, description: &str, approach: &str) -> Self {
        Self {
            id: format!("p-{}", Utc::now().timestamp_millis()),
            name: name.to_string(),
            description: description.to_string(),
            approach: approach.to_string(),
            anti_patterns: Vec::new(),
            source_learnings: Vec::new(),
            confidence: 0.5,
            freshness: Utc::now(),
            decay_days: default_decay_days(),
            use_count: 0,
        }
    }

    /// Check if this pattern has decayed past threshold.
    pub fn is_stale(&self) -> bool {
        let age = Utc::now() - self.freshness;
        age.num_days() > self.decay_days as i64
    }

    /// Apply decay to confidence.
    pub fn decay(&mut self) {
        if self.is_stale() {
            self.confidence *= 0.8;
        }
    }
}

impl Default for Methodology {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            last_revised: Utc::now(),
            revision_trigger: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learning_new() {
        let l = Learning::new("g-1", Some("n-1"), LearningKind::Success, "test");
        assert!(l.id.starts_with("l-"));
        assert_eq!(l.goal_id, "g-1");
        assert_eq!(l.kind, LearningKind::Success);
        assert!(!l.verified);
    }

    #[test]
    fn test_pattern_decay() {
        let mut p = Pattern::new("test", "desc", "approach");
        p.freshness = Utc::now() - chrono::Duration::days(100);
        p.confidence = 1.0;
        assert!(p.is_stale());
        p.decay();
        assert!((p.confidence - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_pattern_not_stale() {
        let p = Pattern::new("test", "desc", "approach");
        assert!(!p.is_stale());
    }
}
