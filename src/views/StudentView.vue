<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import NicknameJoinCard from "../components/NicknameJoinCard.vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import { useAppVersion } from "../composables/useAppVersion";
import { createPeerConnection } from "../composables/usePeerConnection";
import type { SignalEnvelope } from "../types/session";
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
  type WhiteboardSnapshot,
  type WhiteboardSnapshotRequestMessage,
  type WhiteboardStudentEventBatchMessage,
  type WhiteboardTeacherStudentEventBatchMessage,
  type WhiteboardTeacherStudentResyncRequestMessage,
  type WhiteboardTeacherStudentSnapshotMessage,
  type WhiteboardSyncMessage,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const { appVersionLabel } = useAppVersion(props.baseUrl);

const STUDENT_BATCH_INTERVAL_MS = 33;
const STUDENT_BATCH_MAX_EVENTS = 24;

const statusText = ref("尚未連線");
const isConnected = ref(false);
const signalError = ref("");
const activeMode = ref<WhiteboardMode>("home");
const activeTab = ref<WhiteboardBoardTab>("teacher-board");
const isTeacherBoardViewForced = ref(false);
const modeVersion = ref(0);
const tabVersion = ref(0);

const teacherSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
const teacherLastAppliedSequence = ref(0);

const studentSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
let studentNextSequence = 1;
const teacherStudentLastAppliedSequence = ref(0);
const queuedStudentEvents: WhiteboardIncrementalEvent[] = [];
let studentBatchFlushTimer: number | null = null;

let ws: WebSocket | null = null;
let peer: RTCPeerConnection | null = null;
let lessonChannel: RTCDataChannel | null = null;
let selfId: string | undefined;
let teacherId: string | undefined;
let pendingJoinNickname: string | null = null;
const queuedIceCandidates: RTCIceCandidateInit[] = [];

const wsUrl = computed(() => {
  const base = new URL(props.baseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=student";
  return base.toString();
});

function sendSignal(payload: SignalEnvelope) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
  }
}

function sendLessonMessage(message: WhiteboardSyncMessage) {
  if (!lessonChannel || lessonChannel.readyState !== "open") {
    return;
  }
  lessonChannel.send(JSON.stringify(message));
}

function requestTeacherSnapshot(
  reason: WhiteboardSnapshotRequestMessage["reason"],
) {
  sendLessonMessage({
    kind: "snapshot-request",
    boardTab: "teacher-board",
    reason,
    sinceSeq: teacherLastAppliedSequence.value,
  });
}

function requestTeacherStudentSnapshot(
  reason: WhiteboardTeacherStudentResyncRequestMessage["reason"],
) {
  sendLessonMessage({
    kind: "teacher-student-resync-request",
    boardTab: "student-board",
    reason,
    sinceSeq: teacherStudentLastAppliedSequence.value,
  });
}

function stopStudentBatchTimer() {
  if (studentBatchFlushTimer !== null) {
    window.clearTimeout(studentBatchFlushTimer);
    studentBatchFlushTimer = null;
  }
}

function flushStudentEvents() {
  if (queuedStudentEvents.length === 0) {
    return;
  }

  stopStudentBatchTimer();

  const events = queuedStudentEvents.splice(0, queuedStudentEvents.length);
  const message: WhiteboardStudentEventBatchMessage = {
    kind: "student-events-batch",
    senderId: selfId,
    tabVersion: tabVersion.value,
    boardTab: "student-board",
    startSeq: events[0].seq,
    endSeq: events[events.length - 1].seq,
    events,
  };

  sendLessonMessage(message);
}

function scheduleStudentBatchFlush() {
  if (studentBatchFlushTimer !== null) {
    return;
  }

  studentBatchFlushTimer = window.setTimeout(() => {
    studentBatchFlushTimer = null;
    flushStudentEvents();
  }, STUDENT_BATCH_INTERVAL_MS);
}

function enqueueStudentEvent(payload: WhiteboardIncrementalEventPayload) {
  if (
    activeMode.value !== "whiteboard" ||
    activeTab.value !== "student-board"
  ) {
    return;
  }

  queuedStudentEvents.push({
    ...payload,
    seq: studentNextSequence,
    timestamp: Date.now(),
  });

  studentNextSequence += 1;

  if (queuedStudentEvents.length >= STUDENT_BATCH_MAX_EVENTS) {
    flushStudentEvents();
    return;
  }

  scheduleStudentBatchFlush();
}

function onStudentTabChanged(tab: unknown) {
  if (isTeacherBoardViewForced.value) {
    activeTab.value = "teacher-board";
    return;
  }

  if (tab !== "teacher-board" && tab !== "student-board") {
    return;
  }

  activeTab.value = tab;
}

