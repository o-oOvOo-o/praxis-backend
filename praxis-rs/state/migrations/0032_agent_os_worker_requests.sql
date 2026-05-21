CREATE TABLE IF NOT EXISTS agent_os_worker_requests (
    request_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

