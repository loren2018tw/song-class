<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch,
} from "vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import StudentListCard from "../components/StudentListCard.vue";
import { createPeerConnection } from "../composables/usePeerConnection";
import type { SignalEnvelope, StudentSession } from "../types/session";
import {
  cloneWhiteboardSnapshot,
  cloneWhiteboardStroke,
  createEmptyWhiteboardSnapshot,
  isWhiteboardSyncMessage,
  type WhiteboardBoardTab,
  type WhiteboardEventBatchMessage,
  type WhiteboardIncrementalEvent,
  type WhiteboardIncrementalEventPayload,
  type WhiteboardMode,
  type WhiteboardModeSyncMessage,
  type WhiteboardSnapshot,
  type WhiteboardSnapshotRequestMessage,
  type WhiteboardSnapshotSyncMessage,
  type WhiteboardStudentBoardControlMessage,
  type WhiteboardStudentEventBatchMessage,
  type WhiteboardSyncMessage,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const students = ref<StudentSession[]>([]);
const wsStatus = ref("尚未連線");
const rtcError = ref("");
const rtcErrorVisible = ref(false);
const activeFeature = ref<WhiteboardMode>("home");
const activeWhiteboardTab = ref<WhiteboardBoardTab>("teacher-board");
const modeVersion = ref(0);
const tabVersion = ref(0);

const whiteboardBackgroundOptions = [
  { fileName: null, displayName: "空白" },
  { fileName: "SixThinkingHats.png", displayName: "六頂思考帽" },
  { fileName: "english.png", displayName: "英文練習簿" },
  { fileName: "national-character.png", displayName: "生字練習" },
  { fileName: "staff.png", displayName: "五線譜" },
] as const;

const teacherBackground = ref<string | null>(null);
const studentBackground = ref<string | null>(null);

const teacherWhiteboardSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
const studentBoardSnapshots = reactive(new Map<string, WhiteboardSnapshot>());
const studentBoardLastSequence = reactive(new Map<string, number>());

const peers = new Map<string, RTCPeerConnection>();
const pendingCandidates = new Map<string, RTCIceCandidateInit[]>();
const lessonChannels = new Map<string, RTCDataChannel>();

const queuedTeacherEvents: WhiteboardIncrementalEvent[] = [];
let nextTeacherSequence = 1;
let teacherBatchFlushTimer: number | null = null;

let ws: WebSocket | null = null;

const BATCH_INTERVAL_MS = 33;
const BATCH_MAX_EVENTS = 24;

const wsUrl = computed(() => {
  const base = new URL(props.baseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=teacher";
  return base.toString();
});

const teacherBackgroundImage = computed(() => teacherBackground.value);
const studentBoardTiles = computed(() => {
  const collator = new Intl.Collator("zh-Hant", {
    numeric: true,
    sensitivity: "base",
  });

  return [...students.value]
    .sort((left, right) => collator.compare(left.nickname, right.nickname))
    .map((student) => ({
      id: student.connection_id,
      nickname: student.nickname,
      snapshot:
        studentBoardSnapshots.get(student.connection_id) ??
        createEmptyWhiteboardSnapshot(),
    }));
});
const studentGalleryRef = ref<HTMLDivElement | null>(null);
const studentTileHeightPx = ref(240);
const studentGalleryColumns = ref(1);

let studentGalleryResizeObserver: ResizeObserver | null = null;

const studentGalleryGridStyle = computed(() => ({
  gridTemplateColumns: `repeat(${studentGalleryColumns.value}, minmax(0, 1fr))`,
  gridAutoRows: `${Math.max(1, Math.floor(studentTileHeightPx.value))}px`,
}));

function recomputeStudentGalleryLayout() {
  const gallery = studentGalleryRef.value;
  const tileCount = studentBoardTiles.value.length;
  if (!gallery || tileCount === 0) {
    studentGalleryColumns.value = 1;
    studentTileHeightPx.value = 240;
    return;
  }

  const bounds = gallery.getBoundingClientRect();
  const availableWidth = Math.max(0, Math.floor(bounds.width));
  const availableHeight = Math.max(0, Math.floor(bounds.height));

  if (availableWidth === 0 || availableHeight === 0) {
    return;
  }

  const gap = 10;
  const cardAspectRatio = 4 / 3;

  let bestColumns = 1;
  let bestTileHeight = 1;

  for (let columns = 1; columns <= tileCount; columns += 1) {
    const rows = Math.ceil(tileCount / columns);
    const maxHeightByRows =
      (availableHeight - gap * (rows - 1)) / Math.max(1, rows);
    const maxWidthByColumns =
      (availableWidth - gap * (columns - 1)) / Math.max(1, columns);

    if (maxHeightByRows <= 0 || maxWidthByColumns <= 0) {
      continue;
    }

    const fittedTileHeight = Math.min(
      maxHeightByRows,
      maxWidthByColumns / cardAspectRatio,
    );

    if (fittedTileHeight > bestTileHeight) {
      bestTileHeight = fittedTileHeight;
      bestColumns = columns;
    }
  }

  studentGalleryColumns.value = bestColumns;
  studentTileHeightPx.value = Math.max(1, Math.floor(bestTileHeight));
}

function sendSignal(payload: SignalEnvelope) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
  }
}

