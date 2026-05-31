export interface ContactBookTask {
  id: number;
  classroom_id: number;
  task_date: string;
  title: string;
  show_in_contact_book: boolean;
  requires_tracking: boolean;
  is_completed: boolean;
  student_count: number;
  submitted_count: number;
}

export interface TaskSubmissionStatus {
  student_id: number;
  classroom_id: number;
  seat_no_text: string;
  nickname: string;
  display_name: string;
  submitted: boolean;
}

export interface TaskSubmissionsPayload {
  task: ContactBookTask;
  submissions: TaskSubmissionStatus[];
}

export type TaskTab = "contact-book" | "submission";
export type TaskCompletionFilter = "all" | "unfinished" | "completed";
