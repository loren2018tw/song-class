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
import QrcodeVue from "qrcode.vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import ContactBookManager from "../components/ContactBookManager.vue";
import StudentListCard from "../components/StudentListCard.vue";
import { useAppVersion } from "../composables/useAppVersion";
import { createPeerConnection } from "../composables/usePeerConnection";
import type {
  ClassroomStatePayload,
  SignalEnvelope,
  StudentFocusStatus,
  StudentSession,
} from "../types/session";
import {
  QUICK_QA_OPTIONS,
  WHITEBOARD_CANVAS_HEIGHT,
  WHITEBOARD_CANVAS_WIDTH,
  cloneWhiteboardSnapshot,
  cloneWhiteboardStroke,
  createEmptyQuickQaOptions,
  createQuickQaOptionStats,
  createEmptyWhiteboardSnapshot,
  isWhiteboardSyncMessage,
  type QuickQaOption,
  type QuickQaQuestion,
  type QuickQaStateMessage,
  type ActiveModule,
  type WhiteboardBoardTab,
  type WhiteboardEventBatchMessage,
  type WhiteboardIncrementalEvent,
  type WhiteboardIncrementalEventPayload,
  type WhiteboardMode,
  type WhiteboardModeSyncMessage,
  type WhiteboardSnapshot,
  type WhiteboardSnapshotRequestMessage,
  type WhiteboardSnapshotSyncMessage,
  type WhiteboardTeacherBoardControlMessage,
  type WhiteboardStudentBoardControlMessage,
  type WhiteboardStudentSwitchBoardMessage,
  type WhiteboardStudentEventBatchMessage,
  type WhiteboardTeacherStudentEventBatchMessage,
  type WhiteboardStudentViewControlMessage,
  type WhiteboardSyncMessage,
  type StudentFocusStatusMessage,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const { appVersionLabel } = useAppVersion(props.baseUrl);

const students = ref<StudentSession[]>([]);
const currentClassroomName = ref("載入中");
const currentClassroomId = ref<number | null>(null);
const wsStatus = ref("尚未連線");
const rtcError = ref("");
const rtcErrorVisible = ref(false);
const countdownMinutesInput = ref(0);
const countdownSecondsInput = ref(0);
const countdownRemainingSeconds = ref(0);
const countdownRunning = ref(false);
const countdownDoneSnackbarVisible = ref(false);
const studentQrDialogVisible = ref(false);
const openUrlDialogVisible = ref(false);
const studentOpenUrlInput = ref("");
const studentOpenUrlError = ref("");
const activeFeature = ref<WhiteboardMode>("home");
const activeWhiteboardTab = ref<WhiteboardBoardTab>("teacher-board");
const forceTeacherBoardView = ref(false);
const modeVersion = ref(0);
const tabVersion = ref(0);
const quickQaQuestionInput = ref("");
const quickQaOptionInputs = reactive<Record<QuickQaOption, string>>(
  createEmptyQuickQaOptions(),
);
const quickQaQuestion = ref<QuickQaQuestion | null>(null);
const quickQaResultView = ref<"summary" | "details">("summary");
const quickQaCloseDialogVisible = ref(false);
const quickQaStageCorrectCounts = reactive(new Map<string, number>());
const knownStudentNames = reactive(new Map<string, string>());
const studentFocusStatusById = reactive(new Map<string, StudentFocusStatus>());
const studentFocusUpdatedAtById = reactive(new Map<string, number>());
const teacherBroadcastStarting = ref(false);
const teacherBroadcastStream = ref<MediaStream | null>(null);

const whiteboardBackgroundOptions = [
  { fileName: null, displayName: "空白" },
  { fileName: "SixThinkingHats.png", displayName: "六頂思考帽" },
  { fileName: "english.png", displayName: "英文練習簿" },
  { fileName: "national-character.png", displayName: "生字練習" },
  { fileName: "staff.png", displayName: "五線譜" },
] as const;

const teacherBackground = ref<string | null>(null);
const studentBackground = ref<string | null>(null);
const downloadingStudentBoardsPdf = ref(false);

type WhiteboardCanvasExposed = {
  getFlattenedDrawingLayer: () => {
    imageDataUrl: string;
    width: number;
    height: number;
    updatedAt: number;
  } | null;
};

const teacherWhiteboardCanvasRef = ref<WhiteboardCanvasExposed | null>(null);

const teacherWhiteboardSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
const studentBoardSnapshots = reactive(new Map<string, WhiteboardSnapshot>());
const studentBoardLastSequence = reactive(new Map<string, number>());
const teacherToStudentLastSequence = reactive(new Map<string, number>());
const teacherToStudentNextSequence = reactive(new Map<string, number>());
const queuedTeacherToStudentEvents = reactive(
  new Map<string, WhiteboardIncrementalEvent[]>(),
);
const teacherToStudentBatchFlushTimers = new Map<string, number>();

const coeditDialogVisible = ref(false);
const coeditStudentId = ref<string | null>(null);

const peers = new Map<string, RTCPeerConnection>();
const pendingCandidates = new Map<string, RTCIceCandidateInit[]>();
const lessonChannels = new Map<string, RTCDataChannel>();
const teacherBroadcastSenders = new Map<string, RTCRtpSender>();

const queuedTeacherEvents: WhiteboardIncrementalEvent[] = [];
let nextTeacherSequence = 1;
let teacherBatchFlushTimer: number | null = null;

let ws: WebSocket | null = null;
let countdownTimerId: number | null = null;
let countdownAudioContext: AudioContext | null = null;

const BATCH_INTERVAL_MS = 33;
const BATCH_MAX_EVENTS = 24;

function isStudentFocusStatus(value: unknown): value is StudentFocusStatus {
  return value === "focused" || value === "away";
}

function normalizeStudentFocusStatus(value: unknown): StudentFocusStatus {
  return isStudentFocusStatus(value) ? value : "focused";
}

function normalizeStudentFocusUpdatedAt(value: unknown, fallback = 0): number {
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return value;
  }

  return fallback;
}

function mergeStudentsWithFocus(
  nextStudents: StudentSession[],
): StudentSession[] {
  return nextStudents.map((student) => {
    const previousStatus = studentFocusStatusById.get(student.connection_id);
    const previousUpdatedAt =
      studentFocusUpdatedAtById.get(student.connection_id) ?? 0;
    const focusStatus = normalizeStudentFocusStatus(
      student.focus_status ?? previousStatus,
    );
    const focusUpdatedAt = normalizeStudentFocusUpdatedAt(
      student.focus_updated_at,
      previousUpdatedAt,
    );

    studentFocusStatusById.set(student.connection_id, focusStatus);
    studentFocusUpdatedAtById.set(student.connection_id, focusUpdatedAt);

    return {
      ...student,
      focus_status: focusStatus,
      focus_updated_at: focusUpdatedAt,
    };
  });
}

function applyStudentFocusStatus(
  studentId: string,
  status: StudentFocusStatus,
  updatedAt: number,
) {
  studentFocusStatusById.set(studentId, status);
  studentFocusUpdatedAtById.set(studentId, updatedAt);

  students.value = students.value.map((student) => {
    if (student.connection_id !== studentId) {
      return student;
    }

    return {
      ...student,
      focus_status: status,
      focus_updated_at: updatedAt,
    };
  });
}

