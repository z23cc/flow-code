//! Frecency scoring for file access patterns.
//!
//! Stores exponential-decay scores in `.flow/frecency.json`.
//! Algorithm: `new_score = old_score * 0.5^(days_elapsed / 14.0) + weight`
//!
//! Weights:
//! - `git_modified` = 3.0
//! - `recently_opened` = 2.0
//! - `normal_access` = 1.0

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Half-life in days for the exponential decay.
const HALF_LIFE_DAYS: f64 = 14.0;

/// A single file's frecency record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrecencyEntry {
    pub score: f64,
    pub last_access: DateTime<Utc>,
}

/// In-memory frecency store backed by `.flow/frecency.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrecencyStore {
    entries: HashMap<String, FrecencyEntry>,
}

impl FrecencyStore {
    /// Load from `.flow/frecency.json`. Returns empty store if file missing or invalid.
    pub fn load(flow_dir: &Path) -> Self {
        let path = flow_dir.join("frecency.json");
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist to `.flow/frecency.json`.
    pub fn save(&self, flow_dir: &Path) {
        let path = flow_dir.join("frecency.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Record an access with the given weight. Applies decay before adding.
    pub fn record_access(&mut self, path: &str, weight: f64) {
        let now = Utc::now();
        let entry = self.entries.entry(path.to_string()).or_insert(FrecencyEntry {
            score: 0.0,
            last_access: now,
        });
        let decayed = decay(entry.score, entry.last_access, now);
        entry.score = decayed + weight;
        entry.last_access = now;
    }

    /// Get the current score for a path (with decay applied to now).
    pub fn get_score(&self, path: &str) -> f64 {
        match self.entries.get(path) {
            Some(entry) => decay(entry.score, entry.last_access, Utc::now()),
            None => 0.0,
        }
    }

    /// Return the top `limit` files sorted by decayed score descending.
    pub fn top_files(&self, limit: usize) -> Vec<(&str, f64)> {
        let now = Utc::now();
        let mut scored: Vec<(&str, f64)> = self
            .entries
            .iter()
            .map(|(path, entry)| (path.as_str(), decay(entry.score, entry.last_access, now)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }
}

/// Apply exponential decay: `score * 0.5^(elapsed_days / HALF_LIFE_DAYS)`.
fn decay(score: f64, last_access: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let elapsed = now.signed_duration_since(last_access);
    let days = elapsed.num_seconds() as f64 / 86400.0;
    if days <= 0.0 {
        return score;
    }
    score * (0.5_f64).powf(days / HALF_LIFE_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_get_score() {
        let mut store = FrecencyStore::default();
        store.record_access("src/main.rs", 1.0);
        let score = store.get_score("src/main.rs");
        assert!(score > 0.9, "score should be close to 1.0, got {score}");
    }

    #[test]
    fn unknown_file_returns_zero() {
        let store = FrecencyStore::default();
        assert_eq!(store.get_score("nonexistent.rs"), 0.0);
    }

    #[test]
    fn top_files_sorted() {
        let mut store = FrecencyStore::default();
        store.record_access("a.rs", 1.0);
        store.record_access("b.rs", 5.0);
        store.record_access("c.rs", 3.0);
        let top = store.top_files(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "b.rs");
        assert_eq!(top[1].0, "c.rs");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FrecencyStore::default();
        store.record_access("foo.rs", 2.0);
        store.save(dir.path());

        let loaded = FrecencyStore::load(dir.path());
        let score = loaded.get_score("foo.rs");
        assert!(score > 1.9, "roundtrip score should be close to 2.0, got {score}");
    }
}
