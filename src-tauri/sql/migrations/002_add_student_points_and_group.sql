-- Add student points module columns.
-- group_no: 0 means ungrouped.
-- points: allows negative values.

ALTER TABLE students ADD COLUMN group_no INTEGER NOT NULL DEFAULT 0;
ALTER TABLE students ADD COLUMN points INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS reminder_boards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    subtitle TEXT NOT NULL,
    icon TEXT NOT NULL
);