function showRtcError(message: string) {
  rtcError.value = message;
  rtcErrorVisible.value = true;
}

function toModeMessage(): WhiteboardModeSyncMessage {
  return {
    kind: "mode-sync",
    mode: activeFeature.value,
    modeVersion: modeVersion.value,
    activeTab: activeWhiteboardTab.value,
    tabVersion: tabVersion.value,
  };
}

function toTeacherSnapshotMessage(
  reason: WhiteboardSnapshotSyncMessage["reason"],
): WhiteboardSnapshotSyncMessage {
  return {
    kind: "whiteboard-snapshot",
    modeVersion: modeVersion.value,
    tabVersion: tabVersion.value,
    boardTab: "teacher-board",
    seq: Math.max(0, nextTeacherSequence - 1),
    reason,
    snapshot: cloneWhiteboardSnapshot(teacherWhiteboardSnapshot.value),
  };
}

function toStudentBoardControlMessage(): WhiteboardStudentBoardControlMessage {
  return {
    kind: "student-board-control",
    action: "clear-all",
    tabVersion: tabVersion.value,
  };
}

function toStudentBoardBackgroundMessage(
  backgroundImage: string | null,
): WhiteboardStudentBoardControlMessage {
  return {
    kind: "student-board-control",
    action: "set-background",
    tabVersion: tabVersion.value,
    backgroundImage,
  };
}

function toPushTeacherStrokesMessage(): WhiteboardStudentBoardControlMessage {
  return {
    kind: "student-board-control",
    action: "replace-strokes",
    tabVersion: tabVersion.value,
    strokes: teacherWhiteboardSnapshot.value.strokes.map((stroke) =>
      cloneWhiteboardStroke(stroke),
    ),
  };
}

function sendToStudentChannel(
  studentId: string,
  message: WhiteboardSyncMessage,
) {
  const channel = lessonChannels.get(studentId);
  if (!channel || channel.readyState !== "open") {
    return;
  }
  channel.send(JSON.stringify(message));
}

function broadcastToLessonChannels(message: WhiteboardSyncMessage) {
  const raw = JSON.stringify(message);
  for (const channel of lessonChannels.values()) {
    if (channel.readyState === "open") {
      channel.send(raw);
    }
  }
}

function pushBootstrapToStudent(studentId: string) {
  sendToStudentChannel(studentId, toModeMessage());
  sendToStudentChannel(studentId, toTeacherSnapshotMessage("join"));
}

