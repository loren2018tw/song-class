<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import NicknameJoinCard from "../components/NicknameJoinCard.vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import { useAppVersion } from "../composables/useAppVersion";
import { createPeerConnection } from "../composables/usePeerConnection";
import type { SignalEnvelope } from "../types/session";
import {
  QUICK_QA_OPTIONS,
  cloneWhiteboardSnapshot,
  cloneWhiteboardStroke,
  createEmptyWhiteboardSnapshot,
  isWhiteboardSyncMessage,
  type QuickQaOption,
  type QuickQaQuestion,
  type WhiteboardBoardTab,
  type WhiteboardEventBatchMessage,
  type WhiteboardIncrementalEvent,
  type WhiteboardIncrementalEventPayload,
  type WhiteboardMode,
  type WhiteboardSnapshot,
  type WhiteboardSnapshotRequestMessage,
  type WhiteboardStudentEventBatchMessage,
  type WhiteboardTeacherStudentEventBatchMessage,
  type WhiteboardSyncMessage,
  type WhiteboardTeacherBoardControlMessage,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const { appVersionLabel } = useAppVersion(props.baseUrl);

const STUDENT_BATCH_INTERVAL_MS = 33;
const STUDENT_BATCH_MAX_EVENTS = 24;
const LAST_STUDENT_NICKNAME_STORAGE_KEY = "song-class:last-student-nickname";
const LAST_STUDENT_NICKNAME_TTL_MS = 12 * 60 * 60 * 1000;

type PersistedStudentNickname = {
  nickname: string;
  savedAt: number;
};

const statusText = ref("尚未連線");
const isConnected = ref(false);
const signalError = ref("");
const activeMode = ref<WhiteboardMode>("home");
const activeTab = ref<WhiteboardBoardTab>("teacher-board");
const isTeacherBoardViewForced = ref(false);
const modeVersion = ref(0);
const tabVersion = ref(0);
const quickQaQuestion = ref<QuickQaQuestion | null>(null);

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
const isJoinRequested = ref(false);
const lastJoinedNickname = ref(readLastStudentNickname());

const wsUrl = computed(() => {
  const base = new URL(props.baseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=student";
  return base.toString();
});

const studentSelectedOption = computed<QuickQaOption | null>(() => {
  if (!quickQaQuestion.value || !selfId) {
    return null;
  }

  return quickQaQuestion.value.answersByStudent[selfId] ?? null;
});
const quickQaFeedback = computed(() => {
  if (!quickQaQuestion.value || quickQaQuestion.value.status !== "closed") {
    return "";
  }

  const selected = studentSelectedOption.value;
  if (!selected) {
    return "作答已結束";
  }

  if (!quickQaQuestion.value.correctOption) {
    return "作答已結束";
  }

  return selected === quickQaQuestion.value.correctOption
    ? "恭喜答對"
    : "答錯再接再厲";
});
const quickQaAnswerDisabled = computed(
  () => !quickQaQuestion.value || quickQaQuestion.value.status !== "open",
);

function optionBadgeColor(option: QuickQaOption): string {
  switch (option) {
    case "A":
      return "primary";
    case "B":
      return "success";
    case "C":
      return "warning";
    case "D":
      return "error";
  }
}

function isPersistedNicknameExpired(savedAt: number): boolean {
  return Date.now() - savedAt > LAST_STUDENT_NICKNAME_TTL_MS;
}

function parsePersistedStudentNickname(
  raw: string,
): PersistedStudentNickname | null {
  try {
    const parsed = JSON.parse(raw) as Partial<PersistedStudentNickname>;
    if (
      typeof parsed.nickname !== "string" ||
      typeof parsed.savedAt !== "number" ||
      !Number.isFinite(parsed.savedAt)
    ) {
      return null;
    }

    return {
      nickname: parsed.nickname,
      savedAt: parsed.savedAt,
    };
  } catch {
    return null;
  }
}

function readLastStudentNickname(): string {
  try {
    const stored = localStorage.getItem(LAST_STUDENT_NICKNAME_STORAGE_KEY);
    if (!stored) {
      return "";
    }

    const parsed = parsePersistedStudentNickname(stored);
    if (parsed) {
      if (isPersistedNicknameExpired(parsed.savedAt)) {
        localStorage.removeItem(LAST_STUDENT_NICKNAME_STORAGE_KEY);
        return "";
      }

      return parsed.nickname;
    }

    // Backward compatibility: migrate legacy plain-string format.
    const legacyNickname = stored.trim();
    if (!legacyNickname) {
      localStorage.removeItem(LAST_STUDENT_NICKNAME_STORAGE_KEY);
      return "";
    }

    saveLastStudentNickname(legacyNickname);
    return legacyNickname;
  } catch {
    return "";
  }
}

function saveLastStudentNickname(nickname: string) {
  try {
    const payload: PersistedStudentNickname = {
      nickname,
      savedAt: Date.now(),
    };
    localStorage.setItem(
      LAST_STUDENT_NICKNAME_STORAGE_KEY,
      JSON.stringify(payload),
    );
  } catch {
    // Ignore storage failures and continue with in-memory value.
  }
}

function clearLastStudentNickname() {
  try {
    localStorage.removeItem(LAST_STUDENT_NICKNAME_STORAGE_KEY);
  } catch {
    // Ignore storage failures and continue with in-memory value.
  }
}

function closeSignalSocket() {
  if (!ws) {
    return;
  }

  ws.onopen = null;
  ws.onclose = null;
  ws.onerror = null;
  ws.onmessage = null;

  if (
    ws.readyState === WebSocket.OPEN ||
    ws.readyState === WebSocket.CONNECTING
  ) {
    ws.close();
  }

  ws = null;
}

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

  const nextSnapshot = cloneWhiteboardSnapshot(studentSnapshot.value);
  applyIncrementalEvent(nextSnapshot, {
    ...payload,
    seq: studentNextSequence,
    timestamp: Date.now(),
  });
  studentSnapshot.value = nextSnapshot;

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
  isJoinRequested.value = false;
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
  quickQaQuestion.value = null;
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

function submitQuickQaAnswer(option: QuickQaOption) {
  if (!quickQaQuestion.value || quickQaQuestion.value.status !== "open") {
    return;
  }

  sendLessonMessage({
    kind: "quick-qa-answer-submit",
    option,
    submittedAt: Date.now(),
  });
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
      isJoinRequested.value = true;
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
    // 共編學生白板採純事件流；遇到序號缺口時僅略過此批，等待後續事件。
    return;
  }

  const nextSnapshot = cloneWhiteboardSnapshot(studentSnapshot.value);

  try {
    let expectedSeq = expectedStart;
    for (const event of message.events) {
      if (event.seq !== expectedSeq) {
        return;
      }

      applyIncrementalEvent(nextSnapshot, event);
      expectedSeq += 1;
    }
  } catch {
    return;
  }

  studentSnapshot.value = nextSnapshot;
  teacherStudentLastAppliedSequence.value = message.endSeq;
}

function mergeTeacherSnapshotWithoutBackground(
  currentSnapshot: WhiteboardSnapshot,
  incomingSnapshot: WhiteboardSnapshot,
) {
  const nextSnapshot = cloneWhiteboardSnapshot(incomingSnapshot);
  nextSnapshot.backgroundImage = currentSnapshot.backgroundImage ?? null;
  nextSnapshot.backgroundColor = currentSnapshot.backgroundColor;
  return nextSnapshot;
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
      teacherSnapshot.value = mergeTeacherSnapshotWithoutBackground(
        teacherSnapshot.value,
        parsed.snapshot,
      );
      teacherLastAppliedSequence.value = parsed.seq;
      return;
    }

    if (parsed.boardTab === "student-board") {
      // 學生白板共編不套用快照，避免覆蓋本地事件流。
      return;
    }
  }

  if (parsed.kind === "teacher-board-control") {
    const message = parsed as WhiteboardTeacherBoardControlMessage;
    if (
      message.modeVersion < modeVersion.value ||
      message.tabVersion < tabVersion.value
    ) {
      return;
    }

    modeVersion.value = message.modeVersion;
    tabVersion.value = message.tabVersion;

    if (message.action === "set-background") {
      teacherSnapshot.value = {
        ...cloneWhiteboardSnapshot(teacherSnapshot.value),
        backgroundImage: message.backgroundImage ?? null,
        backgroundColor:
          message.backgroundColor ?? teacherSnapshot.value.backgroundColor,
      };
    }

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
    return;
  }

  if (parsed.kind === "quick-qa-state") {
    quickQaQuestion.value = parsed.question
      ? {
          ...parsed.question,
          options: { ...parsed.question.options },
          answersByStudent: { ...parsed.question.answersByStudent },
        }
      : null;
  }
}

