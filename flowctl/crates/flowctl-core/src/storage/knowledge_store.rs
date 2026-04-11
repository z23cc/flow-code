//! KnowledgeStore — CRUD for learnings, patterns, and methodology rules.
//!
//! Layout: .flow/knowledge/learnings/*.json, patterns/*.json, rules/methodology.json

use std::fs;
use std::path::{Path, PathBuf};

use crate::knowledge::{Learning, Methodology, Pattern};

/// Store for the three-layer knowledge pyramid.
pub struct KnowledgeStore {
    root: PathBuf,
}

impl KnowledgeStore {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            root: flow_root.join("knowledge"),
        }
    }

    fn learnings_dir(&self) -> PathBuf { self.root.join("learnings") }
    fn patterns_dir(&self) -> PathBuf { self.root.join("patterns") }
    fn rules_dir(&self) -> PathBuf { self.root.join("rules") }

    // ── Learnings ───────────────────────────────────────────────────

    pub fn add_learning(&self, learning: &Learning) -> Result<(), String> {
        let dir = self.learnings_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("create learnings dir: {e}"))?;
        let path = dir.join(format!("{}.json", learning.id));
        let json = serde_json::to_string_pretty(learning).map_err(|e| format!("serialize: {e}"))?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &json).map_err(|e| format!("write: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))
    }

    pub fn list_learnings(&self) -> Result<Vec<Learning>, String> {
        read_all_json(&self.learnings_dir())
    }

    pub fn get_learning(&self, id: &str) -> Result<Learning, String> {
        let path = self.learnings_dir().join(format!("{id}.json"));
        read_json(&path)
    }

    // ── Patterns ────────────────────────────────────────────────────

    pub fn add_pattern(&self, pattern: &Pattern) -> Result<(), String> {
        let dir = self.patterns_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("create patterns dir: {e}"))?;
        let path = dir.join(format!("{}.json", pattern.id));
        let json = serde_json::to_string_pretty(pattern).map_err(|e| format!("serialize: {e}"))?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &json).map_err(|e| format!("write: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))
    }

    pub fn update_pattern(&self, pattern: &Pattern) -> Result<(), String> {
        self.add_pattern(pattern) // Idempotent — same file path
    }

    pub fn list_patterns(&self) -> Result<Vec<Pattern>, String> {
        read_all_json(&self.patterns_dir())
    }

    // ── Methodology ─────────────────────────────────────────────────

    pub fn get_methodology(&self) -> Result<Methodology, String> {
        let path = self.rules_dir().join("methodology.json");
        if !path.exists() {
            return Ok(Methodology::default());
        }
        read_json(&path)
    }

    pub fn save_methodology(&self, methodology: &Methodology) -> Result<(), String> {
        let dir = self.rules_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("create rules dir: {e}"))?;
        let path = dir.join("methodology.json");
        let json = serde_json::to_string_pretty(methodology).map_err(|e| format!("serialize: {e}"))?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &json).map_err(|e| format!("write: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_all_json<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<Vec<T>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("read dir: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .map(|e| e.path())
        .collect();
    paths.sort();
    for path in paths {
        let item: T = read_json(&path)?;
        items.push(item);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{Learning, LearningKind, Pattern};
    use tempfile::TempDir;

    fn setup() -> (TempDir, KnowledgeStore) {
        let tmp = TempDir::new().unwrap();
        let store = KnowledgeStore::new(tmp.path());
        (tmp, store)
    }

    #[test]
    fn test_add_and_list_learnings() {
        let (_tmp, store) = setup();
        let l1 = Learning::new("g-1", Some("n-1"), LearningKind::Success, "learned something");
        let l2 = Learning::new("g-1", None, LearningKind::Discovery, "found something");
        store.add_learning(&l1).unwrap();
        store.add_learning(&l2).unwrap();
        let all = store.list_learnings().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_add_and_list_patterns() {
        let (_tmp, store) = setup();
        let p = Pattern::new("test pattern", "description", "do this");
        store.add_pattern(&p).unwrap();
        let all = store.list_patterns().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "test pattern");
    }

    #[test]
    fn test_methodology_default() {
        let (_tmp, store) = setup();
        let m = store.get_methodology().unwrap();
        assert!(m.rules.is_empty());
    }

    #[test]
    fn test_save_and_get_methodology() {
        let (_tmp, store) = setup();
        let mut m = Methodology::default();
        m.rules.push(crate::knowledge::MethodRule {
            id: "r-1".into(),
            rule: "always test".into(),
            rationale: "quality".into(),
            active: true,
        });
        store.save_methodology(&m).unwrap();
        let loaded = store.get_methodology().unwrap();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].rule, "always test");
    }
}
