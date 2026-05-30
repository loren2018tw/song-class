<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  cloneWhiteboardSnapshot,
  cloneWhiteboardStroke,
  createEmptyWhiteboardSnapshot,
  WHITEBOARD_BACKGROUND_COLOR,
  WHITEBOARD_CANVAS_HEIGHT,
  WHITEBOARD_CANVAS_WIDTH,
  WHITEBOARD_COLOR_OPTIONS,
  type WhiteboardColor,
  type WhiteboardIncrementalEventPayload,
  type WhiteboardPoint,
  type WhiteboardSnapshot,
  type WhiteboardStroke,
  type WhiteboardTool,
} from "../types/whiteboard";

const props = withDefaults(
  defineProps<{
    title?: string;
    snapshot?: WhiteboardSnapshot | null;
    backgroundImage?: string | null;
    showToolbar?: boolean;
  }>(),
  {
    title: "",
    showToolbar: true,
  },
);

const emit = defineEmits<{
  (event: "update:snapshot", snapshot: WhiteboardSnapshot): void;
  (event: "sync-event", payload: WhiteboardIncrementalEventPayload): void;
}>();

const drawingCanvasRef = ref<HTMLCanvasElement | null>(null);
const backgroundCanvasRef = ref<HTMLCanvasElement | null>(null);
const canvasWrapperRef = ref<HTMLDivElement | null>(null);
const currentSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
const stageWidth = ref(640);
const stageHeight = ref(360);
const currentTool = ref<WhiteboardTool>("pen");
const currentColor = ref<WhiteboardColor>(WHITEBOARD_COLOR_OPTIONS[0]);
const currentSize = ref(8);
const brushSizeValue = ref(currentSize.value);
const isDrawing = ref(false);
const activeStroke = ref<WhiteboardStroke | null>(null);
const drawingPointerId = ref<number | null>(null);
const drawingLayerDirty = ref(true);
const cachedFlattenedDrawingLayer = ref<{
  imageDataUrl: string;
  width: number;
  height: number;
  updatedAt: number;
} | null>(null);
const activeTouchPointerIds = new Set<number>();
let strokeSequence = 0;
let resizeObserver: ResizeObserver | null = null;
let backgroundRenderSequence = 0;

const MIN_STAGE_WIDTH = 640;
const MIN_STAGE_HEIGHT = 360;
const STAGE_RATIO = WHITEBOARD_CANVAS_WIDTH / WHITEBOARD_CANVAS_HEIGHT;

const stageStyle = computed(() => ({
  width: `${stageWidth.value}px`,
  height: `${stageHeight.value}px`,
}));
const isReadOnlyMode = computed(() => !props.showToolbar);

function calculateStageSize() {
  const wrapper = canvasWrapperRef.value;
  if (!wrapper) {
    return;
  }

  const wrapperBounds = wrapper.getBoundingClientRect();
  const availableWidth = Math.max(0, Math.floor(wrapperBounds.width));
  const availableHeight = Math.max(0, Math.floor(wrapperBounds.height));

  if (availableWidth === 0 || availableHeight === 0) {
    return;
  }

  const minimumWidthTarget = Math.min(MIN_STAGE_WIDTH, availableWidth);
  const minimumHeightTarget = Math.min(MIN_STAGE_HEIGHT, availableHeight);

  let fittedWidth = Math.min(
    availableWidth,
    Math.floor(availableHeight * STAGE_RATIO),
  );
  let fittedHeight = Math.floor(fittedWidth / STAGE_RATIO);

  if (
    fittedWidth >= minimumWidthTarget &&
    fittedHeight >= minimumHeightTarget
  ) {
    stageWidth.value = fittedWidth;
    stageHeight.value = fittedHeight;
    return;
  }

  fittedWidth = minimumWidthTarget;
  fittedHeight = Math.floor(fittedWidth / STAGE_RATIO);

  if (fittedHeight > availableHeight) {
    fittedHeight = minimumHeightTarget;
    fittedWidth = Math.floor(fittedHeight * STAGE_RATIO);
  }

  stageWidth.value = Math.max(1, Math.min(fittedWidth, availableWidth));
  stageHeight.value = Math.max(1, Math.min(fittedHeight, availableHeight));
}

