-- AgentOS process records are ephemeral runtime bookkeeping. Rebuild the table
-- with a process_key primary key so identical numeric process ids from different
-- runtime backends cannot overwrite each other.
DROP TABLE IF EXISTS agent_os_processes;

CREATE TABLE IF NOT EXISTS agent_os_processes (
    process_key TEXT PRIMARY KEY,
    process_id INTEGER NOT NULL,
    runtime_kind TEXT,
    runtime_owner_id TEXT,
    command_id TEXT,
    thread_id TEXT,
    task_id TEXT,
    status TEXT,
    snapshot_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_os_processes_status_runtime_updated
    ON agent_os_processes(status, runtime_kind, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_os_processes_owner_process
    ON agent_os_processes(runtime_kind, runtime_owner_id, process_id);

CREATE INDEX IF NOT EXISTS idx_agent_os_processes_thread_task
    ON agent_os_processes(thread_id, task_id);

CREATE INDEX IF NOT EXISTS idx_agent_os_processes_command
    ON agent_os_processes(command_id);