function flushQueuedTeacherEvents() {
  if (queuedTeacherEvents.length === 0) {
    return;
  }

  if (teacherBatchFlushTimer !== null) {
    window.clearTimeout(teacherBatchFlushTimer);
    teacherBatchFlushTimer = null;
  }

  const events = queuedTeacherEvents.splice(0, queuedTeacherEvents.length);
  const message: WhiteboardEventBatchMessage = {
    kind: "whiteboard-events-batch",
    modeVersion: modeVersion.value,
    tabVersion: tabVersion.value,
    boardTab: "teacher-board",
    startSeq: events[0].seq,
    endSeq: events[events.length - 1].seq,
    events,
  };

  broadcastToLessonChannels(message);
}

function scheduleTeacherBatchFlush() {
  if (teacherBatchFlushTimer !== null) {
    return;
  }

  teacherBatchFlushTimer = window.setTimeout(() => {
    teacherBatchFlushTimer = null;
    flushQueuedTeacherEvents();
  }, BATCH_INTERVAL_MS);
}

function enqueueTeacherIncrementalEvent(
  payload: WhiteboardIncrementalEventPayload,
) {
  queuedTeacherEvents.push({
    ...payload,
    seq: nextTeacherSequence,
    timestamp: Date.now(),
  });
  nextTeacherSequence += 1;

  if (queuedTeacherEvents.length >= BATCH_MAX_EVENTS) {
    flushQueuedTeacherEvents();
    return;
  }

  scheduleTeacherBatchFlush();
}

function applyFeatureMode(mode: WhiteboardMode) {
  if (activeFeature.value === mode) {
    return;
  }

  activeFeature.value = mode;
  modeVersion.value += 1;
  broadcastToLessonChannels(toModeMessage());
}

function applyWhiteboardTab(tab: WhiteboardBoardTab) {
  if (activeWhiteboardTab.value === tab) {
    return;
  }

  activeWhiteboardTab.value = tab;
  tabVersion.value += 1;
}

function onWhiteboardTabChanged(tab: unknown) {
  if (tab !== "teacher-board" && tab !== "student-board") {
    return;
  }

  applyWhiteboardTab(tab);
}

function activateHome() {
  applyFeatureMode("home");
}

function activateWhiteboard() {
  applyFeatureMode("whiteboard");
}

function activateTeacherBoardTab() {
  applyWhiteboardTab("teacher-board");
}

function activateStudentBoardTab() {
  applyWhiteboardTab("student-board");
}

function handleTeacherWhiteboardSnapshot(snapshot: WhiteboardSnapshot) {
  teacherWhiteboardSnapshot.value = cloneWhiteboardSnapshot(snapshot);
}

function handleTeacherWhiteboardSyncEvent(
  payload: WhiteboardIncrementalEventPayload,
) {
  enqueueTeacherIncrementalEvent(payload);
}

function ensureStroke(
  snapshot: WhiteboardSnapshot,
  event: WhiteboardIncrementalEvent,
) {
  if (event.type !== "stroke-point") {
    return null;
  }

  return (
    snapshot.strokes.find((stroke) => stroke.id === event.strokeId) ?? null
  );
}

function applyIncrementalEvent(
  snapshot: WhiteboardSnapshot,
  event: WhiteboardIncrementalEvent,
) {
  switch (event.type) {
    case "stroke-begin": {
      const exists = snapshot.strokes.some(
        (stroke) => stroke.id === event.stroke.id,
      );
      if (!exists) {
        snapshot.strokes.push(cloneWhiteboardStroke(event.stroke));
      }
      break;
    }
    case "stroke-point": {
      const stroke = ensureStroke(snapshot, event);
      if (!stroke) {
        throw new Error("missing-stroke");
      }
      stroke.points.push({ x: event.point.x, y: event.point.y });
      break;
    }
    case "stroke-end": {
      break;
    }
    case "clear": {
      snapshot.strokes = [];
      break;
    }
    case "background-change": {
      snapshot.backgroundImage = event.backgroundImage ?? null;
      snapshot.backgroundColor = event.backgroundColor;
      break;
    }
    case "tool-change": {
      break;
    }
  }
}

