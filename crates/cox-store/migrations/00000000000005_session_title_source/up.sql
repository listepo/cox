-- Who set `sessions.title` (A113): 'auto' for the title job's answer,
-- 'user' for a rename. Nullable on purpose: a session with no title, or one
-- titled before this column existed, has no source, and an automatic title
-- may replace anything but a 'user' one.
ALTER TABLE sessions ADD COLUMN title_source TEXT;
