<template>
  <div
    class="reminder-board-view fill-height d-flex flex-column rounded-xl overflow-hidden"
  >
    <!-- 背景容器 -->
    <div class="background-overlay"></div>

    <!-- 頂部：時鐘 -->
    <div class="pa-4 d-flex justify-center position-relative z-1">
      <ReminderBoardClock />
    </div>

    <!-- 中間：看板展示內容 -->
    <div class="flex-grow-1 z-1 overflow-hidden" @click="showSheet = false">
      <ReminderBoardDisplay :board="selectedBoard" />
    </div>

    <!-- 下拉清單選單 (自定義上拉區域，取代 v-bottom-sheet 以便保持分類按鈕可見) -->
    <v-expand-transition v-if="!readonly">
      <div v-if="showSheet" class="selection-panel z-10">
        <v-card
          theme="dark"
          rounded="t-xl"
          elevation="12"
          class="selection-card"
        >
          <v-card-title
            class="text-center pt-4 d-flex align-center justify-center"
          >
            <span>選擇看板項目 - {{ categoryName }}</span>
            <v-btn
              icon="mdi-close"
              variant="text"
              size="small"
              class="position-absolute right-0 mr-4"
              @click="showSheet = false"
            ></v-btn>
          </v-card-title>
          <v-container class="pb-8 overflow-y-auto" style="max-height: 50vh">
            <v-row>
              <v-col
                v-for="board in filteredBoards"
                :key="board.id || board.title"
                cols="12"
                sm="6"
                md="4"
              >
                <v-card
                  :color="
                    selectedBoard?.title === board.title
                      ? 'orange'
                      : 'grey-darken-3'
                  "
                  @click="selectBoard(board)"
                  class="pa-4 h-100 d-flex align-center"
                >
                  <v-icon size="32" class="mr-4">{{ board.icon }}</v-icon>
                  <div>
                    <div class="text-subtitle-1 font-weight-bold">
                      {{ board.title }}
                    </div>
                    <div
                      class="text-caption text-truncate"
                      style="max-width: 200px"
                    >
                      {{ board.subtitle }}
                    </div>
                  </div>
                </v-card>
              </v-col>

              <v-col
                v-if="filteredBoards.length === 0"
                cols="12"
                class="text-center py-10"
              >
                <div class="text-grey">尚無看板項目</div>
                <v-btn
                  v-if="currentCategory === '自訂'"
                  variant="text"
                  prepend-icon="mdi-plus"
                  @click="goToSettings"
                  class="mt-2"
                >
                  新增自訂看板
                </v-btn>
              </v-col>
            </v-row>
          </v-container>
        </v-card>
      </div>
    </v-expand-transition>

    <!-- 下方導航列 -->
    <ReminderBoardFooter
      v-if="!readonly"
      :is-voice-enabled="isVoiceEnabled"
      :category="currentCategory"
      @toggle-voice="isVoiceEnabled = !isVoiceEnabled"
      @change-category="changeCategory"
      @open-settings="goToSettings"
      class="z-10"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@tauri-apps/api/core";
import type { ReminderBoard } from "../types/reminderBoard";
import { PRESET_BOARDS } from "../constants/reminderBoards";
import ReminderBoardClock from "../components/ReminderBoardClock.vue";
import ReminderBoardDisplay from "../components/ReminderBoardDisplay.vue";
import ReminderBoardFooter from "../components/ReminderBoardFooter.vue";
import { useSpeechSynthesis } from "../composables/useSpeechSynthesis";

const { speak } = useSpeechSynthesis();

const REMINDER_BOARD_VOICE_STORAGE_KEY =
  "song-class.reminder-board.voice-enabled";

const isVoiceEnabled = ref(false);
const currentCategory = ref<"移動" | "作息" | "自訂" | "溫馨提醒">("移動");
const showSheet = ref(false);
const selectedBoard = ref<ReminderBoard | null>(null);
const customBoards = ref<ReminderBoard[]>([]);

const categoryName = computed(() => currentCategory.value);

const filteredBoards = computed(() => {
  if (currentCategory.value === "自訂") return customBoards.value;
  return PRESET_BOARDS.filter((b) => b.category === currentCategory.value);
});

const props = withDefaults(
  defineProps<{
    baseUrl?: string;
    readonly?: boolean;
    board?: ReminderBoard | null;
  }>(),
  {
    baseUrl: "",
    readonly: false,
    board: null,
  },
);

