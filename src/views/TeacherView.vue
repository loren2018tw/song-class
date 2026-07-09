<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appDataDir } from "@tauri-apps/api/path";
import { ask, message, open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import StudentListCard from "../components/StudentListCard.vue";
import { useAppVersion } from "../composables/useAppVersion";
import type {
  ClassroomStatePayload,
  ClassroomStudent,
  ClassroomSummary,
  ServerInfo,
  SignalEnvelope,
  StudentSession,
} from "../types/session";

type DebugLogEntry = {
  time: string;
  level: "info" | "warn" | "error";
  message: string;
};

type ConsoleViewMode = "main" | "classrooms" | "members";

type EditableMemberRow = {
  id: number | null;
  seat_no_text: string;
  nickname: string;
};

const serverInfo = ref<ServerInfo>({
  status: "starting",
  ip: "localhost",
  url: "http://localhost:17860",
  error: null,
});
const students = ref<StudentSession[]>([]);
const classroomState = ref<ClassroomStatePayload | null>(null);
const wsStatus = ref("尚未連線");
const actionError = ref("");
const debugLogs = ref<DebugLogEntry[]>([]);
const selectedClassroomId = ref<number | null>(null);

const viewMode = ref<ConsoleViewMode>("main");
const classRenameDialogVisible = ref(false);
const classRenameDraft = ref("");
const classRenameTarget = ref<ClassroomSummary | null>(null);
const lineEnabled = ref(false);
const lineTokenDraft = ref("");
const lineSecretDraft = ref("");
const richMenuDialogVisible = ref(false);
const lineRichMenus = ref<LineRichMenuItem[]>([]);
const loadingRichMenus = ref(false);
const deletingRichMenuId = ref<string | null>(null);

interface LineRichMenuItem {
  rich_menu_id: string;
  name: string;
  chat_bar_text: string;
  selected: boolean;
}

const warningDialogVisible = ref(false);
const warningMessage = ref("");
const confirmDialogVisible = ref(false);
const confirmDialogTitle = ref("確認");
const confirmDialogMessage = ref("");
const confirmDialogConfirmText = ref("確定");

let confirmDialogResolver: ((confirmed: boolean) => void) | null = null;

const memberRows = ref<EditableMemberRow[]>([]);
const memberOriginalSnapshot = ref("");

let ws: WebSocket | null = null;
let unlistenBackupMenu: (() => void) | null = null;
let unlistenRestoreMenu: (() => void) | null = null;
const MAX_DEBUG_LOGS = 120;

defineEmits<{
  (e: "navigate", view: string): void;
}>();

const { appVersionLabel } = useAppVersion();

const consoleBackendUrl = computed(() => {
  const local = new URL(serverInfo.value.url);
  local.hostname = "localhost";
  return local.toString();
});

const wsUrl = computed(() => {
  const base = new URL(consoleBackendUrl.value);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=console";
  return base.toString();
});

const teacherBrowserUrl = computed(() => {
  if (import.meta.env.DEV) {
    const base = new URL(window.location.origin);
    base.pathname = "/";
    base.searchParams.set("mode", "teacher");
    base.searchParams.set("base", serverInfo.value.url);
    return base.toString();
  }

  const localTeacherUrl = new URL(serverInfo.value.url);
  localTeacherUrl.hostname = "localhost";
  localTeacherUrl.pathname = "/teacher";
  localTeacherUrl.searchParams.set("base", serverInfo.value.url);
  return localTeacherUrl.toString();
});

const importantRoutes = computed(() => {
  const base = serverInfo.value.url;
  const teacherUrl = new URL(base);
  teacherUrl.hostname = "localhost";
  teacherUrl.pathname = "/teacher";
  teacherUrl.searchParams.set("base", base);

  const studentUrl = new URL("/student", base);

  return {
    appTeacher: teacherUrl.toString(),
    appStudent: studentUrl.toString(),
  };
});

const infoLogs = computed(() =>
  debugLogs.value.filter((entry) => entry.level === "info"),
);

const classItems = computed(() =>
  (classroomState.value?.classrooms ?? []).map((item) => ({
    title: item.name,
    value: item.id,
  })),
);

const hasMemberChanges = computed(
  () => JSON.stringify(memberRows.value) !== memberOriginalSnapshot.value,
);

function appendLog(message: string, level: DebugLogEntry["level"] = "info") {
  const next: DebugLogEntry = {
    time: new Date().toLocaleTimeString("zh-TW", { hour12: false }),
    level,
    message,
  };

  debugLogs.value = [...debugLogs.value, next].slice(-MAX_DEBUG_LOGS);
}

function openWarning(message: string) {
  warningMessage.value = message;
  warningDialogVisible.value = true;
}

function openConfirmDialog(options: {
  title?: string;
  message: string;
  confirmText?: string;
}): Promise<boolean> {
  confirmDialogTitle.value = options.title ?? "確認";
  confirmDialogMessage.value = options.message;
  confirmDialogConfirmText.value = options.confirmText ?? "確定";
  confirmDialogVisible.value = true;

  return new Promise((resolve) => {
    confirmDialogResolver = resolve;
  });
}

function resolveConfirmDialog(confirmed: boolean) {
  confirmDialogVisible.value = false;
  if (confirmDialogResolver) {
    confirmDialogResolver(confirmed);
    confirmDialogResolver = null;
  }
}

function toEditableRows(studentsList: ClassroomStudent[]): EditableMemberRow[] {
  return studentsList.map((student) => ({
    id: student.id,
    seat_no_text: student.seat_no_text,
    nickname: student.nickname,
  }));
}

async function apiGet<T>(path: string): Promise<T> {
  const response = await fetch(new URL(path, consoleBackendUrl.value));
  if (!response.ok) {
    throw new Error(await response.text());
  }

  return (await response.json()) as T;
}

async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(new URL(path, consoleBackendUrl.value), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }

  return (await response.json()) as T;
}

