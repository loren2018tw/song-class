export const WHITEBOARD_CANVAS_WIDTH = 1280;
export const WHITEBOARD_CANVAS_HEIGHT = 720;
export const WHITEBOARD_BACKGROUND_COLOR = "#ffffff";
export const WHITEBOARD_COLOR_OPTIONS = [
  "#1e88e5",
  "#43a047",
  "#e53935",
  "#fb8c00",
  "#8e24aa",
  "#212121",
] as const;
export const WHITEBOARD_SIZE_OPTIONS = [4, 8, 12, 16, 24] as const;

export type WhiteboardTool = "pen" | "eraser";
export type WhiteboardColor = (typeof WHITEBOARD_COLOR_OPTIONS)[number];
export type WhiteboardMode = "home" | "whiteboard";
export type WhiteboardBoardTab = "teacher-board" | "student-board";

export interface WhiteboardPoint {
  x: number;
  y: number;
}

export interface WhiteboardStroke {
  id: string;
  tool: WhiteboardTool;
  color: string;
  size: number;
  points: WhiteboardPoint[];
}

export interface WhiteboardSnapshot {
  version: 1;
  canvasWidth: number;
  canvasHeight: number;
  backgroundImage: string | null;
  backgroundColor: string;
  strokes: WhiteboardStroke[];
}

export interface WhiteboardStrokeBeginPayload {
  type: "stroke-begin";
  stroke: WhiteboardStroke;
}

export interface WhiteboardStrokePointPayload {
  type: "stroke-point";
  strokeId: string;
  point: WhiteboardPoint;
}

export interface WhiteboardStrokeEndPayload {
  type: "stroke-end";
  strokeId: string;
}

export interface WhiteboardClearPayload {
  type: "clear";
}

export interface WhiteboardBackgroundChangePayload {
  type: "background-change";
  backgroundImage: string | null;
  backgroundColor: string;
}

export interface WhiteboardToolChangePayload {
  type: "tool-change";
  tool: WhiteboardTool;
  color: string;
  size: number;
}

export type WhiteboardIncrementalEventPayload =
  | WhiteboardStrokeBeginPayload
  | WhiteboardStrokePointPayload
  | WhiteboardStrokeEndPayload
  | WhiteboardClearPayload
  | WhiteboardBackgroundChangePayload
  | WhiteboardToolChangePayload;

export type WhiteboardIncrementalEvent = WhiteboardIncrementalEventPayload & {
  seq: number;
  timestamp: number;
};

export interface WhiteboardModeSyncMessage {
  kind: "mode-sync";
  mode: WhiteboardMode;
  modeVersion: number;
  activeTab: WhiteboardBoardTab;
  tabVersion: number;
}

export interface WhiteboardSnapshotSyncMessage {
  kind: "whiteboard-snapshot";
  modeVersion: number;
  tabVersion: number;
  boardTab: WhiteboardBoardTab;
  seq: number;
  reason: "join" | "resync" | "manual";
  snapshot: WhiteboardSnapshot;
}

export interface WhiteboardEventBatchMessage {
  kind: "whiteboard-events-batch";
  modeVersion: number;
  tabVersion: number;
  boardTab: WhiteboardBoardTab;
  startSeq: number;
  endSeq: number;
  events: WhiteboardIncrementalEvent[];
}

export interface WhiteboardSnapshotRequestMessage {
  kind: "snapshot-request";
  boardTab: WhiteboardBoardTab;
  reason: "seq-gap" | "join-init";
  sinceSeq: number;
}

export interface WhiteboardStudentEventBatchMessage {
  kind: "student-events-batch";
  senderId?: string;
  tabVersion: number;
  boardTab: "student-board";
  startSeq: number;
  endSeq: number;
  events: WhiteboardIncrementalEvent[];
}

export interface WhiteboardTeacherStudentEventBatchMessage {
  kind: "teacher-student-events-batch";
  boardTab: "student-board";
  startSeq: number;
  endSeq: number;
  events: WhiteboardIncrementalEvent[];
}

export interface WhiteboardTeacherStudentResyncRequestMessage {
  kind: "teacher-student-resync-request";
  boardTab: "student-board";
  reason: "join-init" | "seq-gap";
  sinceSeq: number;
}

export interface WhiteboardTeacherStudentSnapshotMessage {
  kind: "teacher-student-snapshot";
  boardTab: "student-board";
  seq: number;
  reason: "join" | "resync";
  snapshot: WhiteboardSnapshot;
}

export interface WhiteboardStudentBoardControlMessage {
  kind: "student-board-control";
  action: "clear-all" | "replace-strokes" | "set-background";
  tabVersion: number;
  strokes?: WhiteboardStroke[];
  backgroundImage?: string | null;
}

export interface WhiteboardStudentViewControlMessage {
  kind: "student-view-control";
  forceTeacherBoardView: boolean;
}

export interface WhiteboardStudentOpenUrlMessage {
  kind: "student-open-url";
  url: string;
}

export type WhiteboardSyncMessage =
  | WhiteboardModeSyncMessage
  | WhiteboardSnapshotSyncMessage
  | WhiteboardEventBatchMessage
  | WhiteboardSnapshotRequestMessage
  | WhiteboardStudentEventBatchMessage
  | WhiteboardTeacherStudentEventBatchMessage
  | WhiteboardTeacherStudentResyncRequestMessage
  | WhiteboardTeacherStudentSnapshotMessage
  | WhiteboardStudentBoardControlMessage
  | WhiteboardStudentViewControlMessage
  | WhiteboardStudentOpenUrlMessage;

export function cloneWhiteboardStroke(
  stroke: WhiteboardStroke,
): WhiteboardStroke {
  return {
    id: stroke.id,
    tool: stroke.tool,
    color: stroke.color,
    size: stroke.size,
    points: stroke.points.map((point) => ({ x: point.x, y: point.y })),
  };
}

export function cloneWhiteboardSnapshot(
  snapshot: WhiteboardSnapshot,
): WhiteboardSnapshot {
  return {
    version: snapshot.version,
    canvasWidth: snapshot.canvasWidth,
    canvasHeight: snapshot.canvasHeight,
    backgroundImage: snapshot.backgroundImage ?? null,
    backgroundColor: snapshot.backgroundColor,
    strokes: snapshot.strokes.map((stroke) => cloneWhiteboardStroke(stroke)),
  };
}

export function isWhiteboardSyncMessage(
  value: unknown,
): value is WhiteboardSyncMessage {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const kind = (value as { kind?: unknown }).kind;
  return (
    kind === "mode-sync" ||
    kind === "whiteboard-snapshot" ||
    kind === "whiteboard-events-batch" ||
    kind === "snapshot-request" ||
    kind === "student-events-batch" ||
    kind === "teacher-student-events-batch" ||
    kind === "teacher-student-resync-request" ||
    kind === "teacher-student-snapshot" ||
    kind === "student-board-control" ||
    kind === "student-view-control" ||
    kind === "student-open-url"
  );
}

export function createEmptyWhiteboardSnapshot(): WhiteboardSnapshot {
  return {
    version: 1,
    canvasWidth: WHITEBOARD_CANVAS_WIDTH,
    canvasHeight: WHITEBOARD_CANVAS_HEIGHT,
    backgroundImage: null,
    backgroundColor: WHITEBOARD_BACKGROUND_COLOR,
    strokes: [],
  };
}
