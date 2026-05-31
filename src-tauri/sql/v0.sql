PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS classrooms (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS students (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  classroom_id INTEGER NOT NULL,
  seat_no_text TEXT NOT NULL,
  nickname TEXT NOT NULL DEFAULT '',
  FOREIGN KEY (classroom_id) REFERENCES classrooms(id) ON DELETE CASCADE,
  UNIQUE (classroom_id, seat_no_text)
);

CREATE INDEX IF NOT EXISTS idx_students_classroom_id
  ON students(classroom_id);