async function apiPatch<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(new URL(path, consoleBackendUrl.value), {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }

  return (await response.json()) as T;
}

async function apiDelete<T>(path: string): Promise<T> {
  const response = await fetch(new URL(path, consoleBackendUrl.value), {
    method: "DELETE",
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }

  return (await response.json()) as T;
}

function applyClassroomState(state: ClassroomStatePayload) {
  classroomState.value = state;
  selectedClassroomId.value = state.current_classroom.id;

  if (viewMode.value === "members") {
    memberRows.value = toEditableRows(state.students);
    memberOriginalSnapshot.value = JSON.stringify(memberRows.value);
  }
}

async function refreshClassroomState() {
  try {
    const state = await apiGet<ClassroomStatePayload>("/api/classroom/state");
    applyClassroomState(state);
    actionError.value = "";
    appendLog(`classroom_state_loaded: count=${state.classrooms.length}`);
  } catch (error) {
    actionError.value = `載入班級狀態失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

async function refreshServerInfo() {
  serverInfo.value = await invoke<ServerInfo>("get_server_info");
  appendLog(
    `server_info: status=${serverInfo.value.status}, url=${serverInfo.value.url}, ip=${serverInfo.value.ip}`,
  );
}

async function openTeacherInBrowser() {
  actionError.value = "";
  appendLog(`open_teacher: ${teacherBrowserUrl.value}`);
  try {
    await openUrl(teacherBrowserUrl.value);
  } catch (error) {
    actionError.value = `開啟教師端失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

async function backupDatabase() {
  try {
    const destinationDir = await openDialog({
      directory: true,
      multiple: false,
      title: "選擇備份資料庫位置",
      defaultPath: await appDataDir(),
    });

    if (!destinationDir) {
      return;
    }

    const backupPath = await invoke<string>("backup_database", {
      destinationDir,
    });
    await message(`備份完成：${backupPath}`, {
      title: "備份資料庫",
      kind: "info",
    });
    actionError.value = "";
    appendLog(`backup_database: ${backupPath}`);
  } catch (error) {
    actionError.value = `備份資料庫失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

async function restoreDatabase() {
  const confirmed = await ask("回存資料庫會覆蓋目前資料庫資料，是否繼續？", {
    title: "警告",
    kind: "warning",
    okLabel: "繼續",
    cancelLabel: "取消",
  });

  if (!confirmed) {
    return;
  }

  try {
    const sourcePath = await openDialog({
      title: "選擇要回存的資料庫檔案",
      defaultPath: await appDataDir(),
      filters: [
        { name: "SQLite 資料庫", extensions: ["sqlite", "sqlite3", "db"] },
      ],
    });

    if (!sourcePath) {
      return;
    }

    await invoke("restore_database", { sourcePath });
    await refreshClassroomState();
    actionError.value = "";
    appendLog(`restore_database: ${sourcePath}`);
  } catch (error) {
    actionError.value = `回存資料庫失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

function handleSignal(message: SignalEnvelope) {
  if (message.event === "students" || message.event === "console-ready") {
    const payload = message.payload as
      | { students?: StudentSession[] }
      | undefined;
    students.value = payload?.students ?? [];
  }

  if (message.event === "classroom-state") {
    const payload = message.payload as { state?: ClassroomStatePayload };
    if (payload?.state) {
      applyClassroomState(payload.state);
    }
  }
}

function connectConsoleSocket() {
  if (ws) {
    ws.close();
  }

  ws = new WebSocket(wsUrl.value);
  wsStatus.value = "連線中";
  appendLog(`ws_connecting: ${wsUrl.value}`);

  ws.onopen = () => {
    wsStatus.value = "已連線";
    appendLog("ws_connected");
  };

  ws.onclose = () => {
    wsStatus.value = "已中斷";
    appendLog("ws_closed", "warn");
  };

  ws.onerror = () => {
    wsStatus.value = "發生錯誤";
    appendLog("ws_error", "error");
  };

  ws.onmessage = (event) => {
    const message = JSON.parse(event.data) as SignalEnvelope;
    handleSignal(message);
  };
}

async function switchClassroom() {
  if (!selectedClassroomId.value) {
    return;
  }

  try {
    const state = await apiPost<ClassroomStatePayload>(
      "/api/classrooms/select",
      { classroom_id: selectedClassroomId.value },
    );
    applyClassroomState(state);
    appendLog(`switch_classroom: ${state.current_classroom.name}`);
  } catch (error) {
    actionError.value = `切換班級失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

function openClassroomEditor() {
  viewMode.value = "classrooms";
}

function openClassMembersEditor() {
  if (!classroomState.value) {
    return;
  }

  memberRows.value = toEditableRows(classroomState.value.students);
  memberOriginalSnapshot.value = JSON.stringify(memberRows.value);
  viewMode.value = "members";
}

function backToMain() {
  viewMode.value = "main";
}

async function createClassroom() {
  try {
    const state = await apiPost<ClassroomStatePayload>("/api/classrooms", {});
    applyClassroomState(state);
    appendLog("create_classroom");
  } catch (error) {
    actionError.value = `新增班級失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

function openClassRenameDialog(classroom: ClassroomSummary) {
  classRenameTarget.value = classroom;
  classRenameDraft.value = classroom.name;
  lineEnabled.value = classroom.line_enabled;
  lineTokenDraft.value = classroom.line_channel_access_token;
  lineSecretDraft.value = classroom.line_channel_secret;
  classRenameDialogVisible.value = true;
}

async function openRichMenuDialog() {
  if (!classRenameTarget.value) return;
  richMenuDialogVisible.value = true;
  loadingRichMenus.value = true;
  lineRichMenus.value = [];
  try {
    const menus = await apiGet<LineRichMenuItem[]>(
      `/api/contact-book/line-richmenus/${classRenameTarget.value.id}`,
    );
    lineRichMenus.value = menus;
  } catch (error) {
    actionError.value = `查詢 Rich Menu 失敗: ${String(error)}`;
  } finally {
    loadingRichMenus.value = false;
  }
}

async function deleteLineRichMenu(richMenuId: string) {
  if (!classRenameTarget.value) return;
  deletingRichMenuId.value = richMenuId;
  try {
    await apiDelete(
      `/api/contact-book/line-richmenus/${classRenameTarget.value.id}/${richMenuId}`,
    );
    lineRichMenus.value = lineRichMenus.value.filter(
      (m) => m.rich_menu_id !== richMenuId,
    );
  } catch (error) {
    actionError.value = `刪除 Rich Menu 失敗: ${String(error)}`;
  } finally {
    deletingRichMenuId.value = null;
  }
}

async function confirmClassRename() {
  if (!classRenameTarget.value) {
    return;
  }

  try {
    const body: Record<string, unknown> = { name: classRenameDraft.value };
    body.line_enabled = lineEnabled.value;
    if (lineTokenDraft.value) {
      body.line_channel_access_token = lineTokenDraft.value;
    }
    if (lineSecretDraft.value) {
      body.line_channel_secret = lineSecretDraft.value;
    }
    const state = await apiPatch<ClassroomStatePayload>(
      `/api/classrooms/${classRenameTarget.value.id}`,
      body,
    );
    applyClassroomState(state);
    appendLog(`rename_classroom: ${classRenameTarget.value.id}`);
    classRenameDialogVisible.value = false;
  } catch (error) {
    actionError.value = `更新班級名稱失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

async function deleteClassroom(classroom: ClassroomSummary) {
  const classes = classroomState.value?.classrooms ?? [];
  if (classes.length <= 1) {
    openWarning("無法刪除唯一的班級");
    return;
  }

  const confirmed = await openConfirmDialog({
    title: "刪除班級",
    message: `確定刪除班級「${classroom.name}」？`,
    confirmText: "刪除",
  });
  if (!confirmed) {
    return;
  }

  try {
    const state = await apiDelete<ClassroomStatePayload>(
      `/api/classrooms/${classroom.id}`,
    );
    applyClassroomState(state);
    appendLog(`delete_classroom: ${classroom.id}`);
  } catch (error) {
    actionError.value = `刪除班級失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

function addMemberRow() {
  memberRows.value.push({
    id: null,
    seat_no_text: "",
    nickname: "",
  });
}

function removeMemberRow(index: number) {
  memberRows.value.splice(index, 1);
}

async function saveMembers() {
  if (!classroomState.value) {
    return;
  }

  try {
    const state = await apiPost<ClassroomStatePayload>(
      `/api/classrooms/${classroomState.value.current_classroom.id}/students/bulk-save`,
      { students: memberRows.value },
    );
    applyClassroomState(state);
    memberOriginalSnapshot.value = JSON.stringify(memberRows.value);
    viewMode.value = "main";
    appendLog("save_class_members");
  } catch (error) {
    actionError.value = `儲存學生資料失敗: ${String(error)}`;
    appendLog(actionError.value, "error");
  }
}

async function cancelMembersEdit() {
  if (hasMemberChanges.value) {
    const confirmed = await openConfirmDialog({
      title: "取消編輯",
      message: "尚未儲存變更，確定取消編輯？",
      confirmText: "放棄變更",
    });
    if (!confirmed) {
      return;
    }
  }

  viewMode.value = "main";
}

async function restartServer() {
  appendLog("restart_server");
  await invoke("start_server");
  await refreshServerInfo();
  await refreshClassroomState();
  connectConsoleSocket();
}

async function stopServer() {
  appendLog("stop_server");
  await invoke("stop_server");
  await refreshServerInfo();
  if (ws) {
    ws.close();
  }
}

function clearLogs() {
  debugLogs.value = [];
  appendLog("debug_logs_cleared");
}

onMounted(async () => {
  appendLog("console_bootstrap_start");
  await refreshServerInfo();
  if (serverInfo.value.status !== "running") {
    await invoke("start_server");
    await refreshServerInfo();
  }

  await refreshClassroomState();
  connectConsoleSocket();
  appendLog(`route_teacher=${importantRoutes.value.appTeacher}`);
  appendLog(`route_student=${importantRoutes.value.appStudent}`);

  unlistenBackupMenu = await listen("menu-backup-database", () => {
    void backupDatabase();
  });
  unlistenRestoreMenu = await listen("menu-restore-database", () => {
    void restoreDatabase();
  });
});

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
  unlistenBackupMenu?.();
  unlistenRestoreMenu?.();
});
</script>

<template>
  <v-container class="pt-2 pb-8">
    <v-row class="mb-2">
      <v-col cols="12" class="d-flex justify-space-between align-center">
        <div>
          <h1 class="text-h4 font-weight-black mt-0">後端主控台</h1>
          <p class="text-medium-emphasis mb-0">WebSocket: {{ wsStatus }}</p>
          <p class="text-medium-emphasis mb-0">
            目前班級: {{ classroomState?.current_classroom.name ?? "載入中" }}
          </p>
          <p v-if="actionError" class="text-error text-body-2 mb-0">
            {{ actionError }}
          </p>
        </div>
        <div class="d-flex flex-column align-end ga-2">
          <p class="text-caption text-medium-emphasis mb-0">
            {{ appVersionLabel }}
          </p>
          <div class="d-flex ga-2">
            <v-btn color="primary" @click="openTeacherInBrowser"
              >開啟教師端</v-btn
            >
            <v-btn color="secondary" variant="outlined" @click="stopServer"
              >停止服務</v-btn
            >
            <v-btn color="primary" variant="outlined" @click="restartServer"
              >重新啟動</v-btn
            >
          </div>
        </div>
      </v-col>
    </v-row>

    <v-row v-if="viewMode === 'main'">
      <v-col cols="12" md="7">
        <v-card rounded="xl" elevation="6" class="h-100">
          <v-card-title>連線入口資訊</v-card-title>
          <v-card-text>
            <v-alert
              v-if="serverInfo.error"
              type="error"
              variant="tonal"
              class="mb-3"
            >
              {{ serverInfo.error }}
            </v-alert>
            <div class="mb-3">教師入口: {{ importantRoutes.appTeacher }}</div>
            <div class="mb-5">學生入口: {{ importantRoutes.appStudent }}</div>

            <div class="d-flex flex-wrap ga-2 align-center mb-3">
              <v-select
                v-model="selectedClassroomId"
                label="班級下選單"
                :items="classItems"
                density="comfortable"
                variant="outlined"
                hide-details="auto"
                class="class-select"
                @update:model-value="switchClassroom"
              />
              <v-btn
                color="primary"
                variant="tonal"
                @click="openClassroomEditor"
              >
                編輯班級
              </v-btn>
              <v-btn
                color="primary"
                variant="outlined"
                @click="openClassMembersEditor"
              >
                編輯班級成員
              </v-btn>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
      <v-col cols="12" md="5">
        <StudentListCard title="已連入學生" :students="students" />
      </v-col>
    </v-row>

    <v-row v-else-if="viewMode === 'classrooms'">
      <v-col cols="12">
        <v-card rounded="xl" elevation="6">
          <v-card-title class="d-flex justify-space-between align-center">
            <span>班級編輯畫面</span>
            <v-btn variant="text" @click="backToMain">返回主控端</v-btn>
          </v-card-title>
          <v-card-text>
            <div class="mb-4">
              <v-btn color="primary" @click="createClassroom">新增班級</v-btn>
            </div>

            <v-list lines="one" density="comfortable" class="border rounded">
              <v-list-item
                v-for="classroom in classroomState?.classrooms ?? []"
                :key="classroom.id"
                :title="classroom.name"
              >
                <template #append>
                  <div class="d-flex ga-2">
                    <v-btn
                      size="small"
                      variant="tonal"
                      color="primary"
                      @click="openClassRenameDialog(classroom)"
                    >
                      編輯
                    </v-btn>
                    <v-btn
                      size="small"
                      variant="outlined"
                      color="error"
                      @click="deleteClassroom(classroom)"
                    >
                      刪除
                    </v-btn>
                  </div>
                </template>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-row v-else>
      <v-col cols="12">
        <v-card rounded="xl" elevation="6">
          <v-card-title class="d-flex justify-space-between align-center">
            <span>編輯班級成員</span>
            <v-btn variant="text" @click="cancelMembersEdit">返回主控端</v-btn>
          </v-card-title>
          <v-card-text>
            <div class="member-grid-head mb-2">
              <div>座號</div>
              <div>暱稱</div>
              <div class="text-center">操作</div>
            </div>

            <div
              v-for="(member, index) in memberRows"
              :key="member.id ?? `new-${index}`"
              class="member-grid-row"
            >
              <v-text-field
                v-model="member.seat_no_text"
                density="compact"
                variant="outlined"
                hide-details
                placeholder="座號"
                class="member-input seat-input"
              />
              <v-text-field
                v-model="member.nickname"
                density="compact"
                variant="outlined"
                hide-details
                placeholder="暱稱"
                class="member-input nickname-input"
              />
              <v-btn
                size="x-small"
                color="error"
                variant="outlined"
                @click="removeMemberRow(index)"
              >
                刪除
              </v-btn>
            </div>

            <div class="d-flex flex-wrap ga-2 mt-4">
              <v-btn color="primary" variant="tonal" @click="addMemberRow"
                >新增學生</v-btn
              >
              <v-btn color="primary" @click="saveMembers">儲存</v-btn>
              <v-btn variant="outlined" @click="cancelMembersEdit"
                >取消編輯</v-btn
              >
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-row class="mt-1">
      <v-col cols="12">
        <v-card variant="outlined">
          <v-card-title>除錯 Log</v-card-title>
          <v-card-text class="pt-0">
            <div class="debug-log-box">
              <div
                v-for="(entry, index) in infoLogs"
                :key="`${entry.time}-info-${index}`"
                class="debug-log-line"
              >
                <span class="debug-log-time">[{{ entry.time }}]</span>
                <span class="debug-log-info">info</span>
                <span>{{ entry.message }}</span>
              </div>
              <div v-if="infoLogs.length === 0" class="debug-log-time">
                目前沒有 info 訊息
              </div>
            </div>
            <div class="mt-2 d-flex justify-end">
              <v-btn size="small" variant="text" @click="clearLogs">清空</v-btn>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-dialog v-model="classRenameDialogVisible" max-width="560">
      <v-card>
        <v-card-title>班級設定</v-card-title>
        <v-card-text>
          <v-text-field
            v-model="classRenameDraft"
            label="班級名稱"
            variant="outlined"
            density="comfortable"
            hide-details="auto"
          />
          <v-divider class="my-4" />
          <div class="text-subtitle-2 mb-2 text-medium-emphasis">
            LINE 官方帳號設定
          </div>
          <v-switch
            v-model="lineEnabled"
            label="啟用 LINE 同步"
            color="primary"
            density="compact"
            hide-details
            class="mb-2"
          />
          <v-text-field
            v-model="lineTokenDraft"
            label="Channel Access Token"
            variant="outlined"
            density="comfortable"
            hint="輸入新值以覆寫，留空保留原值"
            persistent-hint
            :type="lineTokenDraft.includes('****') ? 'password' : 'text'"
          />
          <v-text-field
            v-model="lineSecretDraft"
            label="Channel Secret"
            variant="outlined"
            density="comfortable"
            hint="輸入新值以覆寫，留空保留原值"
            persistent-hint
            :type="lineSecretDraft.includes('****') ? 'password' : 'text'"
          />
          <v-btn
            variant="outlined"
            size="small"
            color="secondary"
            prepend-icon="mdi-menu-open"
            class="mt-2"
            @click="openRichMenuDialog"
            >管理 Rich Menu</v-btn
          >
        </v-card-text>
        <v-card-actions class="justify-end">
          <v-btn variant="text" @click="classRenameDialogVisible = false"
            >取消</v-btn
          >
          <v-btn color="primary" @click="confirmClassRename">儲存</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="richMenuDialogVisible" max-width="600">
      <v-card>
        <v-card-title>Rich Menu 管理</v-card-title>
        <v-card-text>
          <v-alert
            v-if="actionError"
            type="error"
            closable
            class="mb-3"
            @click:close="actionError = ''"
          >
            {{ actionError }}
          </v-alert>
          <v-progress-linear v-if="loadingRichMenus" indeterminate class="mb-3" />
          <div v-else-if="lineRichMenus.length === 0" class="text-medium-emphasis">
            目前沒有 Rich Menu
          </div>
          <v-list v-else lines="two">
            <v-list-item
              v-for="menu in lineRichMenus"
              :key="menu.rich_menu_id"
              :title="menu.name || '(未命名)'"
              :subtitle="`${menu.rich_menu_id}${
                menu.selected ? ' (預設)' : ''
              }`"
            >
              <template #append>
                <v-btn
                  variant="text"
                  color="error"
                  icon="mdi-delete"
                  :loading="deletingRichMenuId === menu.rich_menu_id"
                  :disabled="deletingRichMenuId !== null"
                  @click="deleteLineRichMenu(menu.rich_menu_id)"
                />
              </template>
            </v-list-item>
          </v-list>
        </v-card-text>
        <v-card-actions class="justify-end">
          <v-btn variant="text" @click="richMenuDialogVisible = false"
            >關閉</v-btn
          >
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="warningDialogVisible" max-width="460">
      <v-card>
        <v-card-title>警告</v-card-title>
        <v-card-text>{{ warningMessage }}</v-card-text>
        <v-card-actions class="justify-end">
          <v-btn color="primary" @click="warningDialogVisible = false"
            >知道了</v-btn
          >
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="confirmDialogVisible" max-width="480">
      <v-card>
        <v-card-title>{{ confirmDialogTitle }}</v-card-title>
        <v-card-text>{{ confirmDialogMessage }}</v-card-text>
        <v-card-actions class="justify-end">
          <v-btn variant="text" @click="resolveConfirmDialog(false)"
            >取消</v-btn
          >
          <v-btn color="error" @click="resolveConfirmDialog(true)">{{
            confirmDialogConfirmText
          }}</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-container>
</template>

<style scoped>
.debug-log-box {
  max-height: 220px;
  overflow-y: auto;
  border: 1px solid rgba(60, 60, 60, 0.2);
  border-radius: 8px;
  padding: 10px;
  font-family: "Noto Sans Mono CJK TC", "JetBrains Mono", monospace;
  font-size: 12px;
  line-height: 1.5;
  background: rgba(22, 30, 45, 0.03);
}

.debug-log-line {
  display: flex;
  gap: 8px;
  align-items: baseline;
}

.debug-log-time {
  color: rgba(40, 40, 40, 0.75);
}

.debug-log-info {
  color: #166534;
  min-width: 34px;
}

.class-select {
  min-width: 280px;
  max-width: 420px;
}

.member-grid-head,
.member-grid-row {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr) 72px;
  gap: 8px;
  align-items: center;
}

.member-grid-row {
  margin-bottom: 8px;
}

:deep(.member-input .v-field) {
  min-height: 34px;
}

:deep(.member-input .v-field__input) {
  min-height: 34px;
  padding-top: 4px;
  padding-bottom: 4px;
  font-size: 13px;
}

@media (max-width: 840px) {
  .member-grid-head,
  .member-grid-row {
    grid-template-columns: 82px minmax(0, 1fr) 64px;
    gap: 6px;
  }
}
</style>
