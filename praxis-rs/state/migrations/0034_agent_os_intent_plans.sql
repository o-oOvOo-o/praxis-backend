CREATE TABLE IF NOT EXISTS agent_os_intent_plans (
    plan_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