function cloneSnapshot(snapshot: WhiteboardSnapshot): WhiteboardSnapshot {
  return cloneWhiteboardSnapshot(snapshot);
}

function markDrawingLayerDirty() {
  drawingLayerDirty.value = true;
}

function hasStrokesChanged(
  previousSnapshot: WhiteboardSnapshot,
  nextSnapshot: WhiteboardSnapshot,
) {
  if (previousSnapshot.strokes.length !== nextSnapshot.strokes.length) {
    return true;
  }

  for (let index = 0; index < previousSnapshot.strokes.length; index += 1) {
    const previousStroke = previousSnapshot.strokes[index];
    const nextStroke = nextSnapshot.strokes[index];
    if (!nextStroke) {
      return true;
    }

    if (
      previousStroke.id !== nextStroke.id ||
      previousStroke.tool !== nextStroke.tool ||
      previousStroke.color !== nextStroke.color ||
      previousStroke.size !== nextStroke.size ||
      previousStroke.points.length !== nextStroke.points.length
    ) {
      return true;
    }
  }

  return false;
}

function getFlattenedDrawingLayer() {
  const canvasElement = drawingCanvasRef.value;
  if (!canvasElement) {
    return null;
  }

  if (!drawingLayerDirty.value && cachedFlattenedDrawingLayer.value) {
    return cachedFlattenedDrawingLayer.value;
  }

  const flattened = {
    imageDataUrl: canvasElement.toDataURL("image/png"),
    width: WHITEBOARD_CANVAS_WIDTH,
    height: WHITEBOARD_CANVAS_HEIGHT,
    updatedAt: Date.now(),
  };

  cachedFlattenedDrawingLayer.value = flattened;
  drawingLayerDirty.value = false;
  return flattened;
}

defineExpose({
  getFlattenedDrawingLayer,
});

function emitSyncEvent(payload: WhiteboardIncrementalEventPayload) {
  if (isReadOnlyMode.value) {
    return;
  }
  emit("sync-event", payload);
}

function createStroke(tool: WhiteboardTool): WhiteboardStroke {
  const resolvedColor = currentColor.value;
  const resolvedSize =
    tool === "eraser" ? currentSize.value + 10 : currentSize.value;

  return {
    id: `stroke-${Date.now()}-${strokeSequence++}`,
    tool,
    color: resolvedColor,
    size: resolvedSize,
    points: [],
  };
}

function getCanvasContext(canvasElement: HTMLCanvasElement | null) {
  if (!canvasElement) {
    return null;
  }

  const context = canvasElement.getContext("2d");
  if (!context) {
    return null;
  }

  context.lineCap = "round";
  context.lineJoin = "round";
  return context;
}

function normalizeBackgroundInstruction(imagePath: string | null | undefined) {
  if (!imagePath) {
    return null;
  }

  const trimmed = imagePath.trim();
  if (!trimmed) {
    return null;
  }

  return trimmed;
}

function resolveBackgroundImageSource(imagePath: string | null | undefined) {
  const normalized = normalizeBackgroundInstruction(imagePath);
  if (!normalized) {
    return null;
  }

  const isAbsoluteSource =
    normalized.startsWith("http://") ||
    normalized.startsWith("https://") ||
    normalized.startsWith("data:") ||
    normalized.startsWith("blob:") ||
    normalized.startsWith("/");

  if (isAbsoluteSource) {
    return normalized;
  }

  return new URL(`../assets/bg/${normalized}`, import.meta.url).href;
}

function loadImage(src: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      resolve(image);
    };
    image.onerror = () => {
      reject(new Error(`背景圖片載入失敗: ${src}`));
    };
    image.src = src;
  });
}

