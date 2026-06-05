<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import RosterJoinCard from "../components/RosterJoinCard.vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import ReminderBoardView from "./ReminderBoardView.vue";
import { useAppVersion } from "../composables/useAppVersion";
import { createPeerConnection } from "../composables/usePeerConnection";
import type {
  ClassroomStatePayload,
  ClassroomStudent,
  SignalEnvelope,
  StudentFocusStatus,
} from "../types/session";
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
  type ActiveModule,
  type WhiteboardSnapshot,
  type WhiteboardSnapshotRequestMessage,
  type StudentFocusStatusMessage,
  type WhiteboardStudentEventBatchMessage,
  type WhiteboardTeacherStudentSnapshotMessage,
  type WhiteboardTeacherStudentEventBatchMessage,
  type WhiteboardSyncMessage,
  type WhiteboardTeacherBoardControlMessage,
  type ReminderBoardStateMessage,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const { appVersionLabel } = useAppVersion(props.baseUrl);

const STUDENT_BATCH_INTERVAL_MS = 33;
const STUDENT_BATCH_MAX_EVENTS = 24;
const STUDENT_FOCUS_STATUS_DEBOUNCE_MS = 250;
const STUDENT_AUTO_REJOIN_TTL_MS = 12 * 60 * 60 * 1000;
const STUDENT_AUTO_RECONNECT_DELAY_MS = 1200;
const STUDENT_REJOIN_STORAGE_KEY = "song-class.student.rejoin";
const STUDENT_BOARD_SNAPSHOT_STORAGE_KEY = "song-class.student.board-snapshot";
const STUDENT_BOARD_SNAPSHOT_TTL_MS = 12 * 60 * 60 * 1000;
const LESSON_SEND_HIGH_WATERMARK_BYTES = 512 * 1024;
const LESSON_SEND_LOW_WATERMARK_BYTES = 128 * 1024;
const LESSON_SEND_DRAIN_BUDGET_BYTES = 64 * 1024;

type PersistedStudentRejoinState = {
  classroomId: number;
  seatNoText: string;
  expiresAt: number;
};

type PersistedStudentBoardSnapshotState = {
  classroomId: number;
  seatNoText: string;
  snapshot: WhiteboardSnapshot;
  expiresAt: number;
};

const statusText = ref("尚未連線");
const isConnected = ref(false);
const signalError = ref("");
const classroomState = ref<ClassroomStatePayload | null>(null);
const activeMode = ref<WhiteboardMode>("home");
const activeTab = ref<WhiteboardBoardTab>("teacher-board");
const isTeacherBoardViewForced = ref(false);
const teacherBroadcastVideoRef = ref<HTMLVideoElement | null>(null);
const teacherBroadcastStream = ref<MediaStream | null>(null);
const modeVersion = ref(0);
const tabVersion = ref(0);
const quickQaQuestion = ref<QuickQaQuestion | null>(null);
const reminderBoardState = ref<
  import("../types/reminderBoard").ReminderBoard | null
>(null);

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
let focusStatusDebounceTimer: number | null = null;
let pendingFocusStatus: StudentFocusStatus | null = null;
let lastSentFocusStatus: StudentFocusStatus | null = null;
let classroomPollTimer: number | null = null;

let ws: WebSocket | null = null;
let peer: RTCPeerConnection | null = null;
let lessonChannel: RTCDataChannel | null = null;
const queuedLessonMessages: string[] = [];
let queuedLessonBytes = 0;
let lessonDrainTimer: number | null = null;
let selfId: string | undefined;
let teacherId: string | undefined;
let pendingJoinSelection: { classroomId: number; seatNoText: string } | null =
  null;
const queuedIceCandidates: RTCIceCandidateInit[] = [];
const isJoinRequested = ref(false);
const joinedSessionId = ref<string | null>(null);
const joinedClassroomId = ref<number | null>(null);
const joinedSeatNoText = ref("");
let reconnectTimer: number | null = null;

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
const hasJoinedClassroom = computed(
  () =>
    joinedClassroomId.value !== null &&
    joinedSeatNoText.value.trim().length > 0,
);

