-- Plugin grants and per-plugin persistent state (PL§3, A52). A grant is
-- keyed by plugin id, scope (the whole user install, or one repository as
-- `project:<root>`) and package digest: changed bytes or wider capabilities
-- need a new row, never an update of an existing key. `plugin_kv` is the
-- only state that outlives a call, since extism's own vars are switched off
-- (`max_var_bytes = 0`, P12); it is quota-checked in `Store`, not here.
CREATE TABLE plugin_grants (
  plugin_id TEXT NOT NULL, scope TEXT NOT NULL, digest TEXT NOT NULL,
  capabilities TEXT NOT NULL, enabled INTEGER NOT NULL, source TEXT NOT NULL,
  decided_at TEXT NOT NULL,
  PRIMARY KEY (plugin_id, scope, digest)
);

CREATE TABLE plugin_kv (
  plugin_id TEXT NOT NULL, key TEXT NOT NULL, value BLOB NOT NULL, updated_at TEXT NOT NULL,
  PRIMARY KEY (plugin_id, key)
);
