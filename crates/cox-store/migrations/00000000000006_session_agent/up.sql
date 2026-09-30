-- A session driven by an external ACP agent (T52.6, DT§3.3.1): `agent` is
-- the entry's name, `agent_session` the agent's own ACP `sessionId`, which
-- `session/load` needs to reopen it. Both NULL for a cox session, and for
-- every session written before these columns existed.
ALTER TABLE sessions ADD COLUMN agent TEXT;
ALTER TABLE sessions ADD COLUMN agent_session TEXT;