function stopReconnectTimer() {
  if (reconnectTimer !== null) {
    window.clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}

function clearPersistedJoinState() {
  try {
    window.localStorage.removeItem(STUDENT_REJOIN_STORAGE_KEY);
  } catch {
    // Ignore storage write failures.
  }
}

function clearPersistedStudentBoardSnapshot() {
  try {
    window.localStorage.removeItem(STUDENT_BOARD_SNAPSHOT_STORAGE_KEY);
  } catch {
    // Ignore storage write failures.
  }
}

function readPersistedJoinState(): PersistedStudentRejoinState | null {
  try {
    const raw = window.localStorage.getItem(STUDENT_REJOIN_STORAGE_KEY);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as Partial<PersistedStudentRejoinState>;
    if (
      typeof parsed.classroomId !== "number" ||
      !Number.isFinite(parsed.classroomId) ||
      parsed.classroomId <= 0
    ) {
      clearPersistedJoinState();
      return null;
    }

    if (typeof parsed.seatNoText !== "string" || !parsed.seatNoText.trim()) {
      clearPersistedJoinState();
      return null;
    }

    if (
      typeof parsed.expiresAt !== "number" ||
      !Number.isFinite(parsed.expiresAt) ||
      parsed.expiresAt <= Date.now()
    ) {
      clearPersistedJoinState();
      return null;
    }

    return {
      classroomId: parsed.classroomId,
      seatNoText: parsed.seatNoText,
      expiresAt: parsed.expiresAt,
    };
  } catch {
    clearPersistedJoinState();
    return null;
  }
}

function persistJoinState(classroomId: number, seatNoText: string) {
  const normalizedSeatNoText = seatNoText.trim();
  if (!normalizedSeatNoText) {
    clearPersistedJoinState();
    return;
  }

  const state: PersistedStudentRejoinState = {
    classroomId,
    seatNoText: normalizedSeatNoText,
    expiresAt: Date.now() + STUDENT_AUTO_REJOIN_TTL_MS,
  };

  try {
    window.localStorage.setItem(
      STUDENT_REJOIN_STORAGE_KEY,
      JSON.stringify(state),
    );
  } catch {
    // Ignore storage write failures.
  }
}

function persistStudentBoardSnapshot() {
  if (joinedClassroomId.value === null) {
    clearPersistedStudentBoardSnapshot();
    return;
  }

  const seatNoText = joinedSeatNoText.value.trim();
  if (!seatNoText) {
    clearPersistedStudentBoardSnapshot();
    return;
  }

  const state: PersistedStudentBoardSnapshotState = {
    classroomId: joinedClassroomId.value,
    seatNoText,
    snapshot: cloneWhiteboardSnapshot(studentSnapshot.value),
    expiresAt: Date.now() + STUDENT_BOARD_SNAPSHOT_TTL_MS,
  };

  try {
    window.localStorage.setItem(
      STUDENT_BOARD_SNAPSHOT_STORAGE_KEY,
      JSON.stringify(state),
    );
  } catch {
    // Ignore storage write failures.
  }
}

function readPersistedStudentBoardSnapshot(): PersistedStudentBoardSnapshotState | null {
  try {
    const raw = window.localStorage.getItem(STUDENT_BOARD_SNAPSHOT_STORAGE_KEY);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(
      raw,
    ) as Partial<PersistedStudentBoardSnapshotState>;
    if (
      typeof parsed.classroomId !== "number" ||
      !Number.isFinite(parsed.classroomId) ||
      parsed.classroomId <= 0
    ) {
      clearPersistedStudentBoardSnapshot();
      return null;
    }

    if (typeof parsed.seatNoText !== "string" || !parsed.seatNoText.trim()) {
      clearPersistedStudentBoardSnapshot();
      return null;
    }

    if (
      typeof parsed.expiresAt !== "number" ||
      !Number.isFinite(parsed.expiresAt) ||
      parsed.expiresAt <= Date.now()
    ) {
      clearPersistedStudentBoardSnapshot();
      return null;
    }

    const snapshot = parsed.snapshot;
    if (!snapshot || typeof snapshot !== "object") {
      clearPersistedStudentBoardSnapshot();
      return null;
    }

    return {
      classroomId: parsed.classroomId,
      seatNoText: parsed.seatNoText,
      snapshot: snapshot as WhiteboardSnapshot,
      expiresAt: parsed.expiresAt,
    };
  } catch {
    clearPersistedStudentBoardSnapshot();
    return null;
  }
}

function restorePersistedStudentBoardSnapshot() {
  const persisted = readPersistedStudentBoardSnapshot();
  if (!persisted) {
    return;
  }

  if (
    joinedClassroomId.value === null ||
    joinedClassroomId.value !== persisted.classroomId ||
    joinedSeatNoText.value.trim() !== persisted.seatNoText.trim()
  ) {
    return;
  }

  studentSnapshot.value = cloneWhiteboardSnapshot(persisted.snapshot);
}

function markJoinedSelection(classroomId: number, seatNoText: string) {
  const normalizedSeatNoText = seatNoText.trim();
  joinedClassroomId.value = classroomId;
  joinedSeatNoText.value = normalizedSeatNoText;
  persistJoinState(classroomId, normalizedSeatNoText);
}

function clearJoinedSelection() {
  joinedClassroomId.value = null;
  joinedSeatNoText.value = "";
}

function getActiveJoinSelection() {
  if (joinedClassroomId.value !== null && joinedSeatNoText.value.trim()) {
    return {
      classroomId: joinedClassroomId.value,
      seatNoText: joinedSeatNoText.value.trim(),
    };
  }

  const persisted = readPersistedJoinState();
  if (!persisted) {
    return null;
  }

  markJoinedSelection(persisted.classroomId, persisted.seatNoText);
  return {
    classroomId: persisted.classroomId,
    seatNoText: persisted.seatNoText,
  };
}

function queueJoinRequest(
  selection: { classroomId: number; seatNoText: string },
  pendingStatus = "連線建立中，準備送出加入請求...",
) {
  ensureSocket();

  if (
    isJoinRequested.value &&
    pendingJoinSelection?.classroomId === selection.classroomId &&
    pendingJoinSelection?.seatNoText === selection.seatNoText &&
    ws &&
    ws.readyState < WebSocket.CLOSED
  ) {
    return;
  }

  if (ws && ws.readyState === WebSocket.OPEN) {
    sendSignal({
      event: "join",
      payload: {
        classroom_id: selection.classroomId,
        seat_no_text: selection.seatNoText,
      },
    });
    isJoinRequested.value = true;
    return;
  }

  pendingJoinSelection = {
    classroomId: selection.classroomId,
    seatNoText: selection.seatNoText,
  };
  isJoinRequested.value = true;
  statusText.value = pendingStatus;
}

function scheduleAutoReconnect(delayMs = STUDENT_AUTO_RECONNECT_DELAY_MS) {
  if (reconnectTimer !== null) {
    return;
  }

  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;

    if (isConnected.value) {
      return;
    }

    const selection = getActiveJoinSelection();
    if (!selection) {
      return;
    }

    queueJoinRequest(selection, "連線已中斷，正在重新連線...");
  }, delayMs);
}