function setStudentSnapshot(studentId: string, snapshot: WhiteboardSnapshot) {
  studentBoardSnapshots.set(studentId, cloneWhiteboardSnapshot(snapshot));
}

function resetStudentBoardState(studentId: string) {
  const emptySnapshot = createEmptyWhiteboardSnapshot();
  if (studentBackground.value) {
    emptySnapshot.backgroundImage = studentBackground.value;
  }
  setStudentSnapshot(studentId, emptySnapshot);
  studentBoardLastSequence.set(studentId, 0);
}

function processStudentBatch(
  studentId: string,
  message: WhiteboardStudentEventBatchMessage,
) {
  const expectedStart = (studentBoardLastSequence.get(studentId) ?? 0) + 1;
  if (message.startSeq !== expectedStart) {
    resetStudentBoardState(studentId);
    showRtcError(`學生 ${studentId} 白板序號不連續，已重置該學生白板`);
    return;
  }

  const baseSnapshot =
    studentBoardSnapshots.get(studentId) ?? createEmptyWhiteboardSnapshot();
  const nextSnapshot = cloneWhiteboardSnapshot(baseSnapshot);

  try {
    let expectedSeq = expectedStart;
    for (const event of message.events) {
      if (event.seq !== expectedSeq) {
        resetStudentBoardState(studentId);
        showRtcError(`學生 ${studentId} 白板事件序號異常，已重置該學生白板`);
        return;
      }

      applyIncrementalEvent(nextSnapshot, event);
      expectedSeq += 1;
    }
  } catch {
    resetStudentBoardState(studentId);
    showRtcError(`學生 ${studentId} 白板事件重播失敗，已重置該學生白板`);
    return;
  }

  setStudentSnapshot(studentId, nextSnapshot);
  studentBoardLastSequence.set(studentId, message.endSeq);
}

function clearAllStudentBoards() {
  for (const student of students.value) {
    resetStudentBoardState(student.connection_id);
  }

  broadcastToLessonChannels(toStudentBoardControlMessage());
}

function pushTeacherDrawingToStudentBoards() {
  const teacherStrokes = teacherWhiteboardSnapshot.value.strokes.map((stroke) =>
    cloneWhiteboardStroke(stroke),
  );

  for (const student of students.value) {
    const currentSnapshot =
      studentBoardSnapshots.get(student.connection_id) ??
      createEmptyWhiteboardSnapshot();

    setStudentSnapshot(student.connection_id, {
      ...cloneWhiteboardSnapshot(currentSnapshot),
      strokes: teacherStrokes.map((stroke) => cloneWhiteboardStroke(stroke)),
    });

    studentBoardLastSequence.set(student.connection_id, 0);
  }

  broadcastToLessonChannels(toPushTeacherStrokesMessage());
}

function handleChannelMessage(studentId: string, raw: string) {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isWhiteboardSyncMessage(parsed)) {
      return;
    }

    if (parsed.kind === "snapshot-request") {
      const request = parsed as WhiteboardSnapshotRequestMessage;
      if (request.boardTab !== "teacher-board") {
        return;
      }

      const reason = request.reason === "join-init" ? "join" : "resync";
      sendToStudentChannel(studentId, toTeacherSnapshotMessage(reason));
      return;
    }

    if (parsed.kind === "student-events-batch") {
      const message = parsed as WhiteboardStudentEventBatchMessage;
      if (message.boardTab !== "student-board") {
        return;
      }

      if (message.senderId && message.senderId !== studentId) {
        showRtcError(`學生 ${studentId} 回傳的 senderId 不一致，已忽略`);
        return;
      }

      processStudentBatch(studentId, message);
    }
  } catch (error) {
    showRtcError(`資料通道訊息解析失敗: ${String(error)}`);
  }
}

