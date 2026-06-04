CREATE TABLE thread_goals (
    thread_id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    objective TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'active',
        'paused',
        'blocked',
        'usage_limited',
        'budget_limited',
        'complete'
    )),
    token_budget INTEGER,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    time_used_seconds INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

CREATE INDEX idx_thread_goals_status ON thread_goals(status);

CREATE TABLE thread_heartbeats (
    thread_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0,
    interval_ms INTEGER NOT NULL,
    next_wake_at_ms INTEGER NOT NULL,
    last_wake_at_ms INTEGER,
    controller TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

CREATE INDEX idx_thread_heartbeats_due
ON thread_heartbeats(enabled, next_wake_at_ms);
