CREATE TABLE IF NOT EXISTS thread_names (
    thread_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_names_updated_at
ON thread_names(updated_at DESC);