function resetToJoinState(message = "連線已中斷，請重新加入") {
  isConnected.value = false;
  teacherId = undefined;
  selfId = undefined;
  activeMode.value = "home";
  activeTab.value = "teacher-board";
  isTeacherBoardViewForced.value = false;
  modeVersion.value = 0;
  tabVersion.value = 0;
  teacherLastAppliedSequence.value = 0;
  teacherSnapshot.value = createEmptyWhiteboardSnapshot();
  studentSnapshot.value = createEmptyWhiteboardSnapshot();
  studentNextSequence = 1;
  teacherStudentLastAppliedSequence.value = 0;
  queuedStudentEvents.length = 0;
  stopStudentBatchTimer();
  queuedIceCandidates.length = 0;

  if (lessonChannel) {
    lessonChannel.close();
    lessonChannel = null;
  }

  if (peer) {
    peer.close();
    peer = null;
  }

  statusText.value = message;
}

async function addCandidateSafely(candidate: RTCIceCandidateInit) {
  if (!peer) {
    queuedIceCandidates.push(candidate);
    return;
  }

  if (!peer.remoteDescription) {
    queuedIceCandidates.push(candidate);
    return;
  }

  await peer.addIceCandidate(candidate);
}

async function flushQueuedCandidates() {
  if (!peer || !peer.remoteDescription) {
    return;
  }

  while (queuedIceCandidates.length > 0) {
    const candidate = queuedIceCandidates.shift();
    if (!candidate) {
      continue;
    }
    await peer.addIceCandidate(candidate);
  }
}

function ensureSocket() {
  if (ws && ws.readyState <= WebSocket.OPEN) {
    return;
  }

  ws = new WebSocket(wsUrl.value);

  ws.onopen = () => {
    statusText.value = "已連上訊號服務";
    signalError.value = "";
    if (pendingJoinNickname) {
      sendSignal({ event: "join", nickname: pendingJoinNickname });
      pendingJoinNickname = null;
    }
  };

  ws.onclose = () => {
    resetToJoinState("連線已中斷，請重新加入");
  };

  ws.onerror = () => {
    isConnected.value = false;
    signalError.value = "訊號服務連線失敗";
  };

  ws.onmessage = async (event) => {
    try {
      const message = JSON.parse(event.data) as SignalEnvelope;

      if (message.event === "error") {
        signalError.value = message.message ?? "發生未知錯誤";
        return;
      }

      if (message.event === "joined") {
        selfId = message.source;
        teacherId = message.target;
        statusText.value = "加入成功，建立 WebRTC 連線中...";
        await startOffer();
        return;
      }

      if (message.event === "answer" && message.payload && peer) {
        await peer.setRemoteDescription(
          message.payload as RTCSessionDescriptionInit,
        );
        await flushQueuedCandidates();
        statusText.value = "WebRTC 協商完成，等待連線建立...";
        return;
      }

      if (message.event === "ice-candidate" && message.payload) {
        await addCandidateSafely(message.payload as RTCIceCandidateInit);
      }
    } catch (error) {
      signalError.value = `訊號處理失敗: ${String(error)}`;
    }
  };
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

function applyTeacherBatch(message: WhiteboardEventBatchMessage) {
  const expectedStart = teacherLastAppliedSequence.value + 1;
  if (message.startSeq !== expectedStart) {
    requestTeacherSnapshot("seq-gap");
    return;
  }

  const nextSnapshot = cloneWhiteboardSnapshot(teacherSnapshot.value);

  try {
    let expectedSeq = expectedStart;
    for (const event of message.events) {
      if (event.seq !== expectedSeq) {
        requestTeacherSnapshot("seq-gap");
        return;
      }
      applyIncrementalEvent(nextSnapshot, event);
      expectedSeq += 1;
    }
  } catch {
    requestTeacherSnapshot("seq-gap");
    return;
  }

  teacherSnapshot.value = nextSnapshot;
  teacherLastAppliedSequence.value = message.endSeq;
}

function applyTeacherStudentBatch(
  message: WhiteboardTeacherStudentEventBatchMessage,
) {
  const expectedStart = teacherStudentLastAppliedSequence.value + 1;
  if (message.startSeq !== expectedStart) {
    requestTeacherStudentSnapshot("seq-gap");
    return;
  }

  const nextSnapshot = cloneWhiteboardSnapshot(studentSnapshot.value);

  try {
    let expectedSeq = expectedStart;
    for (const event of message.events) {
      if (event.seq !== expectedSeq) {
        requestTeacherStudentSnapshot("seq-gap");
        return;
      }

      applyIncrementalEvent(nextSnapshot, event);
      expectedSeq += 1;
    }
  } catch {
    requestTeacherStudentSnapshot("seq-gap");
    return;
  }

  studentSnapshot.value = nextSnapshot;
  teacherStudentLastAppliedSequence.value = message.endSeq;
}

function openTeacherAssignedUrl(rawUrl: string) {
  const trimmed = rawUrl.trim();
  if (!trimmed) {
    signalError.value = "教師傳送了無效網址";
    return;
  }

  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    signalError.value = "教師傳送了無效網址";
    return;
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    signalError.value = "教師傳送了不支援的網址協定";
    return;
  }

  signalError.value = "";
  try {
    window.open(parsed.toString(), "_blank", "noopener,noreferrer");
  } catch {
    signalError.value = "無法開啟新分頁，請允許瀏覽器彈出視窗";
  }
}

