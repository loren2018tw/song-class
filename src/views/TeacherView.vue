<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import ConnectionInfoCard from "../components/ConnectionInfoCard.vue";
import StudentListCard from "../components/StudentListCard.vue";
import type {
  ServerInfo,
  SignalEnvelope,
  StudentSession,
} from "../types/session";

const serverInfo = ref<ServerInfo>({
  status: "starting",
  ip: "127.0.0.1",
  url: "http://127.0.0.1:17860",
  error: null,
});
const students = ref<StudentSession[]>([]);
const wsStatus = ref("尚未連線");
const actionError = ref("");

let ws: WebSocket | null = null;

const wsUrl = computed(() => {
  const base = new URL(serverInfo.value.url);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=console";
  return base.toString();
});

const serviceLabel = computed(() => {
  switch (serverInfo.value.status) {
    case "running":
      return "可連線";
    case "starting":
      return "啟動中";
    case "error":
      return "錯誤";
    default:
      return "未啟動";
  }
});

const teacherBrowserUrl = computed(() => {
  if (import.meta.env.DEV) {
    const base = new URL(window.location.origin);
    base.searchParams.set("mode", "teacher");
    base.searchParams.set("base", serverInfo.value.url);
    return base.toString();
  }

  return `${serverInfo.value.url}/teacher`;
});

async function refreshServerInfo() {
  serverInfo.value = await invoke<ServerInfo>("get_server_info");
}

async function openTeacherInBrowser() {
  actionError.value = "";
  try {
    await openUrl(teacherBrowserUrl.value);
  } catch (error) {
    actionError.value = `開啟教師端失敗: ${String(error)}`;
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

  ws.onopen = () => {
    wsStatus.value = "已連線";
  };

  ws.onclose = () => {
    wsStatus.value = "已中斷";
  };

  ws.onerror = () => {
    wsStatus.value = "發生錯誤";
  };

  ws.onmessage = (event) => {
    const message = JSON.parse(event.data) as SignalEnvelope;
    handleSignal(message);
  };
}

async function restartServer() {
  await invoke("start_server");
  await refreshServerInfo();
  connectConsoleSocket();
}

async function stopServer() {
  await invoke("stop_server");
  await refreshServerInfo();
  if (ws) {
    ws.close();
  }
}

onMounted(async () => {
  await refreshServerInfo();
  if (serverInfo.value.status !== "running") {
    await invoke("start_server");
    await refreshServerInfo();
  }
  connectConsoleSocket();
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
      </v-col>
    </v-row>

    <v-row>
      <v-col cols="12" md="7">
        <ConnectionInfoCard
          title="連線入口資訊"
          :status-label="serviceLabel"
          :server-url="serverInfo.url"
          :ip="serverInfo.ip"
          :error-message="serverInfo.error"
        />
      </v-col>
      <v-col cols="12" md="5">
        <StudentListCard title="已連入學生" :students="students" />
      </v-col>
    </v-row>
  </v-container>
</template>
