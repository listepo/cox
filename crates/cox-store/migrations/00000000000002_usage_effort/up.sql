-- The reasoning effort the router settled on for one provider call (§1.9).
-- Nullable on purpose: rows written before this column existed recorded no
-- effort, and the ledger reports what it observed rather than a default.
ALTER TABLE usage ADD COLUMN effort TEXT;
