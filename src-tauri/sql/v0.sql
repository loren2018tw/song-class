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

CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  classroom_id INTEGER NOT NULL,
  task_date TEXT NOT NULL,
  title TEXT NOT NULL,
  show_in_contact_book INTEGER NOT NULL DEFAULT 1 CHECK (show_in_contact_book IN (0, 1)),
  requires_tracking INTEGER NOT NULL DEFAULT 0 CHECK (requires_tracking IN (0, 1)),
  is_completed INTEGER NOT NULL DEFAULT 0 CHECK (is_completed IN (0, 1)),
  FOREIGN KEY (classroom_id) REFERENCES classrooms(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tasks_classroom_date
  ON tasks(classroom_id, task_date);

CREATE INDEX IF NOT EXISTS idx_tasks_classroom_tracking_completed
  ON tasks(classroom_id, requires_tracking, is_completed);

CREATE TABLE IF NOT EXISTS task_submissions (
  task_id INTEGER NOT NULL,
  student_id INTEGER NOT NULL,
  submitted INTEGER NOT NULL DEFAULT 0 CHECK (submitted IN (0, 1)),
  PRIMARY KEY (task_id, student_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (student_id) REFERENCES students(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_submissions_student
  ON task_submissions(student_id);