function drawStroke(
  context: CanvasRenderingContext2D,
  stroke: WhiteboardStroke,
) {
  if (stroke.points.length === 0) {
    return;
  }

  context.save();
  context.globalCompositeOperation =
    stroke.tool === "eraser" ? "destination-out" : "source-over";
  context.strokeStyle = stroke.color;
  context.fillStyle = stroke.color;
  context.lineWidth = stroke.size;

  if (stroke.points.length === 1) {
    const point = stroke.points[0];
    context.beginPath();
    context.arc(point.x, point.y, stroke.size / 2, 0, Math.PI * 2);
    context.fill();
    context.restore();
    return;
  }

  context.beginPath();
  context.moveTo(stroke.points[0].x, stroke.points[0].y);
  for (let index = 1; index < stroke.points.length; index += 1) {
    const point = stroke.points[index];
    context.lineTo(point.x, point.y);
  }
  context.stroke();
  context.restore();
}

function redrawDrawingCanvas() {
  const context = getCanvasContext(drawingCanvasRef.value);
  if (!context) {
    return;
  }

  context.clearRect(0, 0, WHITEBOARD_CANVAS_WIDTH, WHITEBOARD_CANVAS_HEIGHT);
  for (const stroke of currentSnapshot.value.strokes) {
    drawStroke(context, stroke);
  }
}

async function redrawBackgroundCanvas() {
  const context = getCanvasContext(backgroundCanvasRef.value);
  if (!context) {
    return;
  }

  const currentRender = ++backgroundRenderSequence;
  context.clearRect(0, 0, WHITEBOARD_CANVAS_WIDTH, WHITEBOARD_CANVAS_HEIGHT);

  const fallbackColor = currentSnapshot.value.backgroundColor;
  if (fallbackColor) {
    context.fillStyle = fallbackColor;
    context.fillRect(0, 0, WHITEBOARD_CANVAS_WIDTH, WHITEBOARD_CANVAS_HEIGHT);
  }

  const backgroundSource = resolveBackgroundImageSource(
    currentSnapshot.value.backgroundImage,
  );

  if (!backgroundSource) {
    return;
  }

  try {
    const image = await loadImage(backgroundSource);
    if (currentRender !== backgroundRenderSequence) {
      return;
    }
    context.drawImage(
      image,
      0,
      0,
      WHITEBOARD_CANVAS_WIDTH,
      WHITEBOARD_CANVAS_HEIGHT,
    );
  } catch (error) {
    console.warn(String(error));
  }
}

function redrawAll() {
  void redrawBackgroundCanvas();
  redrawDrawingCanvas();
}

function emitSnapshot() {
  emit("update:snapshot", cloneSnapshot(currentSnapshot.value));
}

function hasBackgroundChanged(
  previousSnapshot: WhiteboardSnapshot,
  nextSnapshot: WhiteboardSnapshot,
) {
  return (
    (previousSnapshot.backgroundImage ?? null) !==
      (nextSnapshot.backgroundImage ?? null) ||
    previousSnapshot.backgroundColor !== nextSnapshot.backgroundColor
  );
}

function mergeInProgressStroke(
  incomingSnapshot: WhiteboardSnapshot,
): WhiteboardSnapshot {
  if (isReadOnlyMode.value || !isDrawing.value || !activeStroke.value) {
    return incomingSnapshot;
  }

  const mergedSnapshot = cloneSnapshot(incomingSnapshot);
  const localStroke = cloneWhiteboardStroke(activeStroke.value);
  const existingIndex = mergedSnapshot.strokes.findIndex(
    (stroke) => stroke.id === localStroke.id,
  );

  if (existingIndex === -1) {
    mergedSnapshot.strokes.push(localStroke);
  } else {
    mergedSnapshot.strokes[existingIndex] = localStroke;
  }

  return mergedSnapshot;
}

function syncLocalStateFromSnapshot(snapshot: WhiteboardSnapshot) {
  const previousSnapshot = currentSnapshot.value;
  const cloned = mergeInProgressStroke(cloneSnapshot(snapshot));
  currentSnapshot.value = {
    ...cloned,
    backgroundImage: cloned.backgroundImage ?? null,
  };

  if (!isReadOnlyMode.value && isDrawing.value && activeStroke.value) {
    const reboundStroke =
      currentSnapshot.value.strokes.find(
        (stroke) => stroke.id === activeStroke.value?.id,
      ) ?? null;

    if (reboundStroke) {
      activeStroke.value = reboundStroke;
    }
  }

  if (hasStrokesChanged(previousSnapshot, currentSnapshot.value)) {
    markDrawingLayerDirty();
  }

  if (hasBackgroundChanged(previousSnapshot, currentSnapshot.value)) {
    void redrawBackgroundCanvas();
  }
  redrawDrawingCanvas();
}

