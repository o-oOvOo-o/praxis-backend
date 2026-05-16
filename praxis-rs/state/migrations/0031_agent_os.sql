CREATE TABLE IF NOT EXISTS agent_os_threads (
    thread_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_tasks (
    task_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_leases (
    lease_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_tickets (
    ticket_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_commands (
    command_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_runtime_commands (
    command_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_artifacts (
    artifact_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_os_events (
    event_id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    thread_id TEXT,
    task_id TEXT,
    command_id TEXT,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_os_events_thread_created
    ON agent_os_events(thread_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_os_events_task_created
    ON agent_os_events(task_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_os_events_command_created
    ON agent_os_events(command_id, created_at DESC);
