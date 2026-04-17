CREATE TABLE teams (
    id TEXT PRIMARY KEY,
    lead_thread_id TEXT NOT NULL,
    name TEXT NOT NULL,
    objective TEXT,
    execution_mode TEXT NOT NULL,
    resume_mode TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_teams_lead_thread_id
    ON teams(lead_thread_id);

CREATE TABLE team_teammates (
    team_id TEXT NOT NULL,
    teammate_id TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT,
    status TEXT NOT NULL,
    thread_id TEXT,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, teammate_id),
    FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE
);

CREATE INDEX idx_team_teammates_team_status
    ON team_teammates(team_id, status, created_at ASC);

CREATE UNIQUE INDEX idx_team_teammates_thread_id
    ON team_teammates(thread_id)
    WHERE thread_id IS NOT NULL;

CREATE TABLE team_tasks (
    team_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL,
    assignee_teammate_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER,
    PRIMARY KEY (team_id, task_id),
    FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE
);

CREATE INDEX idx_team_tasks_team_status
    ON team_tasks(team_id, status, created_at ASC);

CREATE TABLE team_mailbox_messages (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    sender_kind TEXT NOT NULL,
    sender_teammate_id TEXT,
    recipient_kind TEXT NOT NULL,
    recipient_teammate_id TEXT,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE
);

CREATE INDEX idx_team_mailbox_messages_team_created
    ON team_mailbox_messages(team_id, created_at ASC, id ASC);
