ALTER TABLE threads ADD COLUMN agent_base_name TEXT;
ALTER TABLE threads ADD COLUMN agent_title TEXT;
ALTER TABLE threads ADD COLUMN agent_display_name TEXT;
ALTER TABLE threads ADD COLUMN subagent_agent_base_name TEXT;
ALTER TABLE threads ADD COLUMN subagent_agent_title TEXT;
ALTER TABLE threads ADD COLUMN subagent_agent_display_name TEXT;

UPDATE threads
SET
    agent_base_name = COALESCE(agent_base_name, agent_nickname),
    agent_display_name = COALESCE(agent_display_name, agent_nickname)
WHERE agent_nickname IS NOT NULL;

UPDATE threads
SET
    subagent_agent_base_name = COALESCE(subagent_agent_base_name, subagent_agent_nickname),
    subagent_agent_display_name = COALESCE(subagent_agent_display_name, subagent_agent_nickname)
WHERE subagent_agent_nickname IS NOT NULL;
