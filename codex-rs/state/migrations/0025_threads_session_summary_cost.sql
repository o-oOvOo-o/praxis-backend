ALTER TABLE threads ADD COLUMN session_summary TEXT;
ALTER TABLE threads ADD COLUMN total_cost_micros INTEGER;
ALTER TABLE threads ADD COLUMN last_cost_micros INTEGER;
