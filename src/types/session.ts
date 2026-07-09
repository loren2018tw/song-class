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
  points?: number;
  group_no?: number;
}

export interface ClassroomSummary {
  id: number;
  name: string;
  line_enabled: boolean;
  line_channel_access_token: string;
  line_channel_secret: string;
  line_rich_menu_id: string;
}

export interface UpdateClassroomRequest {
  name?: string;
  line_enabled?: boolean;
  line_channel_access_token?: string;
  line_channel_secret?: string;
}

export interface ClassroomStudent {
  id: number;
  classroom_id: number;
  seat_no_text: string;
  nickname: string;
  display_name: string;
  occupied: boolean;
  points: number;
  group_no: number;
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
  | "contact_book_management"
  | "whiteboard"
  | "quick_qa"
  | "teacher_screen_broadcast"
  | "student_points";

export interface LessonModuleState {
  activeModule: ActiveModule;
  modeVersion: number;
  tabVersion: number;
}
