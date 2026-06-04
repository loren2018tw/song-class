-- Draft migration for next release.
-- Keep modifying this file during development until next release is finalized.
-- Example:
-- ALTER TABLE students ADD COLUMN avatar_url TEXT NOT NULL DEFAULT '';

ALTER TABLE students ADD COLUMN group_no INTEGER NOT NULL DEFAULT 0;
ALTER TABLE students ADD COLUMN points INTEGER NOT NULL DEFAULT 0;
