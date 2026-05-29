<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import StudentListCard from "../components/StudentListCard.vue";
import { useAppVersion } from "../composables/useAppVersion";
import type {
  ServerInfo,
  SignalEnvelope,
  StudentSession,
} from "../types/session";

type DebugLogEntry = {
  time: string;
  level: "info" | "warn" | "error";
  message: string;
};

const serverInfo = ref<ServerInfo>({
  status: "starting",
  ip: "127.0.0.1",
  url: "http://127.0.0.1:17860",
  error: null,
});
const students = ref<StudentSession[]>([]);
const wsStatus = ref("尚未連線");
const actionError = ref("");
const debugLogs = ref<DebugLogEntry[]>([]);

let ws: WebSocket | null = null;
const MAX_DEBUG_LOGS = 120;

const { appVersionLabel } = useAppVersion();

const wsUrl = computed(() => {
  const base = new URL(serverInfo.value.url);
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

  return `${serverInfo.value.url}/teacher`;
});

const importantRoutes = computed(() => {
  const base = serverInfo.value.url;
  return {
    base,
    teacherRedirect: `${base}/teacher`,
    studentRedirect: `${base}/student`,
    appTeacher: `${base}/teacher`,
    appStudent: `${base}/student`,
    health: `${base}/health`,
  };
});

const infoLogs = computed(() =>
  debugLogs.value.filter((entry) => entry.level === "info"),
);

function appendLog(message: string, level: DebugLogEntry["level"] = "info") {
  const next: DebugLogEntry = {
    time: new Date().toLocaleTimeString("zh-TW", { hour12: false }),
    level,
    message,
  };

  debugLogs.value = [...debugLogs.value, next].slice(-MAX_DEBUG_LOGS);
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

function handleSignal(message: SignalEnvelope) {
  if (message.event === "students" || message.event === "console-ready") {
    const payload = message.payload as
      | { students?: StudentSession[] }
      | undefined;
    students.value = payload?.students ?? [];
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

async function restartServer() {
  appendLog("restart_server");
  await invoke("start_server");
  await refreshServerInfo();
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
  connectConsoleSocket();
  appendLog(`route_teacher=${importantRoutes.value.appTeacher}`);
  appendLog(`route_student=${importantRoutes.value.appStudent}`);
});

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
});
</script>

<template>
  <v-container class="py-8">
    <v-row class="mb-4">
      <v-col cols="12" class="d-flex justify-space-between align-center">
        <div>
          <h1 class="text-h4 font-weight-black">後端主控台</h1>
          <p class="text-medium-emphasis mb-0">WebSocket: {{ wsStatus }}</p>
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

    <v-row>
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
            <div>教師入口: {{ importantRoutes.appTeacher }}</div>
            <div>學生入口: {{ importantRoutes.appStudent }}</div>
          </v-card-text>
        </v-card>
      </v-col>
      <v-col cols="12" md="5">
        <StudentListCard title="已連入學生" :students="students" />
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
</style>