const wsUrl = computed(() => {
  const base = new URL(props.baseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=teacher";
  return base.toString();
});
const countdownPauseResumeIcon = computed(() =>
  countdownRunning.value ? "mdi-pause" : "mdi-play",
);
const countdownPauseResumeTooltip = computed(() =>
  countdownRunning.value
    ? "暫停"
    : countdownRemainingSeconds.value > 0
      ? "繼續"
      : "開始",
);
const countdownMinutePresets = [1, 5, 10, 15] as const;
const teacherBroadcastActive = computed(
  () => activeFeature.value === "teacher-broadcast",
);
const studentJoinUrl = computed(() => {
  return new URL("/student", props.baseUrl).toString();
});
const teacherBroadcastCaptureSupportError = computed(() => {
  if (!window.isSecureContext) {
    return "教師廣播需要在安全來源下執行，請改從 localhost 開啟教師端。";
  }

  if (!navigator.mediaDevices?.getDisplayMedia) {
    return "目前瀏覽器環境不支援螢幕擷取。";
  }

  return "";
});

function toActiveModule(mode: WhiteboardMode): ActiveModule {
  switch (mode) {
    case "home":
      return "home";
    case "whiteboard":
      return "whiteboard";
    case "quick-qa":
      return "quick_qa";
    case "teacher-broadcast":
      return "teacher_screen_broadcast";
    case "contact-book":
      return "contact_book_management";
  }
}

function toBroadcastCaptureConstraints() {
  return {
    width: { ideal: 1280, max: 1280 },
    height: { ideal: 720, max: 720 },
    frameRate: { ideal: 15, max: 15 },
  };
}

async function openStudentQrDialogAndCopyUrl() {
  studentQrDialogVisible.value = true;

  await copyStudentJoinUrl();
}

function copyTextWithExecCommand(text: string): boolean {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  textarea.style.pointerEvents = "none";
  textarea.style.zIndex = "-1";
  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();

  let copied = false;
  try {
    copied = document.execCommand("copy");
  } finally {
    document.body.removeChild(textarea);
  }

  return copied;
}

async function copyStudentJoinUrl() {
  try {
    await navigator.clipboard.writeText(studentJoinUrl.value);
    return;
  } catch {
    if (copyTextWithExecCommand(studentJoinUrl.value)) {
      return;
    }
  }

  showRtcError("無法複製學生端連結，請手動複製。");
}

const quickQaStats = computed(() =>
  createQuickQaOptionStats(quickQaQuestion.value),
);
const quickQaTotalAnswers = computed(
  () => Object.keys(quickQaQuestion.value?.answersByStudent ?? {}).length,
);
const quickQaIsOpen = computed(() => quickQaQuestion.value?.status === "open");
const quickQaCanClose = computed(
  () =>
    quickQaQuestion.value !== null && quickQaQuestion.value.status === "open",
);
const quickQaEditorLocked = computed(
  () => quickQaQuestion.value?.status === "open",
);
const quickQaHasQuestion = computed(() => quickQaQuestion.value !== null);
const quickQaDetailsByOption = computed(() =>
  quickQaStats.value.map((stat) => ({
    ...stat,
    students: stat.studentIds.map((studentId) => ({
      id: studentId,
      nickname: getStudentDisplayName(studentId),
    })),
  })),
);
const studentNameCollator = new Intl.Collator("zh-Hant", {
  numeric: true,
  sensitivity: "base",
});

function seatNicknameSortKey(student: StudentSession): string {
  const seat = student.seat_no_text?.trim() ?? "";
  const rawNickname = student.nickname ?? "";

  if (!seat) {
    return rawNickname.trim();
  }

  const nicknameWithoutSeat = rawNickname.startsWith(seat)
    ? rawNickname.slice(seat.length).trim()
    : rawNickname.trim();
  return `${seat}${nicknameWithoutSeat}`;
}

const sortedStudentsForChipList = computed(() =>
  [...students.value].sort((left, right) =>
    studentNameCollator.compare(
      seatNicknameSortKey(left),
      seatNicknameSortKey(right),
    ),
  ),
);
const quickQaStudentStatuses = computed(() => {
  return [...students.value]
    .sort((left, right) =>
      studentNameCollator.compare(left.nickname, right.nickname),
    )
    .map((student) => ({
      id: student.connection_id,
      nickname: student.nickname,
      answered:
        quickQaQuestion.value?.answersByStudent[student.connection_id] !==
        undefined,
    }));
});
const quickQaLeaderboardTop10 = computed(() => {
  return [...quickQaStageCorrectCounts.entries()]
    .map(([studentId, score]) => ({
      studentId,
      nickname: getStudentDisplayName(studentId),
      score,
    }))
    .sort((left, right) => {
      if (right.score !== left.score) {
        return right.score - left.score;
      }
      return left.nickname.localeCompare(right.nickname, "zh-Hant");
    })
    .slice(0, 10);
});

const teacherBackgroundImage = computed(() => teacherBackground.value);
const studentBoardTiles = computed(() => {
  return [...students.value]
    .sort((left, right) =>
      studentNameCollator.compare(left.nickname, right.nickname),
    )
    .map((student) => ({
      id: student.connection_id,
      nickname: student.nickname,
      snapshot:
        studentBoardSnapshots.get(student.connection_id) ??
        createEmptyWhiteboardSnapshot(),
    }));
});
const coeditStudent = computed(() => {
  if (!coeditStudentId.value) {
    return null;
  }

  return (
    students.value.find(
      (student) => student.connection_id === coeditStudentId.value,
    ) ?? null
  );
});
const coeditStudentSnapshot = computed(() => {
  if (!coeditStudentId.value) {
    return createEmptyWhiteboardSnapshot();
  }

  return (
    studentBoardSnapshots.get(coeditStudentId.value) ??
    createEmptyWhiteboardSnapshot()
  );
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

async function renegotiatePeer(studentId: string) {
  const peer = peers.get(studentId);
  if (!peer || peer.signalingState !== "stable") {
    return;
  }

  try {
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    sendSignal({
      event: "offer",
      target: studentId,
      payload: offer,
    });
  } catch (error) {
    showRtcError(`重新協商失敗(${studentId}): ${String(error)}`);
  }
}

function attachBroadcastTrackToStudent(studentId: string) {
  const peer = peers.get(studentId);
  const stream = teacherBroadcastStream.value;
  const track = stream?.getVideoTracks()[0];
  if (!peer || !stream || !track) {
    return;
  }

  const existingSender = teacherBroadcastSenders.get(studentId);
  if (existingSender) {
    void existingSender.replaceTrack(track);
    return;
  }

  const sender = peer.addTrack(track, stream);
  teacherBroadcastSenders.set(studentId, sender);
  void renegotiatePeer(studentId);
}

function stopTeacherBroadcastTrack() {
  const stream = teacherBroadcastStream.value;
  if (!stream) {
    return;
  }

  for (const track of stream.getTracks()) {
    track.stop();
  }
  teacherBroadcastStream.value = null;
}

function detachBroadcastFromAllPeers() {
  for (const [studentId, sender] of teacherBroadcastSenders.entries()) {
    const peer = peers.get(studentId);
    if (!peer) {
      continue;
    }

    try {
      peer.removeTrack(sender);
      void renegotiatePeer(studentId);
    } catch {
      // Ignore remove failures on stale peer state.
    }
  }

  teacherBroadcastSenders.clear();
}

function stopTeacherBroadcast(toMode: WhiteboardMode = "home") {
  if (!teacherBroadcastStream.value && teacherBroadcastSenders.size === 0) {
    if (activeFeature.value === "teacher-broadcast") {
      applyFeatureMode(toMode);
    }
    return;
  }

  detachBroadcastFromAllPeers();
  stopTeacherBroadcastTrack();

  if (activeFeature.value === "teacher-broadcast") {
    applyFeatureMode(toMode);
  }
}

async function startTeacherBroadcast() {
  if (teacherBroadcastStarting.value) {
    return;
  }

  const supportError = teacherBroadcastCaptureSupportError.value;
  if (supportError) {
    showRtcError(`啟動教師廣播失敗: ${supportError}`);
    return;
  }

  teacherBroadcastStarting.value = true;
  try {
    const mediaStream = await navigator.mediaDevices.getDisplayMedia({
      video: toBroadcastCaptureConstraints(),
      audio: false,
    });

    const [videoTrack] = mediaStream.getVideoTracks();
    if (!videoTrack) {
      throw new Error("未取得螢幕視訊軌");
    }

    videoTrack.onended = () => {
      stopTeacherBroadcast("home");
    };

    teacherBroadcastStream.value = mediaStream;
    applyFeatureMode("teacher-broadcast");

    for (const studentId of peers.keys()) {
      attachBroadcastTrackToStudent(studentId);
    }
  } catch (error) {
    showRtcError(`啟動教師廣播失敗: ${String(error)}`);
    stopTeacherBroadcastTrack();
  } finally {
    teacherBroadcastStarting.value = false;
  }
}

async function toggleTeacherBroadcast() {
  if (teacherBroadcastActive.value) {
    stopTeacherBroadcast("home");
    return;
  }

  await startTeacherBroadcast();
}

function showRtcError(message: string) {
  rtcError.value = message;
  rtcErrorVisible.value = true;
}

function clampCountdownInputs() {
  countdownMinutesInput.value = Math.max(
    0,
    Math.min(99, Math.floor(Number(countdownMinutesInput.value) || 0)),
  );
  countdownSecondsInput.value = Math.max(
    0,
    Math.min(59, Math.floor(Number(countdownSecondsInput.value) || 0)),
  );
}

function getConfiguredCountdownSeconds() {
  clampCountdownInputs();
  return countdownMinutesInput.value * 60 + countdownSecondsInput.value;
}

function stopCountdownTimer() {
  if (countdownTimerId !== null) {
    window.clearInterval(countdownTimerId);
    countdownTimerId = null;
  }
  countdownRunning.value = false;
}

async function getCountdownAudioContext() {
  if (!countdownAudioContext) {
    const audioWindow = window as Window & {
      webkitAudioContext?: typeof AudioContext;
    };
    const Ctx = window.AudioContext ?? audioWindow.webkitAudioContext;
    if (!Ctx) {
      return null;
    }
    countdownAudioContext = new Ctx();
  }

  if (countdownAudioContext.state === "suspended") {
    await countdownAudioContext.resume();
  }

  return countdownAudioContext;
}

async function playBeep(frequency = 880, durationMs = 140, volume = 0.06) {
  const ctx = await getCountdownAudioContext();
  if (!ctx) {
    return;
  }

  const oscillator = ctx.createOscillator();
  const gainNode = ctx.createGain();

  oscillator.type = "sine";
  oscillator.frequency.value = frequency;
  gainNode.gain.value = volume;

  oscillator.connect(gainNode);
  gainNode.connect(ctx.destination);

  const now = ctx.currentTime;
  oscillator.start(now);
  oscillator.stop(now + durationMs / 1000);
}

function sleepMs(ms: number) {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

async function playCountdownWarningTone() {
  await playBeep(900, 120, 0.05);
}

async function playCountdownFinishedTone() {
  await playBeep(980, 130, 0.07);
  await sleepMs(120);
  await playBeep(780, 130, 0.07);
  await sleepMs(120);
  await playBeep(1080, 220, 0.08);
}

function syncCountdownRemainingFromInputIfIdle() {
  if (countdownRunning.value) {
    return;
  }
  countdownRemainingSeconds.value = getConfiguredCountdownSeconds();
}

function setCountdownInputsFromRemainingSeconds(totalSeconds: number) {
  const safeTotal = Math.max(0, Math.floor(totalSeconds));
  countdownMinutesInput.value = Math.floor(safeTotal / 60);
  countdownSecondsInput.value = safeTotal % 60;
}

function startCountdownInterval() {
  if (countdownTimerId !== null || countdownRemainingSeconds.value <= 0) {
    return;
  }

  countdownRunning.value = true;
  countdownTimerId = window.setInterval(() => {
    if (countdownRemainingSeconds.value <= 0) {
      return;
    }

    countdownRemainingSeconds.value -= 1;
    setCountdownInputsFromRemainingSeconds(countdownRemainingSeconds.value);

    if (
      countdownRemainingSeconds.value > 0 &&
      countdownRemainingSeconds.value <= 5
    ) {
      void playCountdownWarningTone();
    }

    if (countdownRemainingSeconds.value === 0) {
      stopCountdownTimer();
      countdownDoneSnackbarVisible.value = true;
      void playCountdownFinishedTone();
    }
  }, 1000);
}

function startCountdown() {
  const totalSeconds = getConfiguredCountdownSeconds();
  if (totalSeconds <= 0) {
    showRtcError("請先設定倒數時間（至少 1 秒）");
    return;
  }

  stopCountdownTimer();
  countdownRemainingSeconds.value = totalSeconds;
  setCountdownInputsFromRemainingSeconds(totalSeconds);
  countdownDoneSnackbarVisible.value = false;
  startCountdownInterval();
}

function handlePrimaryCountdownAction() {
  if (countdownRunning.value) {
    stopCountdownTimer();
    return;
  }

  if (countdownRemainingSeconds.value > 0) {
    countdownDoneSnackbarVisible.value = false;
    startCountdownInterval();
    return;
  }

  startCountdown();
}

function endCountdown() {
  stopCountdownTimer();
  countdownDoneSnackbarVisible.value = false;
  countdownRemainingSeconds.value = 0;
  setCountdownInputsFromRemainingSeconds(0);
}

function applyCountdownMinutePreset(minutes: number) {
  if (countdownRunning.value) {
    return;
  }

  countdownMinutesInput.value = Math.max(0, Math.floor(minutes));
  countdownSecondsInput.value = 0;
  syncCountdownRemainingFromInputIfIdle();
}

function toModeMessage(): WhiteboardModeSyncMessage {
  return {
    kind: "mode-sync",
    activeModule: toActiveModule(activeFeature.value),
    mode: activeFeature.value,
    modeVersion: modeVersion.value,
    activeTab: activeWhiteboardTab.value,
    tabVersion: tabVersion.value,
  };
}

function toTeacherSnapshotMessage(
  reason: WhiteboardSnapshotSyncMessage["reason"],
): WhiteboardSnapshotSyncMessage {
  const snapshot = cloneWhiteboardSnapshot(teacherWhiteboardSnapshot.value);
  snapshot.backgroundImage = null;

  return {
    kind: "whiteboard-snapshot",
    modeVersion: modeVersion.value,
    tabVersion: tabVersion.value,
    boardTab: "teacher-board",
    seq: Math.max(0, nextTeacherSequence - 1),
    reason,
    snapshot,
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

function toTeacherBoardBackgroundMessage(
  backgroundImage: string | null,
  backgroundColor: string,
): WhiteboardTeacherBoardControlMessage {
  return {
    kind: "teacher-board-control",
    action: "set-background",
    modeVersion: modeVersion.value,
    tabVersion: tabVersion.value,
    backgroundImage,
    backgroundColor,
  };
}

function toStudentViewControlMessage(): WhiteboardStudentViewControlMessage {
  return {
    kind: "student-view-control",
    forceTeacherBoardView: forceTeacherBoardView.value,
  };
}

function toStudentSwitchBoardMessage(): WhiteboardStudentSwitchBoardMessage {
  return {
    kind: "student-switch-board",
    boardTab: "student-board",
  };
}

function toQuickQaStateMessage(
  reason: QuickQaStateMessage["reason"],
): QuickQaStateMessage {
  return {
    kind: "quick-qa-state",
    reason,
    question: quickQaQuestion.value
      ? {
          ...quickQaQuestion.value,
          options: { ...quickQaQuestion.value.options },
          answersByStudent: { ...quickQaQuestion.value.answersByStudent },
        }
      : null,
  };
}

function getStudentDisplayName(studentId: string): string {
  return (
    students.value.find((student) => student.connection_id === studentId)
      ?.nickname ??
    knownStudentNames.get(studentId) ??
    studentId
  );
}

function publishQuickQaQuestion() {
  if (quickQaEditorLocked.value) {
    return;
  }

  const now = Date.now();
  quickQaQuestion.value = {
    id: `qq-${now}`,
    question: quickQaQuestionInput.value.trim(),
    options: {
      A: quickQaOptionInputs.A.trim(),
      B: quickQaOptionInputs.B.trim(),
      C: quickQaOptionInputs.C.trim(),
      D: quickQaOptionInputs.D.trim(),
    },
    status: "open",
    publishedAt: now,
    closedAt: null,
    correctOption: null,
    answersByStudent: {},
  };

  quickQaResultView.value = "summary";
  applyFeatureMode("quick-qa");
  broadcastToLessonChannels(toQuickQaStateMessage("publish"));
}

function clearQuickQaDraft() {
  if (quickQaEditorLocked.value) {
    return;
  }

  quickQaQuestionInput.value = "";
  for (const option of QUICK_QA_OPTIONS) {
    quickQaOptionInputs[option] = "";
  }
}

function resetQuickQaStageLeaderboard() {
  quickQaStageCorrectCounts.clear();
}

function openCloseQuickQaDialog() {
  if (!quickQaQuestion.value || quickQaQuestion.value.status === "closed") {
    return;
  }

  quickQaCloseDialogVisible.value = true;
}

function closeQuickQaCloseDialog() {
  quickQaCloseDialogVisible.value = false;
}

function closeQuickQaQuestion(correctOptionChoice: QuickQaOption | "none") {
  if (!quickQaQuestion.value || quickQaQuestion.value.status === "closed") {
    return;
  }

  quickQaCloseDialogVisible.value = false;

  const closedAt = Date.now();
  const correctOption =
    correctOptionChoice === "none" ? null : correctOptionChoice;
  const finalAnswers = { ...quickQaQuestion.value.answersByStudent };

  if (correctOption) {
    for (const [studentId, selectedOption] of Object.entries(finalAnswers)) {
      if (selectedOption !== correctOption) {
        continue;
      }
      const currentScore = quickQaStageCorrectCounts.get(studentId) ?? 0;
      quickQaStageCorrectCounts.set(studentId, currentScore + 1);
    }
  }

  quickQaQuestion.value = {
    ...quickQaQuestion.value,
    status: "closed",
    correctOption,
    closedAt,
    answersByStudent: finalAnswers,
  };

  broadcastToLessonChannels(toQuickQaStateMessage("close"));
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

function openStudentCoeditDialog(studentId: string) {
  const student = students.value.find(
    (item) => item.connection_id === studentId,
  );
  if (!student) {
    showRtcError("目標學生已離線，無法開啟共編");
    return;
  }

  coeditStudentId.value = studentId;
  coeditDialogVisible.value = true;
}

function closeStudentCoeditDialog() {
  coeditDialogVisible.value = false;
  coeditStudentId.value = null;
}

function handleGlobalKeydown(event: KeyboardEvent) {
  if (!coeditDialogVisible.value) {
    return;
  }

  if (event.key === "Escape") {
    closeStudentCoeditDialog();
  }
}

function onCoeditDialogVisibilityChanged(value: boolean) {
  if (!value) {
    closeStudentCoeditDialog();
  }
}

function normalizeValidUrl(urlInput: string): string | null {
  const trimmed = urlInput.trim();
  if (!trimmed) {
    return null;
  }

  try {
    const parsed = new URL(trimmed);
    const isHttpProtocol =
      parsed.protocol === "http:" || parsed.protocol === "https:";

    if (!isHttpProtocol) {
      return null;
    }

    return parsed.toString();
  } catch {
    return null;
  }
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
  stroke: WhiteboardSnapshot["strokes"][number],
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
  context.lineCap = "round";
  context.lineJoin = "round";

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

async function renderSnapshotAsPngDataUrl(snapshot: WhiteboardSnapshot) {
  const width = Math.max(
    1,
    Math.floor(snapshot.canvasWidth || WHITEBOARD_CANVAS_WIDTH),
  );
  const height = Math.max(
    1,
    Math.floor(snapshot.canvasHeight || WHITEBOARD_CANVAS_HEIGHT),
  );

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;

  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("無法建立白板匯出畫布");
  }

  context.clearRect(0, 0, width, height);
  context.fillStyle = snapshot.backgroundColor || "#ffffff";
  context.fillRect(0, 0, width, height);

  const backgroundSource = resolveBackgroundImageSource(
    snapshot.backgroundImage,
  );
  if (backgroundSource) {
    try {
      const image = await loadImage(backgroundSource);
      context.drawImage(image, 0, 0, width, height);
    } catch (error) {
      console.warn(String(error));
    }
  }

  for (const stroke of snapshot.strokes) {
    drawStroke(context, stroke);
  }

  return {
    dataUrl: canvas.toDataURL("image/png"),
    width,
    height,
  };
}

function toFileTimestamp() {
  const now = new Date();
  const yyyy = String(now.getFullYear());
  const mm = String(now.getMonth() + 1).padStart(2, "0");
  const dd = String(now.getDate()).padStart(2, "0");
  const hh = String(now.getHours()).padStart(2, "0");
  const min = String(now.getMinutes()).padStart(2, "0");
  const sec = String(now.getSeconds()).padStart(2, "0");
  return `${yyyy}${mm}${dd}-${hh}${min}${sec}`;
}

async function downloadStudentBoardsPdf() {
  if (downloadingStudentBoardsPdf.value) {
    return;
  }

  if (studentBoardTiles.value.length === 0) {
    showRtcError("目前沒有可下載的學生白板");
    return;
  }

  downloadingStudentBoardsPdf.value = true;

  try {
    const { jsPDF } = await import("jspdf");
    const pdf = new jsPDF({
      orientation: "portrait",
      unit: "mm",
      format: "a4",
    });

    const pageWidth = pdf.internal.pageSize.getWidth();
    const pageHeight = pdf.internal.pageSize.getHeight();

    const margin = 10;
    const slotGap = 8;
    const slotWidth = pageWidth - margin * 2;
    const slotHeight = (pageHeight - margin * 2 - slotGap) / 2;
    const cardPadding = 2;

    for (let index = 0; index < studentBoardTiles.value.length; index += 1) {
      if (index > 0 && index % 2 === 0) {
        pdf.addPage("a4", "portrait");
      }

      const tile = studentBoardTiles.value[index];
      const slotIndex = index % 2;
      const slotX = margin;
      const slotY = margin + slotIndex * (slotHeight + slotGap);

      pdf.setDrawColor(160, 160, 160);
      pdf.setLineWidth(0.3);
      pdf.rect(slotX, slotY, slotWidth, slotHeight);

      pdf.setFontSize(12);
      pdf.text(
        `${index + 1}. ${tile.nickname}`,
        slotX + cardPadding,
        slotY + 5,
      );

      const rendered = await renderSnapshotAsPngDataUrl(tile.snapshot);
      const imageAspectRatio = rendered.width / rendered.height;
      const contentX = slotX + cardPadding;
      const contentY = slotY + 8;
      const contentWidth = slotWidth - cardPadding * 2;
      const contentHeight = slotHeight - 10;

      let imageWidth = contentWidth;
      let imageHeight = imageWidth / imageAspectRatio;

      if (imageHeight > contentHeight) {
        imageHeight = contentHeight;
        imageWidth = imageHeight * imageAspectRatio;
      }

      const imageX = contentX + (contentWidth - imageWidth) / 2;
      const imageY = contentY + (contentHeight - imageHeight) / 2;

      pdf.addImage(
        rendered.dataUrl,
        "PNG",
        imageX,
        imageY,
        imageWidth,
        imageHeight,
        undefined,
        "FAST",
      );
    }

    pdf.save(`student-whiteboards-${toFileTimestamp()}.pdf`);
  } catch (error) {
    showRtcError(`下載學生白板失敗: ${String(error)}`);
  } finally {
    downloadingStudentBoardsPdf.value = false;
  }
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
  sendToStudentChannel(
    studentId,
    toTeacherBoardBackgroundMessage(
      teacherWhiteboardSnapshot.value.backgroundImage ?? null,
      teacherWhiteboardSnapshot.value.backgroundColor,
    ),
  );
  sendToStudentChannel(studentId, toStudentViewControlMessage());
  sendToStudentChannel(studentId, toQuickQaStateMessage("join"));
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

function flushQueuedTeacherToStudentEvents(studentId: string) {
  const queue = queuedTeacherToStudentEvents.get(studentId);
  if (!queue || queue.length === 0) {
    return;
  }

  const timer = teacherToStudentBatchFlushTimers.get(studentId);
  if (typeof timer === "number") {
    window.clearTimeout(timer);
    teacherToStudentBatchFlushTimers.delete(studentId);
  }

  const events = queue.splice(0, queue.length);
  const message: WhiteboardTeacherStudentEventBatchMessage = {
    kind: "teacher-student-events-batch",
    boardTab: "student-board",
    startSeq: events[0].seq,
    endSeq: events[events.length - 1].seq,
    events,
  };

  teacherToStudentLastSequence.set(studentId, message.endSeq);
  sendToStudentChannel(studentId, message);
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

function scheduleTeacherToStudentBatchFlush(studentId: string) {
  if (teacherToStudentBatchFlushTimers.has(studentId)) {
    return;
  }

  const timer = window.setTimeout(() => {
    teacherToStudentBatchFlushTimers.delete(studentId);
    flushQueuedTeacherToStudentEvents(studentId);
  }, BATCH_INTERVAL_MS);

  teacherToStudentBatchFlushTimers.set(studentId, timer);
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

function applyTeacherEventToStudentSnapshot(
  studentId: string,
  payload: WhiteboardIncrementalEventPayload,
) {
  const baseSnapshot =
    studentBoardSnapshots.get(studentId) ?? createEmptyWhiteboardSnapshot();
  const nextSnapshot = cloneWhiteboardSnapshot(baseSnapshot);

  applyIncrementalEvent(nextSnapshot, {
    ...payload,
    seq: -1,
    timestamp: Date.now(),
  });

  setStudentSnapshot(studentId, nextSnapshot);
}

function enqueueTeacherToStudentIncrementalEvent(
  studentId: string,
  payload: WhiteboardIncrementalEventPayload,
) {
  const nextSeq = teacherToStudentNextSequence.get(studentId) ?? 1;
  const queue = queuedTeacherToStudentEvents.get(studentId) ?? [];

  queue.push({
    ...payload,
    seq: nextSeq,
    timestamp: Date.now(),
  });

  queuedTeacherToStudentEvents.set(studentId, queue);
  teacherToStudentNextSequence.set(studentId, nextSeq + 1);

  if (queue.length >= BATCH_MAX_EVENTS) {
    flushQueuedTeacherToStudentEvents(studentId);
    return;
  }

  scheduleTeacherToStudentBatchFlush(studentId);
}

function handleCoeditStudentSnapshot(snapshot: WhiteboardSnapshot) {
  const studentId = coeditStudentId.value;
  if (!studentId) {
    return;
  }

  setStudentSnapshot(studentId, snapshot);
}

function handleCoeditStudentSyncEvent(
  payload: WhiteboardIncrementalEventPayload,
) {
  const studentId = coeditStudentId.value;
  if (!studentId) {
    return;
  }

  try {
    applyTeacherEventToStudentSnapshot(studentId, payload);
  } catch (error) {
    showRtcError(`教師共編事件套用失敗: ${String(error)}`);
    return;
  }

  enqueueTeacherToStudentIncrementalEvent(studentId, payload);
}

function applyFeatureMode(mode: WhiteboardMode) {
  if (mode !== "teacher-broadcast" && teacherBroadcastActive.value) {
    stopTeacherBroadcastTrack();
    detachBroadcastFromAllPeers();
  }

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
  if (forceTeacherBoardView.value) {
    forceTeacherBoardView.value = false;
    broadcastToLessonChannels(toStudentViewControlMessage());
  }
  applyFeatureMode("home");
}

function activateWhiteboard() {
  applyFeatureMode("whiteboard");
}

function activateQuickQa() {
  if (activeFeature.value !== "quick-qa") {
    resetQuickQaStageLeaderboard();
  }
  applyFeatureMode("quick-qa");
}

function activateContactBook() {
  applyFeatureMode("contact-book");
}

function activateTeacherBoardTab() {
  applyWhiteboardTab("teacher-board");
}

function activateStudentBoardTab() {
  applyWhiteboardTab("student-board");
}

function onForceTeacherBoardViewChanged(value: boolean | null) {
  forceTeacherBoardView.value = value === true;

  if (forceTeacherBoardView.value) {
    applyFeatureMode("whiteboard");
    applyWhiteboardTab("teacher-board");
  }

  broadcastToLessonChannels(toStudentViewControlMessage());
}

function switchStudentsToOwnBoard() {
  broadcastToLessonChannels(toStudentSwitchBoardMessage());
}

function forceLogoutAllStudents() {
  if (students.value.length === 0) {
    return;
  }

  if (!ws || ws.readyState !== WebSocket.OPEN) {
    showRtcError("尚未連線到訊號服務，無法退出所有學生");
    return;
  }

  sendSignal({
    event: "force-logout-all-students",
  });
}

function openStudentUrlDialog() {
  studentOpenUrlInput.value = "";
  studentOpenUrlError.value = "";
  openUrlDialogVisible.value = true;
}

function closeStudentUrlDialog() {
  openUrlDialogVisible.value = false;
  studentOpenUrlError.value = "";
}

function submitOpenStudentUrlCommand() {
  const validUrl = normalizeValidUrl(studentOpenUrlInput.value);
  if (!validUrl) {
    studentOpenUrlError.value = "請輸入有效網址（需為 http 或 https）";
    return;
  }

  broadcastToLessonChannels({
    kind: "student-open-url",
    url: validUrl,
  });

  closeStudentUrlDialog();
}

function handleTeacherWhiteboardSnapshot(snapshot: WhiteboardSnapshot) {
  teacherWhiteboardSnapshot.value = cloneWhiteboardSnapshot(snapshot);
}

function handleTeacherWhiteboardSyncEvent(
  payload: WhiteboardIncrementalEventPayload,
) {
  if (payload.type === "background-change") {
    broadcastToLessonChannels(
      toTeacherBoardBackgroundMessage(
        payload.backgroundImage ?? null,
        payload.backgroundColor,
      ),
    );
    return;
  }

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
        // 在不保證事件完整順序的模式下，缺少對應筆劃時直接略過該點。
        break;
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
  teacherToStudentLastSequence.set(studentId, 0);
  teacherToStudentNextSequence.set(studentId, 1);
  queuedTeacherToStudentEvents.set(studentId, []);
}

function processStudentBatch(
  studentId: string,
  message: WhiteboardStudentEventBatchMessage,
) {
  const baseSnapshot =
    studentBoardSnapshots.get(studentId) ?? createEmptyWhiteboardSnapshot();
  const nextSnapshot = cloneWhiteboardSnapshot(baseSnapshot);

  for (const event of message.events) {
    applyIncrementalEvent(nextSnapshot, event);
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
  const flattenedDrawingLayer =
    teacherWhiteboardCanvasRef.value?.getFlattenedDrawingLayer() ?? null;
  if (!flattenedDrawingLayer) {
    showRtcError("教師白板壓平資料取得失敗");
  }

  const teacherSnapshot = cloneWhiteboardSnapshot(
    teacherWhiteboardSnapshot.value,
  );

  for (const student of students.value) {
    const currentSnapshot =
      studentBoardSnapshots.get(student.connection_id) ??
      createEmptyWhiteboardSnapshot();

    setStudentSnapshot(student.connection_id, {
      ...cloneWhiteboardSnapshot(currentSnapshot),
      strokes: teacherSnapshot.strokes.map((stroke) =>
        cloneWhiteboardStroke(stroke),
      ),
    });

    studentBoardLastSequence.set(student.connection_id, 0);
    teacherToStudentLastSequence.set(student.connection_id, 0);
    teacherToStudentNextSequence.set(student.connection_id, 1);
    queuedTeacherToStudentEvents.set(student.connection_id, []);
    sendToStudentChannel(student.connection_id, {
      kind: "student-board-control",
      action: "replace-strokes",
      tabVersion: tabVersion.value,
      strokes: teacherSnapshot.strokes.map((stroke) =>
        cloneWhiteboardStroke(stroke),
      ),
    });
  }
}

function handleChannelMessage(studentId: string, raw: string) {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isWhiteboardSyncMessage(parsed)) {
      return;
    }

    if (parsed.kind === "student-focus-status") {
      const message = parsed as StudentFocusStatusMessage;
      const status = normalizeStudentFocusStatus(message.status);
      const updatedAt = normalizeStudentFocusUpdatedAt(
        message.updatedAt,
        Date.now(),
      );
      applyStudentFocusStatus(studentId, status, updatedAt);
      return;
    }

    if (parsed.kind === "quick-qa-answer-submit") {
      if (!quickQaQuestion.value || quickQaQuestion.value.status !== "open") {
        return;
      }

      quickQaQuestion.value = {
        ...quickQaQuestion.value,
        answersByStudent: {
          ...quickQaQuestion.value.answersByStudent,
          [studentId]: parsed.option,
        },
      };

      broadcastToLessonChannels(toQuickQaStateMessage("answer-update"));
      return;
    }

    if (parsed.kind === "snapshot-request") {
      const request = parsed as WhiteboardSnapshotRequestMessage;
      if (request.boardTab === "teacher-board") {
        const reason = request.reason === "join-init" ? "join" : "resync";
        sendToStudentChannel(studentId, toTeacherSnapshotMessage(reason));
        sendToStudentChannel(
          studentId,
          toTeacherBoardBackgroundMessage(
            teacherWhiteboardSnapshot.value.backgroundImage ?? null,
            teacherWhiteboardSnapshot.value.backgroundColor,
          ),
        );
        return;
      }

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
    if (teacherBroadcastActive.value) {
      attachBroadcastTrackToStudent(studentId);
    }
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
  const teacherToStudentTimer = teacherToStudentBatchFlushTimers.get(studentId);
  if (typeof teacherToStudentTimer === "number") {
    window.clearTimeout(teacherToStudentTimer);
    teacherToStudentBatchFlushTimers.delete(studentId);
  }

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
  teacherBroadcastSenders.delete(studentId);
  studentFocusStatusById.delete(studentId);
  studentFocusUpdatedAtById.delete(studentId);
  studentBoardSnapshots.delete(studentId);
  studentBoardLastSequence.delete(studentId);
  teacherToStudentLastSequence.delete(studentId);
  teacherToStudentNextSequence.delete(studentId);
  queuedTeacherToStudentEvents.delete(studentId);

  if (coeditStudentId.value === studentId) {
    closeStudentCoeditDialog();
    showRtcError(`學生 ${studentId} 已離線，共編已關閉`);
  }
}

function reconcileStudentConnections(nextStudents: StudentSession[]) {
  for (const student of nextStudents) {
    knownStudentNames.set(student.connection_id, student.nickname);
  }

  const activeIds = new Set(
    nextStudents.map((student) => student.connection_id),
  );

  for (const studentId of studentFocusStatusById.keys()) {
    if (!activeIds.has(studentId)) {
      studentFocusStatusById.delete(studentId);
      studentFocusUpdatedAtById.delete(studentId);
    }
  }

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
  if (message.event === "classroom-state") {
    const payload = message.payload as { state?: ClassroomStatePayload };
    if (payload?.state) {
      currentClassroomName.value = payload.state.current_classroom.name;
      currentClassroomId.value = payload.state.current_classroom.id;
    }
    return;
  }

  if (message.event === "students" || message.event === "teacher-ready") {
    const payload = message.payload as
      | { students?: StudentSession[] }
      | undefined;
    const nextStudents = payload?.students ?? [];
    const mergedStudents = mergeStudentsWithFocus(nextStudents);
    students.value = mergedStudents;
    reconcileStudentConnections(mergedStudents);
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
      if (teacherBroadcastActive.value) {
        attachBroadcastTrackToStudent(message.source);
      }
    } catch (error) {
      showRtcError(`教師端 WebRTC 錯誤: ${String(error)}`);
    }
    return;
  }

  if (message.event === "answer" && message.source && message.payload) {
    try {
      const peer = ensurePeer(message.source);
      await peer.setRemoteDescription(
        message.payload as RTCSessionDescriptionInit,
      );
      await flushQueuedCandidates(message.source);
    } catch (error) {
      showRtcError(`教師端重協商失敗: ${String(error)}`);
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

watch([countdownMinutesInput, countdownSecondsInput], () => {
  syncCountdownRemainingFromInputIfIdle();
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
  window.addEventListener("keydown", handleGlobalKeydown);

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
  stopTeacherBroadcastTrack();

  stopCountdownTimer();

  if (countdownAudioContext) {
    void countdownAudioContext.close();
    countdownAudioContext = null;
  }

  if (teacherBatchFlushTimer !== null) {
    window.clearTimeout(teacherBatchFlushTimer);
    teacherBatchFlushTimer = null;
  }

  for (const timer of teacherToStudentBatchFlushTimers.values()) {
    window.clearTimeout(timer);
  }
  teacherToStudentBatchFlushTimers.clear();

  window.removeEventListener("keydown", handleGlobalKeydown);

  if (ws) {
    ws.close();
  }

  for (const studentId of peers.keys()) {
    disposeStudentConnection(studentId);
  }

  peers.clear();
  lessonChannels.clear();
  teacherBroadcastSenders.clear();
  pendingCandidates.clear();
  studentBoardSnapshots.clear();
  studentBoardLastSequence.clear();
  teacherToStudentLastSequence.clear();
  teacherToStudentNextSequence.clear();
  queuedTeacherToStudentEvents.clear();

  if (studentGalleryResizeObserver) {
    studentGalleryResizeObserver.disconnect();
    studentGalleryResizeObserver = null;
  }
});
</script>

<template>
  <v-app>
    <v-app-bar title="song-class(教師端)">
      <template #prepend>
        <div class="pl-3 text-caption text-medium-emphasis">
          目前班級: {{ currentClassroomName }}
        </div>
      </template>
      <template #append>
        <div class="student-join-qr-wrap">
          <span class="student-join-qr-text">學生端連結</span>
          <v-btn
            icon
            variant="text"
            size="small"
            class="student-join-qr-btn"
            aria-label="顯示學生端連結 QR Code"
            @click="openStudentQrDialogAndCopyUrl"
          >
            <QrcodeVue
              :value="studentJoinUrl"
              :size="34"
              level="M"
              render-as="svg"
              class="student-join-qr-small"
            />
          </v-btn>
        </div>
        <span class="app-version-text">{{ appVersionLabel }}</span>
      </template>
    </v-app-bar>
    <v-navigation-drawer :width="240" permanent>
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
          color="brown"
          :variant="activeFeature === 'contact-book' ? 'flat' : 'outlined'"
          @click="activateContactBook"
        >
          聯絡簿管理
        </v-btn>
        <v-btn
          color="primary"
          :variant="activeFeature === 'whiteboard' ? 'flat' : 'outlined'"
          @click="activateWhiteboard"
        >
          小白版
        </v-btn>
        <v-btn
          color="indigo"
          :variant="activeFeature === 'quick-qa' ? 'flat' : 'outlined'"
          @click="activateQuickQa"
        >
          快問快答
        </v-btn>
        <v-btn
          color="deep-orange"
          :loading="teacherBroadcastStarting"
          :disabled="
            !!teacherBroadcastCaptureSupportError && !teacherBroadcastActive
          "
          :variant="teacherBroadcastActive ? 'flat' : 'outlined'"
          @click="toggleTeacherBroadcast"
        >
          {{ teacherBroadcastActive ? "停止廣播" : "廣播教師畫面" }}
        </v-btn>
        <v-btn color="info" variant="tonal" @click="openStudentUrlDialog">
          學生開啟網頁
        </v-btn>
      </div>
      <v-alert
        v-if="teacherBroadcastCaptureSupportError"
        type="warning"
        variant="tonal"
        class="mx-2 mt-2"
      >
        {{ teacherBroadcastCaptureSupportError }}
      </v-alert>
      <v-card rounded="lg" variant="outlined" class="countdown-mini-card ma-2">
        <v-card-text class="pa-2 d-flex flex-column ga-2">
          <div class="d-flex ga-2 align-start">
            <div class="countdown-left d-flex flex-column ga-1">
              <div class="countdown-title text-overline">倒數計時器</div>
              <div class="d-flex ga-2">
                <v-text-field
                  v-model.number="countdownMinutesInput"
                  class="countdown-input"
                  label="分"
                  type="number"
                  min="0"
                  max="99"
                  density="compact"
                  variant="outlined"
                  hide-details
                  :readonly="countdownRunning"
                />
                <v-text-field
                  v-model.number="countdownSecondsInput"
                  class="countdown-input"
                  label="秒"
                  type="number"
                  min="0"
                  max="59"
                  density="compact"
                  variant="outlined"
                  hide-details
                  :readonly="countdownRunning"
                />
              </div>

              <div class="countdown-presets-row d-flex ga-2">
                <v-btn
                  v-for="preset in countdownMinutePresets"
                  :key="`countdown-preset-${preset}`"
                  class="countdown-preset-btn"
                  size="x-small"
                  variant="outlined"
                  color="primary"
                  :disabled="countdownRunning"
                  @click="applyCountdownMinutePreset(preset)"
                >
                  {{ preset }}
                </v-btn>
              </div>
            </div>

            <div class="countdown-controls d-flex flex-column ga-2">
              <v-tooltip :text="countdownPauseResumeTooltip" location="top">
                <template #activator="{ props: tooltipProps }">
                  <v-btn
                    v-bind="tooltipProps"
                    class="countdown-control-btn"
                    size="x-small"
                    :icon="countdownPauseResumeIcon"
                    :color="countdownRunning ? 'warning' : 'primary'"
                    :variant="countdownRunning ? 'tonal' : 'flat'"
                    @click="handlePrimaryCountdownAction"
                  />
                </template>
              </v-tooltip>

              <v-tooltip text="停止" location="top">
                <template #activator="{ props: tooltipProps }">
                  <v-btn
                    v-bind="tooltipProps"
                    class="countdown-control-btn"
                    size="x-small"
                    icon="mdi-stop"
                    color="secondary"
                    variant="outlined"
                    @click="endCountdown"
                  />
                </template>
              </v-tooltip>
            </div>
          </div>
        </v-card-text>
      </v-card>
      <div class="ma-2">
        <StudentListCard
          class="teacher-student-list-card"
          title="已連入學生"
          :students="sortedStudentsForChipList"
        />
        <v-btn
          class="mt-2"
          color="error"
          variant="tonal"
          block
          :disabled="students.length === 0"
          @click="forceLogoutAllStudents"
        >
          退出所有學生
        </v-btn>
      </div>
    </v-navigation-drawer>
    <v-main class="teacher-main">
      <div class="feature-main pa-3">
        <div
          v-if="activeFeature === 'contact-book'"
          class="contact-book-layout"
        >
          <ContactBookManager
            :base-url="props.baseUrl"
            :classroom-id="currentClassroomId"
            @error="showRtcError"
          />
        </div>

        <div
          v-else-if="activeFeature === 'whiteboard'"
          class="whiteboard-layout"
        >
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
                  ref="teacherWhiteboardCanvasRef"
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
                    role="button"
                    tabindex="0"
                    @click="openStudentCoeditDialog(tile.id)"
                    @keydown.enter.prevent="openStudentCoeditDialog(tile.id)"
                    @keydown.space.prevent="openStudentCoeditDialog(tile.id)"
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
                v-model="forceTeacherBoardView"
                color="success"
                density="compact"
                hide-details
                label="強制觀看教師白板"
                @update:model-value="onForceTeacherBoardViewChanged"
              />

              <v-btn
                color="primary"
                variant="tonal"
                block
                :disabled="forceTeacherBoardView"
                @click="switchStudentsToOwnBoard"
              >
                學生切換為自己白板
              </v-btn>

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

              <v-btn
                color="success"
                variant="tonal"
                block
                :loading="downloadingStudentBoardsPdf"
                :disabled="students.length === 0"
                @click="downloadStudentBoardsPdf"
              >
                下載學生白板
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

        <div v-else-if="activeFeature === 'quick-qa'" class="quick-qa-layout">
          <v-card rounded="lg" variant="outlined" class="quick-qa-editor-card">
            <v-card-title class="d-flex align-center justify-space-between">
              <span class="text-h6">發布新問題</span>
              <v-chip color="warning" variant="flat" size="small"
                >Step 1 of 2</v-chip
              >
            </v-card-title>
            <v-card-text class="d-flex flex-column ga-4">
              <v-textarea
                v-model="quickQaQuestionInput"
                label="題目內容"
                variant="outlined"
                auto-grow
                rows="3"
                hint="可留白，改用口頭敘述"
                persistent-hint
              />

              <div
                v-for="option in QUICK_QA_OPTIONS"
                :key="option"
                class="d-flex align-center ga-3"
              >
                <v-avatar :color="optionBadgeColor(option)" size="34">
                  <span class="text-white font-weight-bold">{{ option }}</span>
                </v-avatar>
                <v-text-field
                  v-model="quickQaOptionInputs[option]"
                  :label="`${option} 選項內容`"
                  variant="outlined"
                  density="comfortable"
                  hide-details
                  placeholder="可留白，改用口頭敘述"
                />
              </div>

              <div class="d-flex align-center ga-3 mt-2">
                <v-btn
                  color="primary"
                  size="large"
                  :disabled="quickQaEditorLocked"
                  prepend-icon="mdi-send"
                  @click="publishQuickQaQuestion"
                >
                  發布題目
                </v-btn>

                <v-btn
                  color="secondary"
                  size="large"
                  variant="outlined"
                  :disabled="quickQaEditorLocked"
                  prepend-icon="mdi-eraser"
                  @click="clearQuickQaDraft"
                >
                  清除題目與選項
                </v-btn>

                <v-btn
                  color="error"
                  size="large"
                  variant="outlined"
                  :disabled="!quickQaCanClose"
                  prepend-icon="mdi-stop-circle-outline"
                  @click="openCloseQuickQaDialog"
                >
                  結束作答
                </v-btn>
              </div>
            </v-card-text>
          </v-card>

          <div class="quick-qa-right-panel d-flex flex-column ga-3">
            <v-card
              rounded="lg"
              variant="outlined"
              class="quick-qa-result-card"
            >
              <v-card-title class="d-flex align-center justify-space-between">
                <span class="text-h6">
                  {{ quickQaIsOpen ? "作答狀態" : "作答詳情" }}
                </span>
                <v-chip
                  :color="quickQaIsOpen ? 'success' : 'primary'"
                  variant="tonal"
                  size="small"
                >
                  {{ quickQaIsOpen ? "Live" : "Closed" }}
                </v-chip>
              </v-card-title>
              <v-card-text
                v-if="quickQaHasQuestion"
                class="d-flex flex-column ga-3"
              >
                <template v-if="quickQaIsOpen">
                  <v-card variant="tonal" color="primary" rounded="lg">
                    <v-card-text class="py-3">
                      <div class="text-caption text-medium-emphasis">
                        已作答 / 已連線
                      </div>
                      <div class="text-h4 font-weight-black">
                        {{ quickQaTotalAnswers }}/{{ students.length }}
                      </div>
                    </v-card-text>
                  </v-card>

                  <div class="d-flex flex-wrap ga-2">
                    <v-chip
                      v-for="student in quickQaStudentStatuses"
                      :key="`quick-qa-student-${student.id}`"
                      :color="student.answered ? 'success' : undefined"
                      :prepend-icon="student.answered ? 'mdi-check' : undefined"
                      :variant="student.answered ? 'flat' : 'outlined'"
                      size="small"
                    >
                      {{ student.nickname }}
                    </v-chip>
                  </div>

                  <div
                    v-if="quickQaStudentStatuses.length === 0"
                    class="text-body-2 text-medium-emphasis"
                  >
                    目前沒有連線中的學生
                  </div>
                </template>

                <template v-else>
                  <v-card variant="tonal" color="primary" rounded="lg">
                    <v-card-text class="py-3">
                      <div class="text-caption text-medium-emphasis">
                        參與人數
                      </div>
                      <div class="text-h4 font-weight-black">
                        {{ quickQaTotalAnswers }}/{{ students.length }}
                      </div>
                    </v-card-text>
                  </v-card>

                  <v-tabs
                    density="compact"
                    color="primary"
                    :model-value="quickQaResultView"
                    @update:model-value="
                      quickQaResultView = $event as 'summary' | 'details'
                    "
                  >
                    <v-tab value="summary">統計</v-tab>
                    <v-tab value="details">明細</v-tab>
                  </v-tabs>

                  <div
                    v-if="quickQaResultView === 'summary'"
                    class="d-flex flex-column ga-2"
                  >
                    <v-card
                      v-for="stat in quickQaStats"
                      :key="`summary-${stat.option}`"
                      rounded="lg"
                      variant="tonal"
                    >
                      <v-card-text class="py-2">
                        <div class="d-flex align-center justify-space-between">
                          <div class="d-flex align-center ga-2">
                            <v-chip
                              :color="optionBadgeColor(stat.option)"
                              size="small"
                              variant="flat"
                            >
                              {{ stat.option }}
                            </v-chip>
                            <span class="font-weight-bold"
                              >{{ stat.count }} 票</span
                            >
                          </div>
                          <span class="text-medium-emphasis"
                            >{{ stat.percentage }}%</span
                          >
                        </div>
                        <v-progress-linear
                          class="mt-2"
                          rounded
                          :model-value="stat.percentage"
                          :color="optionBadgeColor(stat.option)"
                          height="8"
                        />
                      </v-card-text>
                    </v-card>
                  </div>

                  <div v-else class="d-flex flex-column ga-2">
                    <v-card
                      v-for="optionGroup in quickQaDetailsByOption"
                      :key="`detail-${optionGroup.option}`"
                      rounded="lg"
                      variant="outlined"
                    >
                      <v-card-title
                        class="py-2 text-subtitle-1 d-flex align-center ga-2"
                      >
                        <v-chip
                          :color="optionBadgeColor(optionGroup.option)"
                          size="small"
                          variant="flat"
                        >
                          {{ optionGroup.option }}
                        </v-chip>
                        <span>{{ optionGroup.count }} 人</span>
                      </v-card-title>
                      <v-card-text class="pt-0">
                        <v-list
                          v-if="optionGroup.students.length > 0"
                          density="compact"
                          class="pa-0"
                        >
                          <v-list-item
                            v-for="student in optionGroup.students"
                            :key="`detail-${optionGroup.option}-${student.id}`"
                            :title="student.nickname"
                          />
                        </v-list>
                        <div v-else class="text-body-2 text-medium-emphasis">
                          尚無作答
                        </div>
                      </v-card-text>
                    </v-card>
                  </div>
                </template>
              </v-card-text>

              <v-card-text v-else class="text-medium-emphasis">
                尚未發布題目
              </v-card-text>
            </v-card>

            <v-card
              rounded="lg"
              variant="outlined"
              class="quick-qa-leaderboard-card"
            >
              <v-card-title class="d-flex align-center justify-space-between">
                <span class="text-h6">階段排行榜</span>
                <v-chip size="small" variant="tonal" color="primary"
                  >Top 10</v-chip
                >
              </v-card-title>
              <v-card-text>
                <v-list
                  v-if="quickQaLeaderboardTop10.length > 0"
                  density="compact"
                  class="pa-0"
                >
                  <v-list-item
                    v-for="(entry, index) in quickQaLeaderboardTop10"
                    :key="entry.studentId"
                  >
                    <template #prepend>
                      <v-avatar size="28" color="primary" variant="tonal">
                        {{ index + 1 }}
                      </v-avatar>
                    </template>
                    <v-list-item-title>{{ entry.nickname }}</v-list-item-title>
                    <template #append>
                      <v-chip color="success" size="small" variant="flat"
                        >{{ entry.score }} 題</v-chip
                      >
                    </template>
                  </v-list-item>
                </v-list>
                <div v-else class="text-body-2 text-medium-emphasis">
                  尚無排行榜資料
                </div>
              </v-card-text>
            </v-card>
          </div>
        </div>

        <v-card
          v-else-if="activeFeature === 'teacher-broadcast'"
          rounded="xl"
          elevation="8"
          class="teacher-broadcast-card d-flex align-center justify-center"
        >
          <v-card-text class="text-center py-16">
            <div class="text-display-large font-weight-black mb-3">
              教室畫面廣播中
            </div>
            <div class="text-medium-emphasis">
              教師螢幕已透過 WebRTC 廣播到所有學生端
            </div>
          </v-card-text>
        </v-card>

        <v-card
          v-else
          rounded="xl"
          elevation="6"
          class="h-100 d-flex align-center justify-center"
        >
          <v-card-text class="text-center py-16">
            <div class="text-display-large font-weight-black mb-2">
              請專心學習
            </div>
            <div class="text-medium-emphasis">
              教師目前在首頁模式，可隨時切換到其他模組。
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

    <v-snackbar
      v-model="countdownDoneSnackbarVisible"
      color="warning"
      :timeout="4500"
      location="bottom right"
    >
      倒數結束
      <template #actions>
        <v-btn variant="text" @click="countdownDoneSnackbarVisible = false"
          >關閉</v-btn
        >
      </template>
    </v-snackbar>

    <v-dialog v-model="openUrlDialogVisible" max-width="520">
      <v-card rounded="lg">
        <v-card-title class="text-h6">通知學生開啟網頁</v-card-title>
        <v-card-text>
          <v-text-field
            v-model="studentOpenUrlInput"
            label="網址"
            placeholder="https://example.com"
            variant="outlined"
            density="comfortable"
            :error-messages="studentOpenUrlError"
            @update:model-value="studentOpenUrlError = ''"
            @keydown.enter="submitOpenStudentUrlCommand"
          />
        </v-card-text>
        <v-card-actions class="justify-end">
          <v-btn variant="text" @click="closeStudentUrlDialog">取消</v-btn>
          <v-btn color="primary" @click="submitOpenStudentUrlCommand"
            >確定</v-btn
          >
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="quickQaCloseDialogVisible" max-width="520">
      <v-card rounded="lg">
        <v-card-title class="text-h6">選擇正確答案並結束作答</v-card-title>
        <v-card-text>
          <div class="text-body-2 mb-3">
            請選擇正確答案。按下後將立即結束本題作答。
          </div>
          <div class="d-flex flex-wrap ga-2">
            <v-btn
              v-for="option in QUICK_QA_OPTIONS"
              :key="`close-quick-qa-${option}`"
              :color="optionBadgeColor(option)"
              variant="tonal"
              @click="closeQuickQaQuestion(option)"
            >
              {{ option }}
            </v-btn>
            <v-btn variant="outlined" @click="closeQuickQaQuestion('none')">
              不設定答案
            </v-btn>
          </div>
        </v-card-text>
        <v-card-actions class="justify-end">
          <v-btn variant="text" @click="closeQuickQaCloseDialog">取消</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="studentQrDialogVisible" max-width="92vw" width="92vw">
      <v-card rounded="lg" class="student-join-qr-dialog-card">
        <v-card-title class="d-flex align-center justify-space-between">
          <span class="text-h6">學生端連結 QR Code</span>
          <v-btn
            icon="mdi-close"
            variant="text"
            aria-label="關閉學生端連結 QR Code 對話框"
            @click="studentQrDialogVisible = false"
          />
        </v-card-title>
        <v-card-text class="student-join-qr-dialog-content">
          <QrcodeVue
            :value="studentJoinUrl"
            :size="560"
            level="H"
            render-as="svg"
            class="student-join-qr-large"
          />
          <div class="student-join-qr-url text-medium-emphasis">
            {{ studentJoinUrl }}
          </div>
          <v-btn
            class="student-join-qr-copy-btn"
            color="primary"
            variant="tonal"
            prepend-icon="mdi-content-copy"
            @click="copyStudentJoinUrl"
          >
            複製網址
          </v-btn>
        </v-card-text>
      </v-card>
    </v-dialog>

    <v-dialog
      v-model="coeditDialogVisible"
      max-width="96vw"
      width="96vw"
      height="96vh"
      max-height="96vh"
      @update:model-value="onCoeditDialogVisibilityChanged"
    >
      <v-card rounded="lg" class="coedit-dialog-card">
        <v-card-title
          class="coedit-dialog-title d-flex align-center justify-space-between"
        >
          <span class="text-h6">學生白板共同編輯</span>
          <v-btn
            icon="mdi-close"
            variant="text"
            aria-label="關閉共編對話框"
            @click="closeStudentCoeditDialog"
          />
        </v-card-title>
        <v-card-text class="coedit-dialog-content">
          <WhiteboardCanvas
            :title="coeditStudent?.nickname ?? '學生白板'"
            :snapshot="coeditStudentSnapshot"
            @update:snapshot="handleCoeditStudentSnapshot"
            @sync-event="handleCoeditStudentSyncEvent"
          />
        </v-card-text>
      </v-card>
    </v-dialog>
  </v-app>
</template>

<style scoped>
.student-join-qr-wrap {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-right: 12px;
}

.student-join-qr-text {
  font-size: 0.8rem;
  color: rgba(var(--v-theme-on-surface), 0.8);
  white-space: nowrap;
}

.student-join-qr-btn {
  min-width: 38px;
  width: 38px;
  height: 38px;
  padding: 0;
}

.student-join-qr-small {
  border-radius: 4px;
  background: #fff;
}

.student-join-qr-dialog-card {
  width: 100%;
  min-height: min(90vh, 820px);
}

.student-join-qr-dialog-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  min-height: min(84vh, 760px);
}

.student-join-qr-large {
  width: min(78vw, 78vh);
  max-width: 100%;
  height: auto;
  background: #fff;
  padding: 10px;
  border-radius: 10px;
}

.student-join-qr-url {
  max-width: 100%;
  text-align: center;
  font-size: 0.8rem;
  word-break: break-all;
}

.student-join-qr-copy-btn {
  width: 100%;
  max-width: 320px;
}

.app-version-text {
  font-size: 0.75rem;
  color: rgba(var(--v-theme-on-surface), 0.72);
  white-space: nowrap;
}

.teacher-main {
  height: calc(100vh - 64px);
  overflow: hidden;
}

.feature-main {
  height: 100%;
  min-height: 0;
}

.contact-book-layout {
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

.quick-qa-layout {
  height: 100%;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 320px;
  gap: 12px;
}

.teacher-broadcast-card {
  height: 100%;
}

.quick-qa-editor-card,
.quick-qa-result-card {
  height: 100%;
  overflow: auto;
}

.quick-qa-right-panel {
  min-height: 0;
}

.quick-qa-leaderboard-card {
  max-height: 56%;
  overflow: auto;
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
  cursor: pointer;
}

.student-gallery-item:focus-visible {
  outline: 2px solid rgb(var(--v-theme-primary));
  outline-offset: 2px;
}

.image-tools-panel {
  height: 100%;
  overflow: auto;
}

.coedit-dialog-card {
  height: 100%;
  max-height: 100%;
  display: flex;
  flex-direction: column;
}

.coedit-dialog-content {
  flex: 1;
  min-height: 0;
  display: flex;
  padding: 8px;
}

.coedit-dialog-content :deep(.whiteboard-shell) {
  flex: 1;
  min-height: 0;
}

.coedit-dialog-title {
  padding-block: 8px;
}

.teacher-student-list-card :deep(.v-card-title > span) {
  font-size: 0.95rem;
}

.countdown-mini-card {
  margin-top: 10px;
}

.countdown-left {
  flex: 1;
  min-width: 0;
}

.countdown-title {
  font-size: 0.52rem;
  line-height: 1;
  letter-spacing: 0.08em;
}

.countdown-input :deep(.v-field__input) {
  min-height: 24px;
  padding-top: 1px;
  padding-bottom: 1px;
  text-align: center;
}

.countdown-input :deep(.v-field-label) {
  font-size: 0.68rem;
}

.countdown-controls {
  justify-content: space-between;
}

.countdown-presets-row {
  justify-content: flex-start;
}

.countdown-control-btn {
  width: 26px;
  height: 26px;
}

.countdown-preset-btn {
  min-width: 26px;
  width: 26px;
  height: 26px;
  border-radius: 50%;
  padding: 0;
}

@media (max-width: 960px) {
  .whiteboard-layout {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(0, 1fr) auto;
  }

  .quick-qa-layout {
    grid-template-columns: 1fr;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .image-tools-panel {
    height: auto;
  }

  .coedit-dialog-card {
    height: 100%;
    max-height: 100%;
  }
}
</style>
