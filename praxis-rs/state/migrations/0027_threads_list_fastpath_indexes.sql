-- Speed up resume/thread-list pages. These queries always filter out archived
-- rows (or request archived rows) and rows without a first user message, then
-- order by updated_at or created_at with id as the cursor tie-breaker.
CREATE INDEX IF NOT EXISTS idx_threads_active_updated_at
ON threads(updated_at DESC, id DESC)
WHERE archived = 0 AND first_user_message <> '';

CREATE INDEX IF NOT EXISTS idx_threads_active_created_at
ON threads(created_at DESC, id DESC)
WHERE archived = 0 AND first_user_message <> '';

CREATE INDEX IF NOT EXISTS idx_threads_archived_updated_at
ON threads(updated_at DESC, id DESC)
WHERE archived = 1 AND first_user_message <> '';

CREATE INDEX IF NOT EXISTS idx_threads_archived_created_at
ON threads(created_at DESC, id DESC)
WHERE archived = 1 AND first_user_message <> '';
