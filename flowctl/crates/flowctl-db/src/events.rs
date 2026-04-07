//! Event store — delegates to `json_store::events_*`.

use std::path::Path;

use crate::error::DbError;

/// Sync event store backed by `.state/events.jsonl`.
pub struct EventStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> EventStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Append a JSON event line to the event log.
    pub fn append(&self, event_json: &str) -> Result<(), DbError> {
        flowctl_core::json_store::events_append(self.flow_dir, event_json)?;
        Ok(())
    }

    /// Read all event lines from the log.
    pub fn read_all(&self) -> Result<Vec<String>, DbError> {
        let lines = flowctl_core::json_store::events_read_all(self.flow_dir)?;
        Ok(lines)
    }

    /// Read events filtered by stream_id.
    pub fn read_by_stream(&self, stream_id: &str) -> Result<Vec<String>, DbError> {
        let lines = flowctl_core::json_store::events_read_by_stream(self.flow_dir, stream_id)?;
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_and_read() {
        let tmp = TempDir::new().unwrap();
        let store = EventStore::new(tmp.path());

        store.append(r#"{"stream_id":"s1","type":"created"}"#).unwrap();
        store.append(r#"{"stream_id":"s2","type":"updated"}"#).unwrap();
        store.append(r#"{"stream_id":"s1","type":"done"}"#).unwrap();

        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 3);

        let s1 = store.read_by_stream("s1").unwrap();
        assert_eq!(s1.len(), 2);
    }

    #[test]
    fn empty_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let store = EventStore::new(tmp.path());
        assert!(store.read_all().unwrap().is_empty());
    }
}
