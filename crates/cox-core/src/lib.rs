//! The agent loop as a state machine: turns, context assembly, compaction,
//! the permission engine, hooks, model routing, budget. No I/O except
//! through traits in `cox-protocol`, so the loop can be tested by replaying
//! events instead of calling a model.
