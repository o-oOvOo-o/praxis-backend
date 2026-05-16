CREATE VIRTUAL TABLE IF NOT EXISTS threads_fts USING fts5(
    title,
    first_user_message,
    session_summary,
    cwd,
    git_branch,
    model,
    model_provider,
    agent_nickname,
    agent_role,
    content='threads',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE IF NOT EXISTS thread_names_fts USING fts5(
    thread_id UNINDEXED,
    name,
    content='thread_names',
    content_rowid='rowid'
);

INSERT INTO threads_fts(threads_fts) VALUES('rebuild');
INSERT INTO thread_names_fts(thread_names_fts) VALUES('rebuild');

CREATE TRIGGER IF NOT EXISTS threads_fts_ai AFTER INSERT ON threads BEGIN
    INSERT INTO threads_fts(
        rowid,
        title,
        first_user_message,
        session_summary,
        cwd,
        git_branch,
        model,
        model_provider,
        agent_nickname,
        agent_role
    ) VALUES (
        new.rowid,
        new.title,
        new.first_user_message,
        new.session_summary,
        new.cwd,
        new.git_branch,
        new.model,
        new.model_provider,
        new.agent_nickname,
        new.agent_role
    );
END;

CREATE TRIGGER IF NOT EXISTS threads_fts_ad AFTER DELETE ON threads BEGIN
    INSERT INTO threads_fts(
        threads_fts,
        rowid,
        title,
        first_user_message,
        session_summary,
        cwd,
        git_branch,
        model,
        model_provider,
        agent_nickname,
        agent_role
    ) VALUES (
        'delete',
        old.rowid,
        old.title,
        old.first_user_message,
        old.session_summary,
        old.cwd,
        old.git_branch,
        old.model,
        old.model_provider,
        old.agent_nickname,
        old.agent_role
    );
END;

CREATE TRIGGER IF NOT EXISTS threads_fts_au AFTER UPDATE ON threads BEGIN
    INSERT INTO threads_fts(
        threads_fts,
        rowid,
        title,
        first_user_message,
        session_summary,
        cwd,
        git_branch,
        model,
        model_provider,
        agent_nickname,
        agent_role
    ) VALUES (
        'delete',
        old.rowid,
        old.title,
        old.first_user_message,
        old.session_summary,
        old.cwd,
        old.git_branch,
        old.model,
        old.model_provider,
        old.agent_nickname,
        old.agent_role
    );
    INSERT INTO threads_fts(
        rowid,
        title,
        first_user_message,
        session_summary,
        cwd,
        git_branch,
        model,
        model_provider,
        agent_nickname,
        agent_role
    ) VALUES (
        new.rowid,
        new.title,
        new.first_user_message,
        new.session_summary,
        new.cwd,
        new.git_branch,
        new.model,
        new.model_provider,
        new.agent_nickname,
        new.agent_role
    );
END;

CREATE TRIGGER IF NOT EXISTS thread_names_fts_ai AFTER INSERT ON thread_names BEGIN
    INSERT INTO thread_names_fts(rowid, thread_id, name)
    VALUES (new.rowid, new.thread_id, new.name);
END;

CREATE TRIGGER IF NOT EXISTS thread_names_fts_ad AFTER DELETE ON thread_names BEGIN
    INSERT INTO thread_names_fts(thread_names_fts, rowid, thread_id, name)
    VALUES ('delete', old.rowid, old.thread_id, old.name);
END;

CREATE TRIGGER IF NOT EXISTS thread_names_fts_au AFTER UPDATE ON thread_names BEGIN
    INSERT INTO thread_names_fts(thread_names_fts, rowid, thread_id, name)
    VALUES ('delete', old.rowid, old.thread_id, old.name);
    INSERT INTO thread_names_fts(rowid, thread_id, name)
    VALUES (new.rowid, new.thread_id, new.name);
END;
