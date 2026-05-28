<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import WhiteboardCanvas from "../components/WhiteboardCanvas.vue";
import StudentListCard from "../components/StudentListCard.vue";
import { createPeerConnection } from "../composables/usePeerConnection";
import type { SignalEnvelope, StudentSession } from "../types/session";
import {
  createEmptyWhiteboardSnapshot,
  type WhiteboardSnapshot,
} from "../types/whiteboard";

const props = defineProps<{
  baseUrl: string;
}>();

const students = ref<StudentSession[]>([]);
const wsStatus = ref("尚未連線");
const rtcError = ref("");
const activeFeature = ref<"students" | "whiteboard">("students");
const whiteboardSnapshot = ref<WhiteboardSnapshot>(
  createEmptyWhiteboardSnapshot(),
);
const peers = new Map<string, RTCPeerConnection>();
const pendingCandidates = new Map<string, RTCIceCandidateInit[]>();

let ws: WebSocket | null = null;

const wsUrl = computed(() => {
  const base = new URL(props.baseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/ws";
  base.search = "?role=teacher";
  return base.toString();
});

function sendSignal(payload: SignalEnvelope) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
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

async function handleSignal(message: SignalEnvelope) {
  if (message.event === "students" || message.event === "teacher-ready") {
    const payload = message.payload as
      | { students?: StudentSession[] }
      | undefined;
    students.value = payload?.students ?? [];
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
      rtcError.value = `教師端 WebRTC 錯誤: ${String(error)}`;
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
      rtcError.value = `教師端 ICE 錯誤: ${String(error)}`;
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
  };

  ws.onmessage = async (event) => {
    const message = JSON.parse(event.data) as SignalEnvelope;
    await handleSignal(message);
  };
}

function activateWhiteboard() {
  activeFeature.value = "whiteboard";
}

function handleWhiteboardSnapshot(snapshot: WhiteboardSnapshot) {
  whiteboardSnapshot.value = snapshot;
}

onMounted(() => {
  connectTeacherSocket();
});

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
  peers.forEach((peer) => peer.close());
  peers.clear();
  pendingCandidates.clear();
});
</script>

<template>
  <v-app>
    <v-app-bar title="song-class(教師端)"></v-app-bar>
    <v-navigation-drawer :width="255">
      <div>
        <p class="text-medium-emphasis mb-0">WebSocket: {{ wsStatus }}</p>
        <p v-if="rtcError" class="text-error text-body-2 mb-0">
          {{ rtcError }}
        </p>
      </div>
      <div class="d-flex flex-wrap ga-3 align-start">
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
        <WhiteboardCanvas
          v-if="activeFeature === 'whiteboard'"
          :snapshot="whiteboardSnapshot"
          @update:snapshot="handleWhiteboardSnapshot"
        />

        <v-card
          v-else
          rounded="xl"
          elevation="6"
          class="h-100 d-flex align-center justify-center"
        >
          <v-card-text class="text-center py-16">
            <div class="text-h5 font-weight-black mb-2">功能主畫面區</div>
          </v-card-text>
        </v-card>
      </div>
    </v-main>
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
</style>