function handleLessonMessage(raw: string) {
  const parsed = JSON.parse(raw) as unknown;
  if (!isWhiteboardSyncMessage(parsed)) {
    return;
  }

  if (parsed.kind === "mode-sync") {
    if (parsed.modeVersion < modeVersion.value) {
      return;
    }

    if (parsed.tabVersion < tabVersion.value) {
      return;
    }

    modeVersion.value = parsed.modeVersion;
    tabVersion.value = parsed.tabVersion;
    activeMode.value = parsed.mode;
    return;
  }

  if (parsed.kind === "whiteboard-snapshot") {
    if (
      parsed.modeVersion < modeVersion.value ||
      parsed.tabVersion < tabVersion.value
    ) {
      return;
    }

    modeVersion.value = parsed.modeVersion;
    tabVersion.value = parsed.tabVersion;

    if (parsed.boardTab === "teacher-board") {
      teacherSnapshot.value = cloneWhiteboardSnapshot(parsed.snapshot);
      teacherLastAppliedSequence.value = parsed.seq;
      return;
    }

    if (parsed.boardTab === "student-board") {
      studentSnapshot.value = cloneWhiteboardSnapshot(parsed.snapshot);
      return;
    }
  }

  if (parsed.kind === "teacher-student-snapshot") {
    const message = parsed as WhiteboardTeacherStudentSnapshotMessage;
    if (message.boardTab !== "student-board") {
      return;
    }

    studentSnapshot.value = cloneWhiteboardSnapshot(message.snapshot);
    teacherStudentLastAppliedSequence.value = message.seq;
    return;
  }

  if (parsed.kind === "whiteboard-events-batch") {
    if (
      parsed.modeVersion < modeVersion.value ||
      parsed.tabVersion < tabVersion.value
    ) {
      return;
    }

    if (parsed.boardTab !== "teacher-board") {
      return;
    }

    modeVersion.value = parsed.modeVersion;
    tabVersion.value = parsed.tabVersion;
    applyTeacherBatch(parsed);
    return;
  }

  if (parsed.kind === "teacher-student-events-batch") {
    const message = parsed as WhiteboardTeacherStudentEventBatchMessage;
    if (message.boardTab !== "student-board") {
      return;
    }

    applyTeacherStudentBatch(message);
    return;
  }

  if (parsed.kind === "student-board-control") {
    if (parsed.action === "clear-all") {
      stopStudentBatchTimer();
      queuedStudentEvents.length = 0;
      studentNextSequence = 1;
      teacherStudentLastAppliedSequence.value = 0;
      studentSnapshot.value = {
        ...createEmptyWhiteboardSnapshot(),
        backgroundImage: studentSnapshot.value.backgroundImage ?? null,
      };
      return;
    }

    if (parsed.action === "set-background") {
      studentSnapshot.value = {
        ...cloneWhiteboardSnapshot(studentSnapshot.value),
        backgroundImage: parsed.backgroundImage ?? null,
      };
      return;
    }

    if (parsed.action === "replace-strokes") {
      const nextStrokes = (parsed.strokes ?? []).map((stroke) =>
        cloneWhiteboardStroke(stroke),
      );
      studentSnapshot.value = {
        ...cloneWhiteboardSnapshot(studentSnapshot.value),
        strokes: nextStrokes,
      };
    }

    return;
  }

  if (parsed.kind === "student-view-control") {
    isTeacherBoardViewForced.value = parsed.forceTeacherBoardView;

    if (parsed.forceTeacherBoardView) {
      activeMode.value = "whiteboard";
      activeTab.value = "teacher-board";
    }

    return;
  }

  if (parsed.kind === "student-open-url") {
    openTeacherAssignedUrl(parsed.url);
  }
}

function bindLessonChannel(channel: RTCDataChannel) {
  lessonChannel = channel;

  lessonChannel.onopen = () => {
    isConnected.value = true;
    statusText.value = "已連線，請專心學習";
    signalError.value = "";
    requestTeacherSnapshot("join-init");
    requestTeacherStudentSnapshot("join-init");
  };

  lessonChannel.onmessage = (event) => {
    try {
      handleLessonMessage(String(event.data));
    } catch (error) {
      signalError.value = `資料通道處理失敗: ${String(error)}`;
    }
  };

  lessonChannel.onclose = () => {
    resetToJoinState("資料通道已關閉，請重新加入");
  };

  lessonChannel.onerror = () => {
    signalError.value = "資料通道發生錯誤";
  };
}