function bindLessonChannel(studentId: string, channel: RTCDataChannel) {
  lessonChannels.set(studentId, channel);

  channel.onopen = () => {
    pushBootstrapToStudent(studentId);
  };

  channel.onmessage = (event) => {
    handleChannelMessage(studentId, String(event.data));
  };

  channel.onerror = () => {
    showRtcError(`學生 ${studentId} 資料通道發生錯誤`);
  };

  channel.onclose = () => {
    const current = lessonChannels.get(studentId);
    if (current === channel) {
      lessonChannels.delete(studentId);
    }
  };

  if (channel.readyState === "open") {
    pushBootstrapToStudent(studentId);
  }
}

function ensurePeer(studentId: string) {
  if (peers.has(studentId)) {
    return peers.get(studentId)!;
  }

  const peer = createPeerConnection({
    onIceCandidate: (candidate) => {
      sendSignal({
        event: "ice-candidate",
        target: studentId,
        payload: candidate,
      });
    },
    onDataChannel: (channel) => {
      bindLessonChannel(studentId, channel);
    },
  });

  peers.set(studentId, peer);
  return peer;
}

async function addCandidateSafely(
  studentId: string,
  candidate: RTCIceCandidateInit,
) {
  const peer = ensurePeer(studentId);
  if (!peer.remoteDescription) {
    const queue = pendingCandidates.get(studentId) ?? [];
    queue.push(candidate);
    pendingCandidates.set(studentId, queue);
    return;
  }

  await peer.addIceCandidate(candidate);
}

async function flushQueuedCandidates(studentId: string) {
  const queue = pendingCandidates.get(studentId);
  if (!queue || queue.length === 0) {
    return;
  }

  const peer = ensurePeer(studentId);
  if (!peer.remoteDescription) {
    return;
  }

  for (const candidate of queue) {
    await peer.addIceCandidate(candidate);
  }

  pendingCandidates.delete(studentId);
}

function disposeStudentConnection(studentId: string) {
  const peer = peers.get(studentId);
  if (peer) {
    peer.close();
    peers.delete(studentId);
  }

  const channel = lessonChannels.get(studentId);
  if (channel) {
    channel.close();
    lessonChannels.delete(studentId);
  }

  pendingCandidates.delete(studentId);
  studentBoardSnapshots.delete(studentId);
  studentBoardLastSequence.delete(studentId);
}

function reconcileStudentConnections(nextStudents: StudentSession[]) {
  const activeIds = new Set(
    nextStudents.map((student) => student.connection_id),
  );
  for (const studentId of peers.keys()) {
    if (!activeIds.has(studentId)) {
      disposeStudentConnection(studentId);
    }
  }

  for (const studentId of lessonChannels.keys()) {
    if (!activeIds.has(studentId)) {
      disposeStudentConnection(studentId);
    }
  }

  for (const student of nextStudents) {
    if (!studentBoardSnapshots.has(student.connection_id)) {
      resetStudentBoardState(student.connection_id);
    }
  }
}

async function handleSignal(message: SignalEnvelope) {
  if (message.event === "students" || message.event === "teacher-ready") {
    const payload = message.payload as
      | { students?: StudentSession[] }
      | undefined;
    const nextStudents = payload?.students ?? [];
    students.value = nextStudents;
    reconcileStudentConnections(nextStudents);
    return;
  }

  if (message.event === "offer" && message.source && message.payload) {
    try {
      const peer = ensurePeer(message.source);
      await peer.setRemoteDescription(
        message.payload as RTCSessionDescriptionInit,
      );
      await flushQueuedCandidates(message.source);
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      sendSignal({
        event: "answer",
        target: message.source,
        payload: answer,
      });
    } catch (error) {
      showRtcError(`教師端 WebRTC 錯誤: ${String(error)}`);
    }
    return;
  }

  if (message.event === "ice-candidate" && message.source && message.payload) {
    try {
      await addCandidateSafely(
        message.source,
        message.payload as RTCIceCandidateInit,
      );
    } catch (error) {
      showRtcError(`教師端 ICE 錯誤: ${String(error)}`);
    }
  }
}