function updateBackgroundImage(
  imagePath: string | null | undefined,
  shouldEmit: boolean,
) {
  currentSnapshot.value.backgroundImage =
    normalizeBackgroundInstruction(imagePath);
  void redrawBackgroundCanvas();
  emitSyncEvent({
    type: "background-change",
    backgroundImage: currentSnapshot.value.backgroundImage,
    backgroundColor:
      currentSnapshot.value.backgroundColor || WHITEBOARD_BACKGROUND_COLOR,
  });
  if (shouldEmit) {
    emitSnapshot();
  }
}

function toCanvasPoint(event: PointerEvent): WhiteboardPoint | null {
  const canvasElement = drawingCanvasRef.value;
  if (!canvasElement) {
    return null;
  }

  const bounds = canvasElement.getBoundingClientRect();
  if (bounds.width === 0 || bounds.height === 0) {
    return null;
  }

  const x =
    ((event.clientX - bounds.left) / bounds.width) * WHITEBOARD_CANVAS_WIDTH;
  const y =
    ((event.clientY - bounds.top) / bounds.height) * WHITEBOARD_CANVAS_HEIGHT;

  return {
    x: Math.max(0, Math.min(WHITEBOARD_CANVAS_WIDTH, x)),
    y: Math.max(0, Math.min(WHITEBOARD_CANVAS_HEIGHT, y)),
  };
}

function isTouchPointer(event: PointerEvent) {
  return event.pointerType === "touch";
}

function finishActiveStroke(shouldEmitSnapshot: boolean) {
  if (!isDrawing.value) {
    return;
  }

  isDrawing.value = false;
  if (activeStroke.value) {
    emitSyncEvent({
      type: "stroke-end",
      strokeId: activeStroke.value.id,
    });
  }

  activeStroke.value = null;
  drawingPointerId.value = null;
  if (shouldEmitSnapshot) {
    emitSnapshot();
  }
}

function appendPoint(point: WhiteboardPoint) {
  const stroke = activeStroke.value;
  if (!stroke) {
    return;
  }

  stroke.points.push(point);
  markDrawingLayerDirty();
  emitSyncEvent({
    type: "stroke-point",
    strokeId: stroke.id,
    point: { x: point.x, y: point.y },
  });
  redrawDrawingCanvas();
}

function beginDrawing(event: PointerEvent) {
  if (isReadOnlyMode.value) {
    return;
  }

  if (isTouchPointer(event)) {
    activeTouchPointerIds.add(event.pointerId);
    if (activeTouchPointerIds.size > 1) {
      finishActiveStroke(true);
      return;
    }
  }

  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }

  const canvasElement = drawingCanvasRef.value;
  if (!canvasElement) {
    return;
  }

  if (event.isPrimary) {
    try {
      canvasElement.setPointerCapture(event.pointerId);
    } catch {
      // 某些非原生觸發的事件沒有可捕捉 pointer，忽略即可。
    }
  }

  event.preventDefault();
  isDrawing.value = true;
  drawingPointerId.value = event.pointerId;
  activeStroke.value = createStroke(currentTool.value);
  activeStroke.value.points.push(point);
  currentSnapshot.value.strokes.push(activeStroke.value);
  markDrawingLayerDirty();
  emitSyncEvent({
    type: "stroke-begin",
    stroke: cloneWhiteboardStroke(activeStroke.value),
  });
  redrawDrawingCanvas();
}

function continueDrawing(event: PointerEvent) {
  if (isReadOnlyMode.value) {
    return;
  }

  if (
    drawingPointerId.value !== null &&
    event.pointerId !== drawingPointerId.value
  ) {
    return;
  }

  if (isTouchPointer(event) && activeTouchPointerIds.size > 1) {
    return;
  }

  if (!isDrawing.value) {
    return;
  }

  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }

  event.preventDefault();
  appendPoint(point);
}

