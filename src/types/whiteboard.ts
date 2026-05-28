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
  backgroundColor: string;
  strokes: WhiteboardStroke[];
}

export function createEmptyWhiteboardSnapshot(): WhiteboardSnapshot {
  return {
    version: 1,
    canvasWidth: WHITEBOARD_CANVAS_WIDTH,
    canvasHeight: WHITEBOARD_CANVAS_HEIGHT,
    backgroundColor: WHITEBOARD_BACKGROUND_COLOR,
    strokes: [],
  };
}