function handleStudentSnapshot(snapshot: WhiteboardSnapshot) {
  studentSnapshot.value = cloneWhiteboardSnapshot(snapshot);
}

function handleStudentSyncEvent(payload: WhiteboardIncrementalEventPayload) {
  enqueueStudentEvent(payload);
}

async function startOffer() {
  if (!teacherId) {
    statusText.value = "等待教師端就緒";
    return;
  }

  peer = createPeerConnection({
    onIceCandidate: (candidate) => {
      sendSignal({
        event: "ice-candidate",
        source: selfId,
        target: teacherId,
        payload: candidate,
      });
    },
    onDataChannel: (channel) => {
      bindLessonChannel(channel);
    },
    onConnectionStateChange: (state) => {
      if (["disconnected", "failed", "closed"].includes(state)) {
        resetToJoinState("連線已中斷，請重新加入");
      }
    },
  });

  const channel = peer.createDataChannel("lesson");
  bindLessonChannel(channel);

  const offer = await peer.createOffer();
  await peer.setLocalDescription(offer);

  sendSignal({
    event: "offer",
    source: selfId,
    target: teacherId,
    payload: offer,
  });
}

function handleJoin(nickname: string) {
  ensureSocket();
  if (ws && ws.readyState === WebSocket.OPEN) {
    sendSignal({ event: "join", nickname });
  } else {
    pendingJoinNickname = nickname;
    statusText.value = "連線建立中，準備送出加入請求...";
  }
}

onBeforeUnmount(() => {
  stopStudentBatchTimer();
  if (ws) {
    ws.close();
  }
  if (lessonChannel) {
    lessonChannel.close();
  }
  if (peer) {
    peer.close();
  }
  lessonChannel = null;
  peer = null;
});
</script>

<template>
  <div class="student-view-root">
    <div class="app-version-chip">{{ appVersionLabel }}</div>
    <v-container
      v-if="!isConnected || activeMode !== 'whiteboard'"
      class="py-8"
    >
      <v-row justify="center">
        <v-col cols="12" md="8" lg="7">
          <NicknameJoinCard v-if="!isConnected" @submit="handleJoin" />

          <v-card
            v-else
            rounded="xl"
            elevation="8"
            class="d-flex align-center justify-center py-16"
          >
            <v-card-text class="text-center">
              <div class="text-h4 font-weight-black mb-2">請專心學習</div>
              <div class="text-medium-emphasis">等待教師切換課堂功能</div>
            </v-card-text>
          </v-card>

          <v-alert class="mt-4" type="info" variant="tonal">{{
            statusText
          }}</v-alert>
          <v-alert
            v-if="signalError"
            class="mt-3"
            type="error"
            variant="tonal"
            >{{ signalError }}</v-alert
          >
        </v-col>
      </v-row>
    </v-container>

    <div v-else class="student-whiteboard-screen">
      <v-tabs
        color="primary"
        density="compact"
        :disabled="isTeacherBoardViewForced"
        :model-value="activeTab"
        @update:model-value="onStudentTabChanged"
      >
        <v-tab value="teacher-board">
          <v-icon icon="mdi-account" start />
          教師白板
        </v-tab>
        <v-tab value="student-board">
          <v-icon icon="mdi-account-multiple" start />
          學生白板
        </v-tab>
      </v-tabs>

      <div
        v-show="activeTab === 'teacher-board'"
        class="student-whiteboard-canvas-wrap"
      >
        <WhiteboardCanvas
          title="教師白板"
          :snapshot="teacherSnapshot"
          :show-toolbar="false"
          class="student-whiteboard-canvas"
        />
      </div>

      <div
        v-show="activeTab === 'student-board'"
        class="student-whiteboard-canvas-wrap"
      >
        <WhiteboardCanvas
          title="學生白板"
          :snapshot="studentSnapshot"
          class="student-whiteboard-canvas"
          @update:snapshot="handleStudentSnapshot"
          @sync-event="handleStudentSyncEvent"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.student-view-root {
  position: relative;
}

.app-version-chip {
  position: fixed;
  top: 10px;
  right: 14px;
  z-index: 30;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 0.75rem;
  color: rgba(var(--v-theme-on-surface), 0.72);
  background: rgba(255, 255, 255, 0.72);
  pointer-events: none;
}

.student-whiteboard-screen {
  height: 100dvh;
  max-height: 100dvh;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  box-sizing: border-box;
  overflow: hidden;
}

.student-whiteboard-canvas-wrap {
  min-height: 0;
  flex: 1;
  height: 100%;
}

.student-whiteboard-canvas {
  height: 100%;
}
</style>
