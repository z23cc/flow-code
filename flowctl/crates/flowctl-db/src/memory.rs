//! Memory store — delegates to `json_store::memory_*`.

use std::path::Path;

use crate::error::DbError;

/// Sync memory store backed by `memory/entries.jsonl`.
pub struct MemoryStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> MemoryStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Append a JSON memory entry.
    pub fn append(&self, entry_json: &str) -> Result<(), DbError> {
        flowctl_core::json_store::memory_append(self.flow_dir, entry_json)?;
        Ok(())
    }

    /// Read all memory entries.
    pub fn read_all(&self) -> Result<Vec<String>, DbError> {
        let entries = flowctl_core::json_store::memory_read_all(self.flow_dir)?;
        Ok(entries)
    }

    /// Search memory entries by case-insensitive substring match.
    pub fn search(&self, query: &str) -> Result<Vec<String>, DbError> {
        let results = flowctl_core::json_store::memory_search_text(self.flow_dir, query)?;
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_and_read() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::new(tmp.path());

        store.append(r#"{"text":"Rust is great"}"#).unwrap();
        store.append(r#"{"text":"Python is also nice"}"#).unwrap();

        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn search_text() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::new(tmp.path());

        store.append(r#"{"text":"Rust is great"}"#).unwrap();
        store.append(r#"{"text":"Python is also nice"}"#).unwrap();
        store.append(r#"{"text":"rust patterns"}"#).unwrap();

        let found = store.search("rust").unwrap();
        assert_eq!(found.len(), 2);

        let none = store.search("javascript").unwrap();
        assert!(none.is_empty());
    }
}