function bindLessonChannel(channel: RTCDataChannel) {
  lessonChannel = channel;

  lessonChannel.onopen = () => {
    isConnected.value = true;
    isJoinRequested.value = false;
    statusText.value = "已連線，請專心學習";
    signalError.value = "";
    requestTeacherSnapshot("join-init");
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

function handleStudentSyncEvent(payload: WhiteboardIncrementalEventPayload) {
  enqueueStudentEvent(payload);
}

function attemptResumeReconnect(reason: string) {
  if (isConnected.value || isJoinRequested.value) {
    return;
  }

  if (document.visibilityState === "hidden") {
    return;
  }

  const persistedNickname = readLastStudentNickname();
  if (persistedNickname !== lastJoinedNickname.value) {
    lastJoinedNickname.value = persistedNickname;
  }

  const nickname = persistedNickname.trim();
  if (!nickname) {
    return;
  }

  statusText.value = `偵測到裝置恢復(${reason})，正在自動重連...`;
  handleJoin(nickname);
}

function handleVisibilityChange() {
  if (document.visibilityState === "visible") {
    attemptResumeReconnect("回到前景");
  }
}

function handlePageShow() {
  attemptResumeReconnect("頁面恢復");
}

function handleWindowFocus() {
  attemptResumeReconnect("取得焦點");
}

function handleNetworkOnline() {
  attemptResumeReconnect("網路恢復");
}

function leaveClassroom() {
  pendingJoinNickname = null;
  isJoinRequested.value = false;
  signalError.value = "";
  lastJoinedNickname.value = "";
  clearLastStudentNickname();
  closeSignalSocket();
  resetToJoinState("已離開教室");
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
  const trimmedNickname = nickname.trim();
  if (!trimmedNickname) {
    signalError.value = "請輸入有效暱稱";
    return;
  }

  lastJoinedNickname.value = trimmedNickname;
  saveLastStudentNickname(trimmedNickname);

  ensureSocket();

  if (
    isJoinRequested.value &&
    pendingJoinNickname === trimmedNickname &&
    ws &&
    ws.readyState < WebSocket.CLOSED
  ) {
    return;
  }

  if (ws && ws.readyState === WebSocket.OPEN) {
    sendSignal({ event: "join", nickname: trimmedNickname });
    isJoinRequested.value = true;
  } else {
    pendingJoinNickname = trimmedNickname;
    isJoinRequested.value = true;
    statusText.value = "連線建立中，準備送出加入請求...";
  }
}

onMounted(() => {
  document.addEventListener("visibilitychange", handleVisibilityChange);
  window.addEventListener("pageshow", handlePageShow);
  window.addEventListener("focus", handleWindowFocus);
  window.addEventListener("online", handleNetworkOnline);

  attemptResumeReconnect("頁面載入");
});

onBeforeUnmount(() => {
  document.removeEventListener("visibilitychange", handleVisibilityChange);
  window.removeEventListener("pageshow", handlePageShow);
  window.removeEventListener("focus", handleWindowFocus);
  window.removeEventListener("online", handleNetworkOnline);

  stopStudentBatchTimer();
  closeSignalSocket();
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
  <v-app>
    <v-app-bar title="song-class(學生端)">
      <template #append>
        <v-btn
          v-if="isConnected"
          class="mr-2"
          color="error"
          variant="outlined"
          size="small"
          @click="leaveClassroom"
        >
          離開教室
        </v-btn>
        <span class="app-version-text">{{ appVersionLabel }}</span>
      </template>
    </v-app-bar>

    <v-main class="student-main">
      <v-container v-if="!isConnected || activeMode === 'home'" class="py-8">
        <v-row justify="center">
          <v-col cols="12" md="8" lg="7">
            <NicknameJoinCard
              v-if="!isConnected"
              :initial-nickname="lastJoinedNickname"
              @submit="handleJoin"
            />

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

      <div
        v-else-if="activeMode === 'quick-qa'"
        class="student-quick-qa-screen"
      >
        <div class="student-quick-qa-shell">
          <div class="student-quick-qa-heading">
            <div class="text-h4 font-weight-black">快問快答</div>
            <div class="text-body-2 text-medium-emphasis">請選擇你的答案</div>
          </div>

          <v-card rounded="xl" elevation="6" class="pa-3 pa-md-6">
            <div class="text-medium-emphasis mb-4">
              {{ quickQaQuestion?.question || "教師口頭敘述題目中..." }}
            </div>

            <v-row>
              <v-col
                v-for="option in QUICK_QA_OPTIONS"
                :key="`student-quick-qa-${option}`"
                cols="12"
                sm="6"
              >
                <v-btn
                  rounded="lg"
                  block
                  class="student-answer-btn"
                  :color="optionBadgeColor(option)"
                  :variant="studentSelectedOption === option ? 'flat' : 'tonal'"
                  @click="submitQuickQaAnswer(option)"
                >
                  <div class="d-flex align-center ga-3 py-2 text-left w-100">
                    <v-avatar color="white" size="42">
                      <span class="font-weight-bold text-high-emphasis">{{
                        option
                      }}</span>
                    </v-avatar>
                    <div class="student-answer-btn-content">
                      <div class="text-body-1 font-weight-bold text-wrap">
                        {{
                          quickQaQuestion?.options[option] || "等待教師口頭敘述"
                        }}
                      </div>
                      <div class="text-caption mt-1">
                        {{
                          studentSelectedOption === option
                            ? "已選擇"
                            : "點擊作答"
                        }}
                      </div>
                    </div>
                    <v-icon
                      v-if="studentSelectedOption === option"
                      icon="mdi-check-circle"
                      size="22"
                      class="ml-auto"
                    />
                  </div>
                </v-btn>
              </v-col>
            </v-row>

            <v-alert
              v-if="quickQaFeedback"
              class="mt-4"
              :type="quickQaFeedback === '恭喜答對' ? 'success' : 'info'"
              variant="tonal"
            >
              {{ quickQaFeedback }}
            </v-alert>

            <div class="text-caption text-medium-emphasis mt-4">
              <template v-if="quickQaAnswerDisabled">作答已結束</template>
              <template v-else
                >作答進行中，可隨時更換答案，系統以最後一次為準。</template
              >
            </div>
          </v-card>
        </div>
      </div>

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
            @sync-event="handleStudentSyncEvent"
          />
        </div>
      </div>
    </v-main>
  </v-app>
</template>

<style scoped>
.app-version-text {
  font-size: 0.75rem;
  color: rgba(var(--v-theme-on-surface), 0.72);
  white-space: nowrap;
}

.student-main {
  height: calc(100vh - 64px);
  overflow: hidden;
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

.student-quick-qa-screen {
  min-height: 100dvh;
  padding: 14px;
}

.student-quick-qa-shell {
  max-width: 880px;
  margin: 0 auto;
}

.student-quick-qa-heading {
  margin-bottom: 12px;
}

.student-answer-btn {
  min-height: 92px;
  justify-content: flex-start;
  text-transform: none;
  letter-spacing: 0;
}

.student-answer-btn :deep(.v-btn__content) {
  width: 100%;
}

.student-answer-btn-content {
  min-width: 0;
}

@media (max-width: 960px) {
  .student-quick-qa-shell {
    max-width: 100%;
  }
}
</style>
