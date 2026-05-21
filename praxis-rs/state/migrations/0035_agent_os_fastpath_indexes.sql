-- AgentOS is still snapshot-backed for forward compatibility, but runtime
-- scheduling, janitors, and dashboards need indexed fast paths.  These JSON
-- expression indexes preserve the flexible snapshot schema while making the
-- hot queries (pending commands, live leases, running tasks/threads) avoid
-- full table scans until the tables are fully typed.

CREATE INDEX IF NOT EXISTS idx_agent_os_threads_scope_state_updated
    ON agent_os_threads(
        json_extract(snapshot_json, '$.coordination_scope'),
        json_extract(snapshot_json, '$.state'),
        updated_at DESC
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_threads_task_updated
    ON agent_os_threads(
        json_extract(snapshot_json, '$.current_task_id'),
        updated_at DESC
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_tasks_status_assignee_priority
    ON agent_os_tasks(
        json_extract(snapshot_json, '$.status'),
        json_extract(snapshot_json, '$.assigned_thread_id'),
        json_extract(snapshot_json, '$.priority'),
        updated_at DESC
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_leases_resource_scope_owner
    ON agent_os_leases(
        json_extract(snapshot_json, '$.resource_type'),
        json_extract(snapshot_json, '$.scope'),
        json_extract(snapshot_json, '$.owner_thread_id')
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_leases_expires
    ON agent_os_leases(json_extract(snapshot_json, '$.expires_at'));

CREATE INDEX IF NOT EXISTS idx_agent_os_leases_command_process
    ON agent_os_leases(
        json_extract(snapshot_json, '$.command_id'),
        json_extract(snapshot_json, '$.process_id')
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_commands_thread_task_open
    ON agent_os_commands(
        json_extract(snapshot_json, '$.thread_id'),
        json_extract(snapshot_json, '$.task_id'),
        json_extract(snapshot_json, '$.ended_at')
    );

CREATE INDEX IF NOT EXISTS idx_agent_os_artifacts_task_type_created
    ON agent_os_artifacts(
        json_extract(snapshot_json, '$.task_id'),
        json_extract(snapshot_json, '$.artifact_type'),
        json_extract(snapshot_json, '$.created_at')
    );
