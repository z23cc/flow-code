-- SQLite doesn't support DROP COLUMN before 3.35.0, but rusqlite bundles 3.45+.
ALTER TABLE epics DROP COLUMN body;
ALTER TABLE tasks DROP COLUMN body;