isVoiceEnabled.value = readStoredVoiceEnabled();

// 判斷是否應該使用 Tauri invoke
const shouldUseInvoke = () => {
  if (!isTauri()) return false;
  if (!props.baseUrl) return true;
  try {
    const url = new URL(props.baseUrl);
    return (
      url.hostname === "localhost" ||
      url.hostname === "127.0.0.1" ||
      props.baseUrl.startsWith(window.location.origin)
    );
  } catch {
    return true;
  }
};

const loadCustomBoards = async () => {
  if (shouldUseInvoke()) {
    try {
      customBoards.value = await invoke("get_reminder_boards");
      return;
    } catch (err) {
      console.error("Tauri invoke 載入失敗，嘗試 API:", err);
    }
  }

  // Fallback to HTTP API
  const base = props.baseUrl || window.location.origin;
  try {
    const response = await fetch(
      `${base}/api/reminder-boards`.replace(/([^:])\/\//g, "$1/"),
    );
    if (response.ok) {
      customBoards.value = await response.json();
    }
  } catch (err) {
    console.error("透過 API 載入自訂看板失敗:", err);
  }
};

const changeCategory = (cat: "移動" | "作息" | "自訂" | "溫馨提醒") => {
  if (currentCategory.value === cat && showSheet.value) {
    showSheet.value = false;
  } else {
    currentCategory.value = cat;
    showSheet.value = true;
  }
};

const selectBoard = (board: ReminderBoard) => {
  selectedBoard.value = board;
  showSheet.value = false;
  emit("board-selected", board);

  if (isVoiceEnabled.value) {
    const textToSpeak = `${board.title}。${board.subtitle.replace(/^##\s*/, "")}`;
    speak(textToSpeak);
  }
};

const emit = defineEmits<{
  (e: "navigate", view: string): void;
  (e: "board-selected", board: ReminderBoard | null): void;
}>();

function readStoredVoiceEnabled(): boolean {
  if (props.readonly) {
    return false;
  }

  try {
    return (
      window.localStorage.getItem(REMINDER_BOARD_VOICE_STORAGE_KEY) === "true"
    );
  } catch {
    return false;
  }
}

function persistVoiceEnabled(value: boolean) {
  if (props.readonly) {
    return;
  }

  try {
    window.localStorage.setItem(
      REMINDER_BOARD_VOICE_STORAGE_KEY,
      String(value),
    );
  } catch {
    // Ignore storage write failures.
  }
}

const goToSettings = () => {
  emit("navigate", "reminder-settings");
};

watch(
  () => props.board,
  (nextBoard) => {
    if (nextBoard) {
      selectedBoard.value = nextBoard;
      return;
    }

    if (!selectedBoard.value && filteredBoards.value.length > 0) {
      selectedBoard.value = filteredBoards.value[0];
    }
  },
  { immediate: true },
);

watch(
  selectedBoard,
  (nextBoard) => {
    if (nextBoard) {
      emit("board-selected", nextBoard);
    }
  },
  { deep: true },
);

watch(isVoiceEnabled, (nextValue) => {
  persistVoiceEnabled(nextValue);
});

onMounted(async () => {
  isVoiceEnabled.value = readStoredVoiceEnabled();

  await loadCustomBoards();

  if (props.board) {
    selectedBoard.value = props.board;
    return;
  }

  if (filteredBoards.value.length > 0) {
    selectedBoard.value = filteredBoards.value[0];
  }
});

// 當分類改變時，如果目前的看板不再範圍內，預設選第一個
watch(currentCategory, () => {
  if (filteredBoards.value.length > 0) {
    // 不自動切換，讓使用者點擊
  }
});
</script>

<style scoped>
.reminder-board-view {
  background-color: #121212;
  height: 100%;
  width: 100%;
  overflow: hidden;
  position: relative;
}

.background-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: radial-gradient(circle at center, #2c3e50 0%, #000000 100%);
  opacity: 0.8;
}

.z-1 {
  z-index: 1;
}

.z-10 {
  z-index: 10;
}

.selection-panel {
  position: absolute;
  bottom: 80px; /* 這裡要避開 footer 的高度 */
  left: 0;
  right: 0;
  background: transparent;
}

.selection-card {
  border-bottom: 2px solid rgba(255, 165, 0, 0.3);
}
</style>
