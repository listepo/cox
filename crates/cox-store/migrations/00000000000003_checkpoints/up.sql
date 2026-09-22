-- Pre-images archived before a tool call changed a file, plus one marker
-- row per user turn (T26.1). `archive_id` points at the `archive` row that
-- holds the bytes; NULL for a created file, a turn marker, or a pre-image
-- too large to keep.
CREATE TABLE checkpoints (
  id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, turn INTEGER NOT NULL, call_id TEXT,
  path TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('pre','created','deleted','turn')),
  archive_id TEXT, created_at TEXT NOT NULL
);
CREATE INDEX checkpoints_session ON checkpoints(session_id, turn);