function connectTeacherSocket() {
  if (ws) {
    ws.close();
  }

  ws = new WebSocket(wsUrl.value);
  wsStatus.value = "連線中";

  ws.onopen = () => {
    wsStatus.value = "已連線";
  };

  ws.onclose = () => {
    wsStatus.value = "已中斷";
  };

  ws.onerror = () => {
    wsStatus.value = "發生錯誤";
    showRtcError("WebSocket 連線發生錯誤");
  };

  ws.onmessage = async (event) => {
    const message = JSON.parse(event.data) as SignalEnvelope;
    await handleSignal(message);
  };
}

watch(studentBackground, (nextBackground) => {
  for (const student of students.value) {
    const currentSnapshot =
      studentBoardSnapshots.get(student.connection_id) ??
      createEmptyWhiteboardSnapshot();

    setStudentSnapshot(student.connection_id, {
      ...cloneWhiteboardSnapshot(currentSnapshot),
      backgroundImage: nextBackground ?? null,
    });
  }

  broadcastToLessonChannels(toStudentBoardBackgroundMessage(nextBackground));
});

watch(
  () => [studentBoardTiles.value.length, activeWhiteboardTab.value],
  async () => {
    await nextTick();
    recomputeStudentGalleryLayout();
  },
);

onMounted(() => {
  connectTeacherSocket();

  void nextTick(() => {
    recomputeStudentGalleryLayout();

    if (studentGalleryRef.value) {
      studentGalleryResizeObserver = new ResizeObserver(() => {
        recomputeStudentGalleryLayout();
      });
      studentGalleryResizeObserver.observe(studentGalleryRef.value);
    }
  });
});

onBeforeUnmount(() => {
  if (teacherBatchFlushTimer !== null) {
    window.clearTimeout(teacherBatchFlushTimer);
    teacherBatchFlushTimer = null;
  }

  if (ws) {
    ws.close();
  }

  for (const studentId of peers.keys()) {
    disposeStudentConnection(studentId);
  }

  peers.clear();
  lessonChannels.clear();
  pendingCandidates.clear();
  studentBoardSnapshots.clear();
  studentBoardLastSequence.clear();

  if (studentGalleryResizeObserver) {
    studentGalleryResizeObserver.disconnect();
    studentGalleryResizeObserver = null;
  }
});
</script>

