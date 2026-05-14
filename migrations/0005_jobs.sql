-- Migration 0005: jobs table for durable background work
CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    queue TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    params TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','running','done','failed','cancelled')),
    priority INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    last_error TEXT,
    parent_job_id INTEGER REFERENCES jobs(id),
    depth INTEGER NOT NULL DEFAULT 0,
    result TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    heartbeat_at TEXT,
    idempotency_key TEXT UNIQUE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_jobs_pending ON jobs(queue, priority DESC, id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_jobs_running ON jobs(heartbeat_at) WHERE status = 'running';
CREATE INDEX IF NOT EXISTS idx_jobs_parent ON jobs(parent_job_id);
