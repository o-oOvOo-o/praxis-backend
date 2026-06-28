CREATE TABLE automations (
    automation_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    kind TEXT NOT NULL,
    prompt TEXT NOT NULL,
    schedule_json TEXT NOT NULL,
    config_json TEXT NOT NULL,
    next_run_at_ms INTEGER,
    last_run_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX idx_automations_enabled_due
ON automations(enabled, next_run_at_ms);

CREATE TABLE automation_runs (
    run_id TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL,
    status TEXT NOT NULL,
    trigger_kind TEXT NOT NULL,
    thread_id TEXT,
    turn_id TEXT,
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    error TEXT,
    metadata_json TEXT NOT NULL,
    FOREIGN KEY (automation_id) REFERENCES automations(automation_id) ON DELETE CASCADE
);

CREATE INDEX idx_automation_runs_automation_started
ON automation_runs(automation_id, started_at_ms DESC);

CREATE INDEX idx_automation_runs_thread_turn
ON automation_runs(thread_id, turn_id);
