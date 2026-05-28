<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  createEmptyWhiteboardSnapshot,
  WHITEBOARD_CANVAS_HEIGHT,
  WHITEBOARD_CANVAS_WIDTH,
  WHITEBOARD_COLOR_OPTIONS,
  type WhiteboardPoint,
  type WhiteboardSnapshot,
  type WhiteboardStroke,
  type WhiteboardTool,
  type WhiteboardColor,
} from "../types/whiteboard";

const props = withDefaults(
  defineProps<{
    title?: string;
    snapshot?: WhiteboardSnapshot | null;
    backgroundImage?: string | null;
  }>(),
  {
    title: "",
  },
);

const emit = defineEmits<{
  (event: "update:snapshot", snapshot: WhiteboardSnapshot): void;
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
  return {
    version: snapshot.version,
    canvasWidth: snapshot.canvasWidth,
    canvasHeight: snapshot.canvasHeight,
    backgroundImage: snapshot.backgroundImage ?? null,
    backgroundColor: snapshot.backgroundColor,
    strokes: snapshot.strokes.map((stroke) => ({
      id: stroke.id,
      tool: stroke.tool,
      color: stroke.color,
      size: stroke.size,
      points: stroke.points.map((point) => ({ x: point.x, y: point.y })),
    })),
  };
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

function resolveBackgroundImageSource(imagePath: string | null | undefined) {
  if (!imagePath) {
    return null;
  }

  const trimmed = imagePath.trim();
  if (!trimmed) {
    return null;
  }

  return trimmed;
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

function syncLocalStateFromSnapshot(snapshot: WhiteboardSnapshot) {
  const previousSnapshot = currentSnapshot.value;
  const cloned = cloneSnapshot(snapshot);
  currentSnapshot.value = {
    ...cloned,
    backgroundImage: cloned.backgroundImage ?? null,
  };

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
    resolveBackgroundImageSource(imagePath);
  void redrawBackgroundCanvas();
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

function appendPoint(point: WhiteboardPoint) {
  const stroke = activeStroke.value;
  if (!stroke) {
    return;
  }

  stroke.points.push(point);
  redrawDrawingCanvas();
}

function beginDrawing(event: PointerEvent) {
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
  isDrawing.value = true;
  activeStroke.value = createStroke(currentTool.value);
  activeStroke.value.points.push(point);
  currentSnapshot.value.strokes.push(activeStroke.value);
  redrawDrawingCanvas();
}

function continueDrawing(event: PointerEvent) {
  if (!isDrawing.value) {
    return;
  }

  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }

  appendPoint(point);
}

function endDrawing(event: PointerEvent) {
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

  isDrawing.value = false;
  activeStroke.value = null;
  emitSnapshot();
}

function selectPenColor(color: WhiteboardColor) {
  currentTool.value = "pen";
  currentColor.value = color;
  emitSnapshot();
}

function selectBrushSize(size: number) {
  currentTool.value = "pen";
  currentSize.value = size;
  brushSizeValue.value = size;
  emitSnapshot();
}

function updateBrushSize(value: number) {
  const size = Math.max(4, Math.min(24, Math.round(value)));
  selectBrushSize(size);
}

function activateEraser() {
  currentTool.value = "eraser";
  emitSnapshot();
}

function clearCanvas() {
  currentSnapshot.value = {
    ...createEmptyWhiteboardSnapshot(),
    backgroundImage: currentSnapshot.value.backgroundImage ?? null,
  };
  activeStroke.value = null;
  isDrawing.value = false;
  redrawDrawingCanvas();
  emitSnapshot();
}

watch(
  () => props.snapshot,
  (snapshot) => {
    if (!snapshot) {
      currentSnapshot.value = {
        ...createEmptyWhiteboardSnapshot(),
        backgroundImage: resolveBackgroundImageSource(props.backgroundImage),
      };
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
  activeStroke.value = null;
});
</script>

<template>
  <v-card rounded="xl" elevation="8" class="whiteboard-shell h-100">
    <v-card-title class="d-flex justify-center">
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

        <div class="d-flex flex-wrap ga-2">
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

        <v-divider vertical class="mx-2 d-none d-md-flex" />

        <div class="brush-slider">
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
          color="amber-darken-2"
          :variant="currentTool === 'eraser' ? 'flat' : 'tonal'"
          @click="activateEraser"
        >
          <v-icon icon="mdi-eraser" start />
          橡皮擦
        </v-btn>

        <v-btn color="error" variant="tonal" @click="clearCanvas"
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
            width="1280"
            height="720"
            @pointerdown.prevent="beginDrawing"
            @pointermove.prevent="continueDrawing"
            @pointerup.prevent="endDrawing"
            @pointercancel.prevent="endDrawing"
            @pointerleave.prevent="endDrawing"
          />
        </div>
      </div>
    </v-card-text>
  </v-card>
</template>

<style scoped>
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
  touch-action: none;
  cursor: crosshair;
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