function restorePersistedJoinState() {
  const selection = getActiveJoinSelection();
  if (!selection) {
    return;
  }

  statusText.value = "正在恢復登入狀態...";
  signalError.value = "";
  queueJoinRequest(selection, "正在恢復登入狀態...");
}

function disposePeerConnection() {
  if (lessonChannel) {
    lessonChannel.onbufferedamountlow = null;
    lessonChannel.onopen = null;
    lessonChannel.onmessage = null;
    lessonChannel.onclose = null;
    lessonChannel.onerror = null;
    lessonChannel.close();
    lessonChannel = null;
  }

  if (peer) {
    peer.close();
    peer = null;
  }

  resetLessonSendQueue();
  queuedIceCandidates.length = 0;
}

function handleLessonTransportInterrupted(message: string) {
  if (!hasJoinedClassroom.value) {
    resetToJoinState(message);
    return;
  }

  persistStudentBoardSnapshot();

  isConnected.value = false;
  isJoinRequested.value = false;
  activeMode.value = "home";
  activeTab.value = "teacher-board";
  isTeacherBoardViewForced.value = false;
  modeVersion.value = 0;
  tabVersion.value = 0;
  teacherLastAppliedSequence.value = 0;
  teacherStudentLastAppliedSequence.value = 0;
  pendingFocusStatus = null;
  lastSentFocusStatus = null;
  stopStudentBatchTimer();
  stopFocusStatusDebounceTimer();
  setTeacherBroadcastStream(null);
  disposePeerConnection();
  teacherId = undefined;
  statusText.value = message;
  signalError.value = "";
  scheduleAutoReconnect();
}