function endDrawing(event: PointerEvent) {
  if (isReadOnlyMode.value) {
    return;
  }

  if (isTouchPointer(event)) {
    activeTouchPointerIds.delete(event.pointerId);
  }

  if (
    drawingPointerId.value !== null &&
    event.pointerId !== drawingPointerId.value
  ) {
    return;
  }

  const canvasElement = drawingCanvasRef.value;
  if (canvasElement && event.isPrimary) {
    try {
      if (canvasElement.hasPointerCapture(event.pointerId)) {
        canvasElement.releasePointerCapture(event.pointerId);
      }
    } catch {
      // 同 beginDrawing，遇到無效 pointer id 時安全忽略。
    }
  }

  if (!isDrawing.value) {
    return;
  }

  event.preventDefault();
  finishActiveStroke(true);
}

function selectPenColor(color: WhiteboardColor) {
  currentTool.value = "pen";
  currentColor.value = color;
  emitSyncEvent({
    type: "tool-change",
    tool: "pen",
    color: currentColor.value,
    size: currentSize.value,
  });
  emitSnapshot();
}

function selectBrushSize(size: number) {
  currentSize.value = size;
  brushSizeValue.value = size;

  const tool = currentTool.value;
  const emittedSize =
    tool === "eraser" ? currentSize.value + 10 : currentSize.value;

  emitSyncEvent({
    type: "tool-change",
    tool,
    color: currentColor.value,
    size: emittedSize,
  });
  emitSnapshot();
}

function updateBrushSize(value: number) {
  const size = Math.max(4, Math.min(24, Math.round(value)));
  selectBrushSize(size);
}

function activateEraser() {
  currentTool.value = "eraser";
  emitSyncEvent({
    type: "tool-change",
    tool: "eraser",
    color: currentColor.value,
    size: currentSize.value + 10,
  });
  emitSnapshot();
}

function clearCanvas() {
  currentSnapshot.value.strokes = [];
  activeStroke.value = null;
  isDrawing.value = false;
  drawingPointerId.value = null;
  markDrawingLayerDirty();
  redrawDrawingCanvas();
  emitSyncEvent({ type: "clear" });
}

watch(
  () => props.snapshot,
  (snapshot) => {
    if (!snapshot) {
      currentSnapshot.value = {
        ...createEmptyWhiteboardSnapshot(),
        backgroundImage: normalizeBackgroundInstruction(props.backgroundImage),
      };
      markDrawingLayerDirty();
      redrawAll();
      return;
    }

    syncLocalStateFromSnapshot(snapshot);
  },
  { immediate: true },
);

watch(
  () => props.backgroundImage,
  (backgroundImage) => {
    if (backgroundImage === undefined) {
      return;
    }
    updateBackgroundImage(backgroundImage, true);
  },
  { immediate: true },
);

onMounted(() => {
  calculateStageSize();
  if (canvasWrapperRef.value) {
    resizeObserver = new ResizeObserver(() => {
      calculateStageSize();
    });
    resizeObserver.observe(canvasWrapperRef.value);
  }
  window.addEventListener("resize", calculateStageSize);
  redrawAll();
});

onBeforeUnmount(() => {
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  window.removeEventListener("resize", calculateStageSize);
  activeTouchPointerIds.clear();
  activeStroke.value = null;
  drawingPointerId.value = null;
});
</script>

