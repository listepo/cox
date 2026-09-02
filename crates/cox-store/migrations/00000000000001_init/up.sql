-- Initial schema (plan.md §1.7). PRAGMAs (journal_mode, foreign_keys,
-- busy_timeout) are set on connection open in `Store::open`, not here.

CREATE TABLE migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);

CREATE TABLE sessions (
  id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  cwd TEXT NOT NULL, project_slug TEXT NOT NULL, title TEXT, parent_id TEXT,
  rollout_path TEXT NOT NULL, turns INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL NOT NULL DEFAULT 0, state TEXT NOT NULL CHECK (state IN ('open','closed','error'))
);

CREATE TABLE usage (
  id INTEGER PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), turn INTEGER NOT NULL,
  job TEXT NOT NULL, tier TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
  input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL,
  estimated INTEGER NOT NULL DEFAULT 0, cost_usd REAL NOT NULL, latency_ms INTEGER NOT NULL,
  context_tokens INTEGER NOT NULL,            -- what the model saw this call (for context-token-turns)
  created_at TEXT NOT NULL
);
CREATE INDEX usage_session ON usage(session_id, turn);
CREATE INDEX usage_day ON usage(created_at);

CREATE TABLE archive (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL, call_id TEXT NOT NULL, tool TEXT NOT NULL,
  subject TEXT, bytes INTEGER NOT NULL, sha256 TEXT NOT NULL,
  inline BLOB, path TEXT, created_at TEXT NOT NULL,
  CHECK ((inline IS NULL) <> (path IS NULL))
);

CREATE TABLE memory (
  id INTEGER PRIMARY KEY, project_slug TEXT NOT NULL, name TEXT NOT NULL, path TEXT NOT NULL,
  kind TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(project_slug, name)
);
CREATE VIRTUAL TABLE memory_fts USING fts5(name, body, project_slug UNINDEXED);
CREATE VIRTUAL TABLE rollout_fts USING fts5(session_id UNINDEXED, turn UNINDEXED, text);
