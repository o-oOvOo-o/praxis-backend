ALTER TABLE automation_runs
ADD COLUMN turn_id TEXT;

CREATE INDEX idx_automation_runs_thread_turn
ON automation_runs(thread_id, turn_id);
