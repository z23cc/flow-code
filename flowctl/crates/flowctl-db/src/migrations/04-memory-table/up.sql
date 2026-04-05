-- Memory table with structured schema (CE-inspired).
-- Stores memory entries with module/severity/problem_type classification.
-- Backward compatible: all new columns are optional (nullable).

CREATE TABLE IF NOT EXISTS memory (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_type    TEXT NOT NULL,           -- pitfall, convention, decision
    content       TEXT NOT NULL,
    summary       TEXT,
    hash          TEXT UNIQUE,             -- SHA256 prefix for dedup
    module        TEXT,                    -- e.g. "flowctl-core", "scheduler"
    severity      TEXT,                    -- critical, high, medium, low
    problem_type  TEXT,                    -- build_error, test_failure, best_practice, etc.
    component     TEXT,                    -- optional sub-module
    tags          TEXT DEFAULT '[]',       -- JSON array
    track         TEXT,                    -- auto-derived: "bug" or "knowledge"
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    last_verified TEXT,
    refs          INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_memory_type ON memory(entry_type);
CREATE INDEX idx_memory_module ON memory(module);
CREATE INDEX idx_memory_track ON memory(track);
CREATE INDEX idx_memory_severity ON memory(severity);
