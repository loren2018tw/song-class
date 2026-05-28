<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import NicknameJoinCard from "../components/NicknameJoinCard.vue";
import { createPeerConnection } from "../composables/usePeerConnection";
import type { SignalEnvelope } from "../types/session";

const props = defineProps<{
  baseUrl: string;
}>();

const statusText = ref("尚未連線");
const isConnected = ref(false);
const signalError = ref("");

let ws: WebSocket | null = null;
let peer: RTCPeerConnection | null = null;
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

function resetToJoinState(message = "連線已中斷，請重新加入") {
  isConnected.value = false;
  teacherId = undefined;
  selfId = undefined;
  queuedIceCandidates.length = 0;

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
    onConnectionStateChange: (state) => {
      if (state === "connected") {
        isConnected.value = true;
        statusText.value = "已連線，請專心學習";
        signalError.value = "";
        return;
      }

      if (["disconnected", "failed", "closed"].includes(state)) {
        resetToJoinState("連線已中斷，請重新加入");
      }
    },
  });

  const channel = peer.createDataChannel("lesson");
  channel.onopen = () => {
    isConnected.value = true;
    statusText.value = "已連線，請專心學習";
    signalError.value = "";
  };
  channel.onclose = () => {
    resetToJoinState("資料通道已關閉，請重新加入");
  };

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
  if (ws) {
    ws.close();
  }
  if (peer) {
    peer.close();
  }
  peer = null;
});
</script>

<template>
  <v-container class="py-8">
    <v-row justify="center">
      <v-col cols="12" md="7" lg="5">
        <NicknameJoinCard v-if="!isConnected" @submit="handleJoin" />
        <v-alert class="mt-4" type="info" variant="tonal">{{
          statusText
        }}</v-alert>
        <v-alert v-if="signalError" class="mt-3" type="error" variant="tonal">{{
          signalError
        }}</v-alert>
      </v-col>
    </v-row>
  </v-container>
</template>
