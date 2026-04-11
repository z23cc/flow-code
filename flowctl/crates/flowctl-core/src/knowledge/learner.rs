//! Learner — record, inject, compound, refresh.

use std::collections::HashMap;
use std::path::Path;

use crate::knowledge::types::*;
use crate::storage::knowledge_store::KnowledgeStore;

/// The Learner manages the three-layer knowledge pyramid.
pub struct Learner {
    store: KnowledgeStore,
}

impl Learner {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            store: KnowledgeStore::new(flow_root),
        }
    }

    /// Record a learning from completed or failed work.
    pub fn record(
        &self,
        goal_id: &str,
        node_id: Option<&str>,
        kind: LearningKind,
        content: &str,
        tags: Vec<String>,
    ) -> Result<Learning, String> {
        let mut learning = Learning::new(goal_id, node_id, kind, content);
        learning.tags = tags;
        self.store.add_learning(&learning)?;
        Ok(learning)
    }

    /// Inject relevant patterns for a node's objective.
    pub fn inject_for_node(&self, objective: &str, limit: usize) -> Result<Vec<Pattern>, String> {
        let patterns = self.store.list_patterns()?;
        let obj_lower = objective.to_lowercase();

        let mut scored: Vec<(f64, Pattern)> = patterns
            .into_iter()
            .map(|p| {
                let name_match = if obj_lower.contains(&p.name.to_lowercase()) { 2.0 } else { 0.0 };
                let desc_match = if obj_lower.contains(&p.description.to_lowercase().split_whitespace().next().unwrap_or("")) {
                    1.0
                } else {
                    0.0
                };
                let score = (name_match + desc_match) * p.confidence;
                (score, p)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, p)| p).collect())
    }

    /// Compound learnings into patterns after goal completion.
    /// Groups learnings by tags, promotes clusters of 3+ into patterns.
    pub fn compound(&self, goal_id: &str) -> Result<Vec<Pattern>, String> {
        let learnings = self.store.list_learnings()?;
        let goal_learnings: Vec<&Learning> = learnings.iter()
            .filter(|l| l.goal_id == goal_id)
            .collect();

        // Group by tags
        let mut tag_groups: HashMap<String, Vec<&Learning>> = HashMap::new();
        for learning in &goal_learnings {
            for tag in &learning.tags {
                tag_groups.entry(tag.clone()).or_default().push(learning);
            }
        }

        let mut new_patterns = Vec::new();
        let existing = self.store.list_patterns()?;

        for (tag, group) in &tag_groups {
            if group.len() < 3 {
                continue;
            }

            // Check if a pattern with this name already exists
            let existing_pattern = existing.iter().find(|p| p.name == *tag);

            if let Some(ep) = existing_pattern {
                // Boost confidence of existing pattern
                let mut updated = ep.clone();
                updated.confidence = (updated.confidence + 0.1).min(1.0);
                updated.freshness = chrono::Utc::now();
                updated.use_count += 1;
                self.store.update_pattern(&updated)?;
            } else {
                // Create new pattern
                let successes: Vec<&&Learning> = group.iter().filter(|l| l.kind == LearningKind::Success).collect();
                let approach = if successes.is_empty() {
                    format!("Avoid: {}", group[0].content)
                } else {
                    successes[0].content.clone()
                };

                let mut pattern = Pattern::new(tag, &format!("Pattern from {tag}"), &approach);
                pattern.source_learnings = group.iter().map(|l| l.id.clone()).collect();
                pattern.confidence = 0.6;
                self.store.add_pattern(&pattern)?;
                new_patterns.push(pattern);
            }
        }

        Ok(new_patterns)
    }

    /// Refresh stale patterns — decay confidence on old, unverified patterns.
    pub fn refresh_stale(&self) -> Result<u32, String> {
        let patterns = self.store.list_patterns()?;
        let mut decayed_count = 0;

        for mut pattern in patterns {
            if pattern.is_stale() {
                pattern.decay();
                self.store.update_pattern(&pattern)?;
                decayed_count += 1;
            }
        }

        Ok(decayed_count)
    }

    /// Search across all knowledge layers using word-overlap scoring.
    /// Scores each item by counting how many query words appear in the text.
    /// Higher score = more relevant. Items with score 0 are filtered out.
    pub fn search(&self, query: &str, limit: usize) -> Result<KnowledgeResult, String> {
        let query_words: Vec<String> = query.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .map(String::from)
            .collect();

        // Score patterns: check name + description + approach
        let mut scored_patterns: Vec<(f64, Pattern)> = self.store.list_patterns()?
            .into_iter()
            .map(|p| {
                let text = format!("{} {} {}", p.name, p.description, p.approach).to_lowercase();
                let word_hits = query_words.iter().filter(|w| text.contains(w.as_str())).count();
                let score = word_hits as f64 * p.confidence;
                (score, p)
            })
            .filter(|(s, _)| *s > 0.0)
            .collect();
        scored_patterns.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored_patterns.truncate(limit);
        let patterns: Vec<Pattern> = scored_patterns.into_iter().map(|(_, p)| p).collect();

        // Score learnings
        let mut scored_learnings: Vec<(usize, Learning)> = self.store.list_learnings()?
            .into_iter()
            .map(|l| {
                let text = l.content.to_lowercase();
                let hits = query_words.iter().filter(|w| text.contains(w.as_str())).count();
                (hits, l)
            })
            .filter(|(h, _)| *h > 0)
            .collect();
        scored_learnings.sort_by(|a, b| b.0.cmp(&a.0));
        scored_learnings.truncate(limit);
        let learnings: Vec<Learning> = scored_learnings.into_iter().map(|(_, l)| l).collect();

        // Score rules
        let methodology = self.store.get_methodology()?;
        let rules: Vec<MethodRule> = methodology.rules
            .into_iter()
            .filter(|r| {
                let text = r.rule.to_lowercase();
                query_words.iter().any(|w| text.contains(w.as_str()))
            })
            .collect();

        Ok(KnowledgeResult { patterns, learnings, rules })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Learner) {
        let tmp = TempDir::new().unwrap();
        let learner = Learner::new(tmp.path());
        (tmp, learner)
    }

    #[test]
    fn test_record() {
        let (_tmp, learner) = setup();
        let l = learner.record("g-1", Some("n-1"), LearningKind::Success, "test", vec!["tag1".into()]).unwrap();
        assert!(l.id.starts_with("l-"));
        assert_eq!(l.tags, vec!["tag1"]);
    }

    #[test]
    fn test_compound_creates_pattern() {
        let (_tmp, learner) = setup();
        // Record 3+ learnings with same tag
        for i in 0..4 {
            learner.record("g-1", None, LearningKind::Success, &format!("learning {i}"), vec!["auth".into()]).unwrap();
        }
        let patterns = learner.compound("g-1").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].name, "auth");
    }

    #[test]
    fn test_search() {
        let (_tmp, learner) = setup();
        learner.record("g-1", None, LearningKind::Discovery, "OAuth token refresh issue", vec![]).unwrap();
        let result = learner.search("OAuth", 5).unwrap();
        assert_eq!(result.learnings.len(), 1);
    }

    #[test]
    fn test_refresh_stale() {
        let (_tmp, learner) = setup();
        let mut p = Pattern::new("old", "old pattern", "do something");
        p.freshness = chrono::Utc::now() - chrono::Duration::days(100);
        p.confidence = 1.0;
        learner.store.add_pattern(&p).unwrap();

        let decayed = learner.refresh_stale().unwrap();
        assert_eq!(decayed, 1);

        let patterns = learner.store.list_patterns().unwrap();
        assert!(patterns[0].confidence < 1.0);
    }
}
