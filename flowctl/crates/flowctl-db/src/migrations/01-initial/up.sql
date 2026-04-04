-- flowctl initial schema
-- Indexed from Markdown frontmatter (rebuildable via reindex)

CREATE TABLE epics (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open',
    branch_name TEXT,
    plan_review TEXT DEFAULT 'unknown',
    file_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    epic_id     TEXT NOT NULL REFERENCES epics(id),
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'todo',
    priority    INTEGER DEFAULT 999,
    domain      TEXT DEFAULT 'general',
    file_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE task_deps (
    task_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE epic_deps (
    epic_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (epic_id, depends_on)
);

CREATE TABLE file_ownership (
    file_path   TEXT NOT NULL,
    task_id     TEXT NOT NULL,
    PRIMARY KEY (file_path, task_id)
);

-- Runtime-only data (not in Markdown, not rebuildable)

CREATE TABLE runtime_state (
    task_id       TEXT PRIMARY KEY,
    assignee      TEXT,
    claimed_at    TEXT,
    completed_at  TEXT,
    duration_secs INTEGER,
    blocked_reason TEXT,
    baseline_rev  TEXT,
    final_rev     TEXT
);

CREATE TABLE file_locks (
    file_path   TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL,
    locked_at   TEXT NOT NULL
);

CREATE TABLE heartbeats (
    task_id     TEXT PRIMARY KEY,
    last_beat   TEXT NOT NULL,
    worker_pid  INTEGER
);

CREATE TABLE phase_progress (
    task_id     TEXT NOT NULL,
    phase       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    completed_at TEXT,
    PRIMARY KEY (task_id, phase)
);

CREATE TABLE evidence (
    task_id       TEXT PRIMARY KEY,
    commits       TEXT,
    tests         TEXT,
    files_changed INTEGER,
    insertions    INTEGER,
    deletions     INTEGER,
    review_iters  INTEGER
);

-- Event log + metrics (append-only, runtime-only)

CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    epic_id     TEXT NOT NULL,
    task_id     TEXT,
    event_type  TEXT NOT NULL,
    actor       TEXT,
    payload     TEXT,
    session_id  TEXT
);

CREATE TABLE token_usage (
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

CREATE TABLE daily_rollup (
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

CREATE TABLE monthly_rollup (
    month            TEXT PRIMARY KEY,
    epics_completed  INTEGER DEFAULT 0,
    tasks_completed  INTEGER DEFAULT 0,
    avg_lead_time_h  REAL DEFAULT 0,
    total_tokens     INTEGER DEFAULT 0,
    total_cost_usd   REAL DEFAULT 0
);

-- Indexes

CREATE INDEX idx_tasks_epic ON tasks(epic_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_events_entity ON events(epic_id, task_id);
CREATE INDEX idx_events_ts ON events(timestamp);
CREATE INDEX idx_events_type ON events(event_type, timestamp);
CREATE INDEX idx_token_epic ON token_usage(epic_id);

-- Auto-aggregation trigger: roll up task events into daily_rollup

CREATE TRIGGER trg_daily_rollup AFTER INSERT ON events
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
