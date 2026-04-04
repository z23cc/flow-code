-- SQLite doesn't support DROP COLUMN before 3.35.0; this is best-effort.
-- For older SQLite, a full table rebuild would be needed.
ALTER TABLE runtime_state DROP COLUMN retry_count;
