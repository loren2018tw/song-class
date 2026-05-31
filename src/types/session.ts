export type ServiceStatus = "stopped" | "starting" | "running" | "error";

export interface ServerInfo {
  status: ServiceStatus;
  ip: string;
  url: string;
  error?: string | null;
}

export interface StudentSession {
  connection_id: string;
  student_id?: number;
  classroom_id?: number;
  seat_no_text?: string;
  nickname: string;
  connected: boolean;
  focus_status?: StudentFocusStatus;
  focus_updated_at?: number;
}

export interface ClassroomSummary {
  id: number;
  name: string;
}

export interface ClassroomStudent {
  id: number;
  classroom_id: number;
  seat_no_text: string;
  nickname: string;
  display_name: string;
  occupied: boolean;
}

export interface ClassroomStatePayload {
  current_classroom: ClassroomSummary;
  classrooms: ClassroomSummary[];
  students: ClassroomStudent[];
}

export type StudentFocusStatus = "focused" | "away";

export interface SignalEnvelope {
  event: string;
  source?: string;
  target?: string;
  nickname?: string;
  payload?: unknown;
  message?: string;
}

export type ActiveModule =
  | "home"
  | "whiteboard"
  | "quick_qa"
  | "teacher_screen_broadcast";

export interface LessonModuleState {
  activeModule: ActiveModule;
  modeVersion: number;
  tabVersion: number;
}