<template>
  <v-card rounded="xl" elevation="8" class="whiteboard-shell h-100">
    <v-card-title
      v-if="props.showToolbar || props.title"
      class="whiteboard-toolbar-title d-flex justify-center"
      :class="{ 'py-2': !props.showToolbar }"
    >
      <div
        class="toolbar-wrap d-flex flex-wrap ga-3 align-center justify-center"
      >
        <v-chip
          v-if="props.title"
          color="primary"
          variant="tonal"
          size="small"
          class="title-chip mr-10"
        >
          {{ props.title }}
        </v-chip>

        <div v-if="props.showToolbar" class="d-flex flex-wrap ga-2">
          <v-btn
            v-for="color in WHITEBOARD_COLOR_OPTIONS"
            :key="color"
            class="color-chip"
            :class="{
              'color-chip--active':
                currentTool === 'pen' && currentColor === color,
            }"
            :style="{ backgroundColor: color }"
            size="small"
            rounded="circle"
            variant="flat"
            :title="`畫筆顏色 ${color}`"
            @click="selectPenColor(color)"
          />
        </div>

        <v-divider
          v-if="props.showToolbar"
          vertical
          class="mx-2 d-none d-md-flex"
        />

        <div v-if="props.showToolbar" class="brush-slider brush-slider--bottom">
          <v-slider
            :model-value="brushSizeValue"
            :min="4"
            :max="24"
            :step="1"
            thumb-label="always"
            hide-details
            color="primary"
            class="ma-0"
            @update:model-value="updateBrushSize"
          />
        </div>

        <v-btn
          v-if="props.showToolbar"
          color="amber-darken-2"
          :variant="currentTool === 'eraser' ? 'flat' : 'tonal'"
          @click="activateEraser"
        >
          <v-icon icon="mdi-eraser" start />
          橡皮擦
        </v-btn>

        <v-btn
          v-if="props.showToolbar"
          color="error"
          variant="tonal"
          @click="clearCanvas"
          >清除畫布</v-btn
        >
      </div>
    </v-card-title>

    <v-card-text class="whiteboard-content">
      <div ref="canvasWrapperRef" class="whiteboard-stage-host">
        <div class="whiteboard-stage" :style="stageStyle">
          <canvas
            ref="backgroundCanvasRef"
            class="whiteboard-canvas whiteboard-canvas--background"
            width="1280"
            height="720"
            aria-hidden="true"
          />
          <canvas
            ref="drawingCanvasRef"
            class="whiteboard-canvas whiteboard-canvas--drawing"
            :class="{
              'whiteboard-canvas--readonly': isReadOnlyMode,
            }"
            width="1280"
            height="720"
            @pointerdown="beginDrawing"
            @pointermove="continueDrawing"
            @pointerup="endDrawing"
            @pointercancel="endDrawing"
            @pointerleave="endDrawing"
          />
        </div>
      </div>
    </v-card-text>
  </v-card>
</template>

<style scoped>
.whiteboard-toolbar-title {
  min-height: 72px;
  overflow: visible;
}

.whiteboard-shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.whiteboard-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.whiteboard-stage-host {
  width: 100%;
  height: 100%;
  min-height: 0;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  overflow: hidden;
}

.whiteboard-stage {
  position: relative;
  border-radius: 20px;
  overflow: hidden;
  border: 1px solid rgba(33, 33, 33, 0.12);
  background: linear-gradient(135deg, #fafafa 0%, #f1f5f9 100%);
}

.whiteboard-canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
}

.whiteboard-canvas--background {
  pointer-events: none;
}

.whiteboard-canvas--drawing {
  touch-action: pinch-zoom;
  cursor:
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='18' height='18' viewBox='0 0 18 18'%3E%3Ccircle cx='9' cy='9' r='5' fill='none' stroke='%23000000' stroke-width='1.5'/%3E%3Ccircle cx='9' cy='9' r='1.2' fill='%23000000'/%3E%3C/svg%3E")
      9 9,
    crosshair;
}

.whiteboard-canvas--readonly {
  pointer-events: none;
  cursor: default;
}

.toolbar-wrap {
  width: 100%;
}

.title-chip {
  font-weight: 700;
}

.brush-slider {
  width: min(320px, 70vw);
}

.brush-slider--bottom {
  align-self: flex-end;
}

.color-chip {
  width: 32px;
  min-width: 32px;
  height: 32px;
  border: 2px solid rgba(255, 255, 255, 0.92);
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
}

.color-chip--active {
  outline: 3px solid rgba(33, 33, 33, 0.9);
  outline-offset: 2px;
}
</style>
