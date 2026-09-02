//! One SQLite file (`~/.cox/cox.db`): sessions, rollouts (JSONL), the
//! tool-output archive, memory, and the cost ledger. Separate so `cox-core`
//! never opens a file directly; it only calls the `Store` trait.
