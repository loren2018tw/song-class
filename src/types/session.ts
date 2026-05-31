export type ServiceStatus = "stopped" | "starting" | "running" | "error";

export interface ServerInfo {
  status: ServiceStatus;
  ip: string;
  url: string;
  error?: string | null;
}

export interface StudentSession {
  connection_id: string;
  nickname: string;
  connected: boolean;
  focus_status?: StudentFocusStatus;
  focus_updated_at?: number;
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
