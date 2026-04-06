-- flowctl libSQL schema (fresh, no migrations).
-- Consolidates migrations 01-04 plus adds native vector column on memory.
-- Applied once on DB open via pool::apply_schema().

-- ── Indexed from Markdown (rebuildable via reindex) ─────────────────

CREATE TABLE IF NOT EXISTS epics (
    id                    TEXT PRIMARY KEY,
    title                 TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'open',
    branch_name           TEXT,
    plan_review           TEXT DEFAULT 'unknown',
    auto_execute_pending  INTEGER DEFAULT 0,
    auto_execute_set_at   TEXT,
    archived              INTEGER DEFAULT 0,
    file_path             TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    body                  TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS tasks (
    id          TEXT PRIMARY KEY,
    epic_id     TEXT NOT NULL REFERENCES epics(id),
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'todo',
    priority    INTEGER DEFAULT 999,
    domain      TEXT DEFAULT 'general',
    file_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS task_deps (
    task_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE IF NOT EXISTS epic_deps (
    epic_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (epic_id, depends_on)
);

CREATE TABLE IF NOT EXISTS file_ownership (
    file_path   TEXT NOT NULL,
    task_id     TEXT NOT NULL,
    PRIMARY KEY (file_path, task_id)
);

-- ── Gaps registry (replaces epics/{id}.gaps.json sidecar) ─────────────

CREATE TABLE IF NOT EXISTS gaps (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    epic_id     TEXT NOT NULL,
    capability  TEXT NOT NULL,
    priority    TEXT NOT NULL DEFAULT 'important',
    source      TEXT,
    status      TEXT NOT NULL DEFAULT 'open',
    resolved_at TEXT,
    evidence    TEXT,
    task_id     TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (epic_id) REFERENCES epics(id)
);

-- ── Runtime-only (not in Markdown, not rebuildable) ─────────────────

CREATE TABLE IF NOT EXISTS runtime_state (
    task_id        TEXT PRIMARY KEY,
    assignee       TEXT,
    claimed_at     TEXT,
    completed_at   TEXT,
    duration_secs  INTEGER,
    blocked_reason TEXT,
    baseline_rev   TEXT,
    final_rev      TEXT,
    retry_count    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS file_locks (
    file_path  TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL,
    locked_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS heartbeats (
    task_id    TEXT PRIMARY KEY,
    last_beat  TEXT NOT NULL,
    worker_pid INTEGER
);

CREATE TABLE IF NOT EXISTS phase_progress (
    task_id      TEXT NOT NULL,
    phase        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    completed_at TEXT,
    PRIMARY KEY (task_id, phase)
);

CREATE TABLE IF NOT EXISTS evidence (
    task_id       TEXT PRIMARY KEY,
    commits       TEXT,
    tests         TEXT,
    files_changed INTEGER,
    insertions    INTEGER,
    deletions     INTEGER,
    review_iters  INTEGER
);

-- ── Event log + metrics (append-only) ───────────────────────────────

CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    epic_id     TEXT NOT NULL,
    task_id     TEXT,
    event_type  TEXT NOT NULL,
    actor       TEXT,
    payload     TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS token_usage (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    epic_id         TEXT NOT NULL,
    task_id         TEXT,
    phase           TEXT,
    model           TEXT,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    cache_read      INTEGER DEFAULT 0,
    cache_write     INTEGER DEFAULT 0,
    estimated_cost  REAL
);

CREATE TABLE IF NOT EXISTS daily_rollup (
    day              TEXT NOT NULL,
    epic_id          TEXT,
    tasks_started    INTEGER DEFAULT 0,
    tasks_completed  INTEGER DEFAULT 0,
    tasks_failed     INTEGER DEFAULT 0,
    total_duration_s INTEGER DEFAULT 0,
    input_tokens     INTEGER DEFAULT 0,
    output_tokens    INTEGER DEFAULT 0,
    PRIMARY KEY (day, epic_id)
);

CREATE TABLE IF NOT EXISTS monthly_rollup (
    month            TEXT PRIMARY KEY,
    epics_completed  INTEGER DEFAULT 0,
    tasks_completed  INTEGER DEFAULT 0,
    avg_lead_time_h  REAL DEFAULT 0,
    total_tokens     INTEGER DEFAULT 0,
    total_cost_usd   REAL DEFAULT 0
);

-- ── Approvals (replaces stdin-blocking Teams protocol) ─────────────

CREATE TABLE IF NOT EXISTS approvals (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL,
    kind         TEXT NOT NULL,                    -- file_access | mutation | generic
    payload      TEXT NOT NULL,                    -- JSON
    status       TEXT NOT NULL DEFAULT 'pending',  -- pending | approved | rejected
    created_at   INTEGER NOT NULL,
    resolved_at  INTEGER,
    resolver     TEXT,
    reason       TEXT
);

CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);
CREATE INDEX IF NOT EXISTS idx_approvals_task ON approvals(task_id);

-- ── Memory with native vector embedding (BGE-small, 384-dim) ────────

CREATE TABLE IF NOT EXISTS memory (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_type    TEXT NOT NULL,
    content       TEXT NOT NULL,
    summary       TEXT,
    hash          TEXT UNIQUE,
    module        TEXT,
    severity      TEXT,
    problem_type  TEXT,
    component     TEXT,
    tags          TEXT DEFAULT '[]',
    track         TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    last_verified TEXT,
    refs          INTEGER NOT NULL DEFAULT 0,
    embedding     F32_BLOB(384)
);

-- ── Indexes ─────────────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_gaps_epic ON gaps(epic_id);
CREATE INDEX IF NOT EXISTS idx_gaps_status ON gaps(status);
CREATE INDEX IF NOT EXISTS idx_tasks_epic ON tasks(epic_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_events_entity ON events(epic_id, task_id);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type, timestamp);
CREATE INDEX IF NOT EXISTS idx_token_epic ON token_usage(epic_id);
CREATE INDEX IF NOT EXISTS idx_memory_type ON memory(entry_type);
CREATE INDEX IF NOT EXISTS idx_memory_module ON memory(module);
CREATE INDEX IF NOT EXISTS idx_memory_track ON memory(track);
CREATE INDEX IF NOT EXISTS idx_memory_severity ON memory(severity);

-- Native libSQL vector index for semantic memory search
CREATE INDEX IF NOT EXISTS memory_emb_idx ON memory(libsql_vector_idx(embedding));

-- ── Auto-aggregation trigger ────────────────────────────────────────

CREATE TRIGGER IF NOT EXISTS trg_daily_rollup AFTER INSERT ON events
WHEN NEW.event_type IN ('task_completed', 'task_failed', 'task_started')
BEGIN
    INSERT INTO daily_rollup (day, epic_id, tasks_completed, tasks_failed, tasks_started)
    VALUES (DATE(NEW.timestamp), NEW.epic_id,
            CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
            CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
            CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END)
    ON CONFLICT(day, epic_id) DO UPDATE SET
        tasks_completed = tasks_completed +
            CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
        tasks_failed = tasks_failed +
            CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
        tasks_started = tasks_started +
            CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END;
END;