function setTeacherBroadcastStream(stream: MediaStream | null) {
  teacherBroadcastStream.value = stream;

  const video = teacherBroadcastVideoRef.value;
  if (!video) {
    return;
  }

  video.srcObject = stream;
  if (stream) {
    void video.play().catch(() => {
      // Ignore autoplay restrictions on first render.
    });
  }
}

function fromActiveModule(activeModule: ActiveModule): WhiteboardMode {
  switch (activeModule) {
    case "home":
      return "home";
    case "contact_book_management":
      return "home";
    case "whiteboard":
      return "whiteboard";
    case "quick_qa":
      return "quick-qa";
    case "teacher_screen_broadcast":
      return "teacher-broadcast";
    case "student_points":
      return "home";
    case "reminder_board":
      return "reminder-board";
    case "reminder_settings":
      return "reminder-board";
    default:
      return "home";
  }
}

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

async function refreshClassroomStateFromApi() {
  try {
    const response = await fetch(
      new URL("/api/classroom/state", props.baseUrl),
    );
    if (!response.ok) {
      return;
    }

    const state = (await response.json()) as ClassroomStatePayload;
    classroomState.value = state;
  } catch {
    // Ignore temporary fetch failures; websocket/error UI will provide feedback.
  }
}

function stopClassroomPollTimer() {
  if (classroomPollTimer !== null) {
    window.clearInterval(classroomPollTimer);
    classroomPollTimer = null;
  }
}

function startClassroomPollTimer() {
  stopClassroomPollTimer();
  classroomPollTimer = window.setInterval(() => {
    if (isConnected.value) {
      return;
    }
    void refreshClassroomStateFromApi();
  }, 3000);
}

function sendSignal(payload: SignalEnvelope) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
  }
}

function estimateMessageBytes(raw: string) {
  // WebRTC DataChannel 文字訊息以 UTF-8 傳輸；以保守估算控制排程即可。
  return Math.max(1, raw.length * 2);
}

function stopLessonDrainTimer() {
  if (lessonDrainTimer !== null) {
    window.clearTimeout(lessonDrainTimer);
    lessonDrainTimer = null;
  }
}

function resetLessonSendQueue() {
  stopLessonDrainTimer();
  queuedLessonMessages.length = 0;
  queuedLessonBytes = 0;
}

function scheduleLessonDrain(delayMs = 0) {
  if (lessonDrainTimer !== null) {
    return;
  }

  lessonDrainTimer = window.setTimeout(() => {
    lessonDrainTimer = null;
    drainLessonMessages();
  }, delayMs);
}

function drainLessonMessages() {
  const channel = lessonChannel;
  if (!channel || channel.readyState !== "open") {
    return;
  }

  let sentBytes = 0;
  while (queuedLessonMessages.length > 0) {
    if (channel.bufferedAmount >= LESSON_SEND_HIGH_WATERMARK_BYTES) {
      return;
    }

    const raw = queuedLessonMessages.shift();
    if (raw === undefined) {
      break;
    }

    const bytes = estimateMessageBytes(raw);
    queuedLessonBytes = Math.max(0, queuedLessonBytes - bytes);
    channel.send(raw);
    sentBytes += bytes;

    if (sentBytes >= LESSON_SEND_DRAIN_BUDGET_BYTES) {
      break;
    }
  }

  if (queuedLessonMessages.length > 0) {
    scheduleLessonDrain(8);
  }
}

function sendLessonMessage(message: WhiteboardSyncMessage) {
  if (!lessonChannel || lessonChannel.readyState !== "open") {
    return;
  }

  const raw = JSON.stringify(message);
  queuedLessonMessages.push(raw);
  queuedLessonBytes += estimateMessageBytes(raw);
  drainLessonMessages();
}

function stopFocusStatusDebounceTimer() {
  if (focusStatusDebounceTimer !== null) {
    window.clearTimeout(focusStatusDebounceTimer);
    focusStatusDebounceTimer = null;
  }
}

function isBrowserInForeground() {
  return document.visibilityState === "visible" && document.hasFocus();
}

function resolveCurrentFocusStatus(): StudentFocusStatus {
  return isBrowserInForeground() ? "focused" : "away";
}

function sendFocusStatusNow(status: StudentFocusStatus) {
  if (!isConnected.value) {
    return;
  }

  const message: StudentFocusStatusMessage = {
    kind: "student-focus-status",
    studentId: selfId,
    status,
    updatedAt: Date.now(),
  };

  sendLessonMessage(message);
  lastSentFocusStatus = status;
}

