ALTER TABLE automations
ADD COLUMN thread_id TEXT;

CREATE INDEX idx_automations_thread
ON automations(thread_id);

CREATE TABLE thread_control_queue (
    queue_id TEXT PRIMARY KEY,
    target_thread_id TEXT NOT NULL,
    controller_json TEXT NOT NULL,
    text TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    dispatched_turn_id TEXT,
    error TEXT
);

CREATE INDEX idx_thread_control_queue_target_status_created
ON thread_control_queue(target_thread_id, status, created_at_ms);
