ALTER TABLE threads ADD COLUMN source_kind TEXT;
ALTER TABLE threads ADD COLUMN subagent_kind TEXT;
ALTER TABLE threads ADD COLUMN subagent_parent_thread_id TEXT;
ALTER TABLE threads ADD COLUMN subagent_depth INTEGER;
ALTER TABLE threads ADD COLUMN subagent_agent_nickname TEXT;

CREATE INDEX idx_threads_source_kind_updated_at
    ON threads(source_kind, archived, updated_at DESC, id DESC);

CREATE INDEX idx_threads_source_kind_created_at
    ON threads(source_kind, archived, created_at DESC, id DESC);

CREATE INDEX idx_threads_subagent_kind_updated_at
    ON threads(subagent_kind, archived, updated_at DESC, id DESC);

CREATE INDEX idx_threads_subagent_kind_created_at
    ON threads(subagent_kind, archived, created_at DESC, id DESC);

CREATE INDEX idx_threads_subagent_parent_updated_at
    ON threads(subagent_parent_thread_id, subagent_kind, updated_at DESC, id DESC);

CREATE INDEX idx_threads_subagent_parent_created_at
    ON threads(subagent_parent_thread_id, subagent_kind, created_at DESC, id DESC);