function queueFocusStatus(status: StudentFocusStatus, force = false) {
  if (!isConnected.value) {
    return;
  }

  if (!force && status === lastSentFocusStatus) {
    return;
  }

  pendingFocusStatus = status;

  if (force) {
    stopFocusStatusDebounceTimer();
    const next = pendingFocusStatus;
    pendingFocusStatus = null;
    if (next) {
      sendFocusStatusNow(next);
    }
    return;
  }

  stopFocusStatusDebounceTimer();
  focusStatusDebounceTimer = window.setTimeout(() => {
    focusStatusDebounceTimer = null;
    const next = pendingFocusStatus;
    pendingFocusStatus = null;
    if (!next) {
      return;
    }
    sendFocusStatusNow(next);
  }, STUDENT_FOCUS_STATUS_DEBOUNCE_MS);
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
  stopReconnectTimer();
  isConnected.value = false;
  isJoinRequested.value = false;
  teacherId = undefined;
  selfId = undefined;
  joinedSessionId.value = null;
  clearJoinedSelection();
  clearPersistedJoinState();
  clearPersistedStudentBoardSnapshot();
  activeMode.value = "home";
  activeTab.value = "teacher-board";
  isTeacherBoardViewForced.value = false;
  modeVersion.value = 0;
  tabVersion.value = 0;
  teacherLastAppliedSequence.value = 0;
  teacherSnapshot.value = createEmptyWhiteboardSnapshot();
  studentSnapshot.value = createEmptyWhiteboardSnapshot();
  quickQaQuestion.value = null;
  setTeacherBroadcastStream(null);
  studentNextSequence = 1;
  teacherStudentLastAppliedSequence.value = 0;
  queuedStudentEvents.length = 0;
  resetLessonSendQueue();
  stopStudentBatchTimer();
  stopFocusStatusDebounceTimer();
  pendingFocusStatus = null;
  lastSentFocusStatus = null;
  disposePeerConnection();

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
    stopReconnectTimer();
    statusText.value = "已連上訊號服務";
    signalError.value = "";
    if (pendingJoinSelection) {
      sendSignal({
        event: "join",
        payload: {
          classroom_id: pendingJoinSelection.classroomId,
          seat_no_text: pendingJoinSelection.seatNoText,
        },
      });
      isJoinRequested.value = true;
      pendingJoinSelection = null;
    }
  };

  ws.onclose = () => {
    ws = null;

    if (!hasJoinedClassroom.value) {
      resetToJoinState("連線已中斷，請重新加入");
      return;
    }

    persistStudentBoardSnapshot();

    isConnected.value = false;
    isJoinRequested.value = false;
    disposePeerConnection();
    teacherId = undefined;
    statusText.value = "連線已中斷，正在重新連線...";
    signalError.value = "";
    scheduleAutoReconnect();
  };

  ws.onerror = () => {
    isConnected.value = false;
    signalError.value = "訊號服務連線失敗";

    if (hasJoinedClassroom.value) {
      persistStudentBoardSnapshot();
      scheduleAutoReconnect();
    }
  };

  ws.onmessage = async (event) => {
    try {
      const message = JSON.parse(event.data) as SignalEnvelope;

      if (message.event === "error") {
        if (
          message.message === "找不到可用目標端" &&
          hasJoinedClassroom.value &&
          !isConnected.value
        ) {
          statusText.value = "教師端尚未就緒，等待重新連線...";
          signalError.value = "";
          return;
        }

        signalError.value = message.message ?? "發生未知錯誤";
        return;
      }

      if (message.event === "teacher-ready") {
        teacherId = message.source;
        if (hasJoinedClassroom.value && !isConnected.value) {
          statusText.value = "教師端已就緒，重新建立連線中...";
          await startOffer();
        }
        return;
      }

      if (message.event === "classroom-state") {
        const payload = message.payload as { state?: ClassroomStatePayload };
        if (payload?.state) {
          classroomState.value = payload.state;
        }
        return;
      }

      if (message.event === "force-logout") {
        resetToJoinState(message.message ?? "班級已切換，請重新加入");
        signalError.value = "";
        return;
      }

      if (message.event === "joined") {
        selfId = message.source;
        joinedSessionId.value = message.source ?? null;
        if (pendingJoinSelection) {
          markJoinedSelection(
            pendingJoinSelection.classroomId,
            pendingJoinSelection.seatNoText,
          );
        }
        if (joinedClassroomId.value !== null && joinedSeatNoText.value.trim()) {
          persistJoinState(joinedClassroomId.value, joinedSeatNoText.value);
        }
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

      if (message.event === "offer" && message.payload && peer) {
        await peer.setRemoteDescription(
          message.payload as RTCSessionDescriptionInit,
        );
        await flushQueuedCandidates();
        const answer = await peer.createAnswer();
        await peer.setLocalDescription(answer);
        sendSignal({
          event: "answer",
          source: selfId,
          target: teacherId,
          payload: answer,
        });
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

  if (parsed.kind === "reminder-board-state") {
    const reminderBoardMessage = parsed as ReminderBoardStateMessage;
    reminderBoardState.value = reminderBoardMessage.board;
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
    activeMode.value = parsed.activeModule
      ? fromActiveModule(parsed.activeModule)
      : parsed.mode;
    if (parsed.mode === "teacher-broadcast") {
      activeTab.value = "teacher-board";
    }
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

  if (parsed.kind === "student-switch-board") {
    if (parsed.boardTab === "student-board") {
      activeMode.value = "whiteboard";
      activeTab.value = "student-board";
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

function pushStudentSnapshotToTeacherIfNotEmpty(
  reason: WhiteboardTeacherStudentSnapshotMessage["reason"] = "resync",
) {
  if (!isConnected.value) {
    return;
  }

  const hasDrawableStroke = studentSnapshot.value.strokes.some(
    (stroke) => stroke.points.length > 0,
  );
  if (!hasDrawableStroke) {
    return;
  }

  const snapshot = cloneWhiteboardSnapshot(studentSnapshot.value);
  snapshot.backgroundImage = null;

  const message: WhiteboardTeacherStudentSnapshotMessage = {
    kind: "teacher-student-snapshot",
    boardTab: "student-board",
    seq: Math.max(0, studentNextSequence - 1),
    reason,
    snapshot,
  };

  sendLessonMessage(message);
}

function bindLessonChannel(channel: RTCDataChannel) {
  lessonChannel = channel;
  lessonChannel.bufferedAmountLowThreshold = LESSON_SEND_LOW_WATERMARK_BYTES;

  lessonChannel.onopen = () => {
    isConnected.value = true;
    isJoinRequested.value = false;
    statusText.value = "已連線，請專心學習";
    signalError.value = "";
    restorePersistedStudentBoardSnapshot();
    pushStudentSnapshotToTeacherIfNotEmpty("resync");
    drainLessonMessages();
    requestTeacherSnapshot("join-init");
    queueFocusStatus(resolveCurrentFocusStatus(), true);
  };

  lessonChannel.onbufferedamountlow = () => {
    drainLessonMessages();
  };

  lessonChannel.onmessage = (event) => {
    try {
      handleLessonMessage(String(event.data));
    } catch (error) {
      signalError.value = `資料通道處理失敗: ${String(error)}`;
    }
  };

  lessonChannel.onclose = () => {
    resetLessonSendQueue();
    handleLessonTransportInterrupted("教師端連線中斷，等待重新連線...");
  };

  lessonChannel.onerror = () => {
    signalError.value = "資料通道發生錯誤";
  };
}

function handleStudentSyncEvent(payload: WhiteboardIncrementalEventPayload) {
  enqueueStudentEvent(payload);
}

function handleVisibilityChange() {
  if (joinedClassroomId.value !== null && joinedSeatNoText.value.trim()) {
    persistJoinState(joinedClassroomId.value, joinedSeatNoText.value);
    if (document.visibilityState === "hidden") {
      persistStudentBoardSnapshot();
    }
  }

  if (isConnected.value) {
    queueFocusStatus(resolveCurrentFocusStatus());
    return;
  }

  if (document.visibilityState === "visible" && hasJoinedClassroom.value) {
    scheduleAutoReconnect(0);
  }
}

function handleWindowForegroundChange() {
  if (!isConnected.value) {
    return;
  }

  queueFocusStatus(resolveCurrentFocusStatus());
}

function leaveClassroom() {
  stopReconnectTimer();
  pendingJoinSelection = null;
  isJoinRequested.value = false;
  signalError.value = "";
  clearPersistedStudentBoardSnapshot();
  closeSignalSocket();
  resetToJoinState("已離開教室");
}

async function startOffer() {
  if (!selfId) {
    statusText.value = "尚未加入教室";
    return;
  }

  if (!teacherId) {
    statusText.value = "等待教師端就緒";
    return;
  }

  disposePeerConnection();

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
        handleLessonTransportInterrupted("教師端連線中斷，等待重新連線...");
      }
    },
    onTrack: (event) => {
      const [stream] = event.streams;
      if (!stream) {
        return;
      }

      setTeacherBroadcastStream(stream);
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

function handleJoin(student: ClassroomStudent) {
  if (!classroomState.value) {
    signalError.value = "班級資料尚未載入";
    return;
  }

  if (student.occupied) {
    signalError.value = "該學生已連入，請選擇其他座號";
    return;
  }

  markJoinedSelection(
    classroomState.value.current_classroom.id,
    student.seat_no_text,
  );
  queueJoinRequest({
    classroomId: classroomState.value.current_classroom.id,
    seatNoText: student.seat_no_text,
  });
}

onMounted(() => {
  document.addEventListener("visibilitychange", handleVisibilityChange);
  window.addEventListener("focus", handleWindowForegroundChange);
  window.addEventListener("blur", handleWindowForegroundChange);
  void refreshClassroomStateFromApi();
  startClassroomPollTimer();
  restorePersistedJoinState();
});

watch([teacherBroadcastVideoRef, teacherBroadcastStream], () => {
  const video = teacherBroadcastVideoRef.value;
  const stream = teacherBroadcastStream.value;
  if (!video) {
    return;
  }

  if (video.srcObject !== stream) {
    video.srcObject = stream;
  }

  if (stream) {
    void video.play().catch(() => {
      // Ignore autoplay restrictions when user has not interacted.
    });
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("visibilitychange", handleVisibilityChange);
  window.removeEventListener("focus", handleWindowForegroundChange);
  window.removeEventListener("blur", handleWindowForegroundChange);
  stopClassroomPollTimer();
  stopReconnectTimer();

  resetLessonSendQueue();
  stopStudentBatchTimer();
  stopFocusStatusDebounceTimer();
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
          v-if="hasJoinedClassroom"
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
            <RosterJoinCard
              v-if="!hasJoinedClassroom"
              :classroom="classroomState?.current_classroom ?? null"
              :students="classroomState?.students ?? []"
              :loading="isJoinRequested"
              @submit="handleJoin"
            />

            <v-card
              v-else
              rounded="xl"
              elevation="8"
              class="d-flex align-center justify-center py-16"
            >
              <v-card-text class="text-center">
                <div class="text-display-large font-weight-black mb-2">
                  請專心學習
                </div>
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

      <div
        v-else-if="activeMode === 'reminder-board'"
        class="student-reminder-board-screen"
      >
        <ReminderBoardView
          class="student-reminder-board"
          :base-url="props.baseUrl"
          :readonly="true"
          :board="reminderBoardState"
        />
      </div>

      <div
        v-else-if="activeMode === 'teacher-broadcast'"
        class="student-broadcast-screen"
      >
        <div class="student-broadcast-shell">
          <div class="student-broadcast-overlay text-body-2 font-weight-bold">
            教師畫面廣播中
          </div>
          <video
            ref="teacherBroadcastVideoRef"
            class="teacher-broadcast-video"
            autoplay
            playsinline
            controls
          />
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

.student-broadcast-screen {
  height: 100%;
  max-height: 100%;
  padding: 10px;
  box-sizing: border-box;
  overflow: hidden;
}

.student-reminder-board-screen {
  height: 100%;
  max-height: 100%;
  padding: 10px;
  box-sizing: border-box;
  overflow: hidden;
}

.student-reminder-board {
  height: 100%;
}

.student-broadcast-shell {
  width: 100%;
  height: 100%;
  min-height: 0;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.student-broadcast-overlay {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 1;
  color: rgba(var(--v-theme-on-surface), 0.92);
  background: rgba(255, 255, 255, 0.72);
  padding: 6px 10px;
  border-radius: 999px;
}

.teacher-broadcast-video {
  width: 100%;
  height: 100%;
  min-height: 0;
  border-radius: 10px;
  background: #101418;
  object-fit: contain;
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