<template>
  <v-app>
    <v-app-bar title="song-class(教師端)"></v-app-bar>
    <v-navigation-drawer :width="255">
      <div>
        <p class="text-medium-emphasis mb-0">WebSocket: {{ wsStatus }}</p>
      </div>
      <div class="d-flex flex-column ga-3 align-stretch">
        <v-btn
          color="secondary"
          :variant="activeFeature === 'home' ? 'flat' : 'outlined'"
          @click="activateHome"
        >
          首頁
        </v-btn>
        <v-btn
          color="primary"
          :variant="activeFeature === 'whiteboard' ? 'flat' : 'outlined'"
          @click="activateWhiteboard"
        >
          小白版
        </v-btn>
      </div>
      <div class="ma-2">
        <StudentListCard title="已連入學生" :students="students" />
      </div>
    </v-navigation-drawer>
    <v-main class="teacher-main">
      <div class="feature-main pa-3">
        <div v-if="activeFeature === 'whiteboard'" class="whiteboard-layout">
          <div class="whiteboard-canvas-wrap d-flex flex-column ga-3">
            <v-tabs
              color="primary"
              density="compact"
              :model-value="activeWhiteboardTab"
              @update:model-value="onWhiteboardTabChanged"
            >
              <v-tab value="teacher-board" @click="activateTeacherBoardTab">
                <v-icon icon="mdi-account" start />
                教師白板
              </v-tab>
              <v-tab value="student-board" @click="activateStudentBoardTab">
                <v-icon icon="mdi-account-multiple" start />
                學生白板
              </v-tab>
            </v-tabs>

            <div class="whiteboard-tab-panels">
              <div
                v-show="activeWhiteboardTab === 'teacher-board'"
                class="whiteboard-panel"
              >
                <WhiteboardCanvas
                  :snapshot="teacherWhiteboardSnapshot"
                  :background-image="teacherBackgroundImage"
                  @update:snapshot="handleTeacherWhiteboardSnapshot"
                  @sync-event="handleTeacherWhiteboardSyncEvent"
                />
              </div>

              <div
                v-show="activeWhiteboardTab === 'student-board'"
                class="whiteboard-panel"
              >
                <div
                  ref="studentGalleryRef"
                  class="student-gallery-grid"
                  :style="studentGalleryGridStyle"
                >
                  <div
                    v-for="tile in studentBoardTiles"
                    :key="tile.id"
                    class="student-gallery-item"
                  >
                    <WhiteboardCanvas
                      :title="tile.nickname"
                      :snapshot="tile.snapshot"
                      :show-toolbar="false"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <v-card rounded="lg" variant="outlined" class="image-tools-panel">
            <v-card-text class="d-flex flex-column ga-3">
              <v-switch
                color="success"
                density="compact"
                hide-details
                label="鎖定學生白板"
              />

              <v-btn
                color="error"
                variant="tonal"
                block
                @click="clearAllStudentBoards"
              >
                清空全部學生白板
              </v-btn>

              <v-btn
                color="primary"
                variant="tonal"
                block
                @click="pushTeacherDrawingToStudentBoards"
              >
                推送教師白板
              </v-btn>

              <v-select
                v-model="teacherBackground"
                label="教師背景"
                :items="whiteboardBackgroundOptions"
                item-title="displayName"
                item-value="fileName"
                density="compact"
                variant="outlined"
                hide-details
              />

              <v-select
                v-model="studentBackground"
                label="學生背景"
                :items="whiteboardBackgroundOptions"
                item-title="displayName"
                item-value="fileName"
                density="compact"
                variant="outlined"
                hide-details
              />
            </v-card-text>
          </v-card>
        </div>

        <v-card
          v-else
          rounded="xl"
          elevation="6"
          class="h-100 d-flex align-center justify-center"
        >
          <v-card-text class="text-center py-16">
            <div class="text-h5 font-weight-black mb-2">請專心學習</div>
            <div class="text-medium-emphasis">
              教師目前在首頁模式，可隨時切換到小白版。
            </div>
          </v-card-text>
        </v-card>
      </div>
    </v-main>

    <v-snackbar
      v-model="rtcErrorVisible"
      color="error"
      :timeout="4500"
      location="bottom right"
    >
      {{ rtcError }}
      <template #actions>
        <v-btn variant="text" @click="rtcErrorVisible = false">關閉</v-btn>
      </template>
    </v-snackbar>
  </v-app>
</template>

<style scoped>
.teacher-main {
  height: calc(100vh - 64px);
  overflow: hidden;
}

.feature-main {
  height: 100%;
  min-height: 0;
}

.whiteboard-layout {
  height: 100%;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 230px;
  gap: 12px;
}

.whiteboard-canvas-wrap {
  min-width: 0;
  min-height: 0;
  height: 100%;
}

.whiteboard-tab-panels {
  min-height: 0;
  height: 100%;
}

.whiteboard-panel {
  height: 100%;
  min-height: 0;
}

.student-gallery-grid {
  display: grid;
  gap: 10px;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.student-gallery-item {
  min-height: 0;
  height: 100%;
}

.image-tools-panel {
  height: 100%;
  overflow: auto;
}

@media (max-width: 960px) {
  .whiteboard-layout {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(0, 1fr) auto;
  }

  .image-tools-panel {
    height: auto;
  }
}
</style>
