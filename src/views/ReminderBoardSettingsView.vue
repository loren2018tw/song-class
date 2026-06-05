<template>
  <v-container class="fill-height">
    <v-row justify="center">
      <v-col cols="12" md="8">
        <v-card theme="dark" flat border>
          <v-toolbar color="grey-darken-4">
            <v-btn icon="mdi-arrow-left" @click="goBack"></v-btn>
            <v-toolbar-title>設定：自訂看板項目</v-toolbar-title>
            <v-spacer></v-spacer>
            <v-btn color="orange" prepend-icon="mdi-plus" @click="openDialog()"
              >新增</v-btn
            >
          </v-toolbar>

          <v-list class="bg-transparent">
            <v-list-item
              v-for="board in customBoards"
              :key="board.id"
              :prepend-icon="board.icon"
            >
              <v-list-item-title class="font-weight-bold">{{
                board.title
              }}</v-list-item-title>
              <v-list-item-subtitle>{{ board.subtitle }}</v-list-item-subtitle>

              <template v-slot:append>
                <v-btn
                  icon="mdi-pencil"
                  variant="text"
                  size="small"
                  @click="openDialog(board)"
                ></v-btn>
                <v-btn
                  icon="mdi-delete"
                  variant="text"
                  size="small"
                  color="error"
                  @click="deleteBoard(board.id!)"
                ></v-btn>
              </template>
            </v-list-item>

            <v-list-item
              v-if="customBoards.length === 0"
              class="text-center py-10 opacity-50"
            >
              尚無自訂看板，點擊上方按鈕新增。
            </v-list-item>
          </v-list>
        </v-card>
      </v-col>
    </v-row>

    <!-- 編輯/新增對話框 -->
    <v-dialog v-model="dialog.show" max-width="500px">
      <v-card theme="dark">
        <v-card-title>{{
          dialog.isEdit ? "修改看板" : "新增自訂看板"
        }}</v-card-title>
        <v-card-text>
          <v-text-field
            v-model="dialog.item.title"
            label="大標題"
            placeholder="例如：到操場排隊"
          ></v-text-field>
          <v-textarea
            v-model="dialog.item.subtitle"
            label="副標題"
            placeholder="例如：請攜帶水壺"
            rows="3"
          ></v-textarea>
          <v-select
            v-model="dialog.item.icon"
            label="選擇圖示"
            :items="COMMON_ICONS"
            item-title="title"
            item-value="value"
            prepend-icon="mdi-image-search"
          >
            <template v-slot:selection="{ item }">
              <v-icon
                :icon="
                  (item as any)?.value ??
                  (item as any)?.raw?.value ??
                  'mdi-help'
                "
                class="mr-2"
              ></v-icon>
              {{ (item as any)?.title ?? (item as any)?.raw?.title }}
            </template>
          </v-select>
          <div class="text-caption text-grey ml-8">
            預覽: <v-icon :icon="dialog.item.icon || 'mdi-help'"></v-icon>
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn variant="text" @click="dialog.show = false">取消</v-btn>
          <v-btn color="orange" @click="saveBoard">儲存</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-container>
</template>

<script setup lang="ts">
import { ref, onMounted, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@tauri-apps/api/core";
import type { ReminderBoard } from "../types/reminderBoard";

const customBoards = ref<ReminderBoard[]>([]);

const COMMON_ICONS = [
  { title: "跑跑", value: "mdi-run-fast" },
  { title: "電腦", value: "mdi-laptop" },
  { title: "書本", value: "mdi-book-open-variant" },
  { title: "睡覺", value: "mdi-sleep" },
  { title: "蘋果", value: "mdi-food-apple" },
  { title: "揮手", value: "mdi-hand-wave" },
  { title: "鉛筆", value: "mdi-pencil" },
  { title: "音樂", value: "mdi-music" },
  { title: "小組", value: "mdi-account-group" },
  { title: "警告", value: "mdi-alert-circle" },
  { title: "時鐘", value: "mdi-clock-outline" },
  { title: "畫筆", value: "mdi-brush" },
  { title: "籃球", value: "mdi-basketball" },
  { title: "燒瓶", value: "mdi-flask-outline" },
  { title: "房子", value: "mdi-home" },
  { title: "水壺", value: "mdi-water-outline" },
  { title: "洗手間", value: "mdi-human-male-female" },
];

const dialog = reactive({
  show: false,
  isEdit: false,
  item: {
    id: null as number | null,
    category: "自訂" as any,
    title: "",
    subtitle: "",
    icon: "mdi-run-fast",
  },
});

const props = withDefaults(
  defineProps<{
    baseUrl?: string;
  }>(),
  {
    baseUrl: "",
  },
);

// 判斷是否應該使用 Tauri invoke
const shouldUseInvoke = () => {
  if (!isTauri()) return false;
  // 如果沒有 baseUrl，或者 baseUrl 指向本地開發環境，或者 baseUrl 就是目前的 origin，則使用 invoke
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

const loadBoards = async () => {
  if (shouldUseInvoke()) {
    try {
      customBoards.value = await invoke("get_reminder_boards");
      return;
    } catch (err) {
      console.error("Tauri 載入失敗:", err);
    }
  }

  const base = props.baseUrl || window.location.origin;
  try {
    const response = await fetch(
      `${base}/api/reminder-boards`.replace(/([^:])\/\//g, "$1/"),
    );
    if (response.ok) {
      customBoards.value = await response.json();
    }
  } catch (err) {
    console.error("透過 API 載入看板失敗:", err);
  }
};

const openDialog = (board?: ReminderBoard) => {
  if (board) {
    dialog.isEdit = true;
    dialog.item = { ...board, id: board.id ?? null, category: "自訂" };
  } else {
    dialog.isEdit = false;
    dialog.item = {
      id: null,
      category: "自訂",
      title: "",
      subtitle: "",
      icon: "mdi-run-fast",
    };
  }
  dialog.show = true;
};

const saveBoard = async () => {
  try {
    if (shouldUseInvoke()) {
      if (dialog.isEdit) {
        await invoke("update_reminder_board", { board: dialog.item });
      } else {
        await invoke("create_reminder_board", { board: dialog.item });
      }
    } else {
      const base = props.baseUrl || window.location.origin;
      const url = dialog.isEdit
        ? `${base}/api/reminder-boards/${dialog.item.id}`
        : `${base}/api/reminder-boards`;
      const method = dialog.isEdit ? "PATCH" : "POST";
      const response = await fetch(url.replace(/([^:])\/\//g, "$1/"), {
        method,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(dialog.item),
      });
      if (!response.ok) throw new Error(await response.text());
    }

    dialog.show = false;
    await loadBoards();
  } catch (err) {
    alert("儲存失敗：" + err);
  }
};

const deleteBoard = async (id: number) => {
  if (!confirm("確定要刪除此看板嗎？")) return;
  try {
    if (shouldUseInvoke()) {
      await invoke("delete_reminder_board", { id });
    } else {
      const base = props.baseUrl || window.location.origin;
      const response = await fetch(
        `${base}/api/reminder-boards/${id}`.replace(/([^:])\/\//g, "$1/"),
        {
          method: "DELETE",
        },
      );
      if (!response.ok) throw new Error(await response.text());
    }
    await loadBoards();
  } catch (err) {
    alert("刪除失敗：" + err);
  }
};

onMounted(loadBoards);

const emit = defineEmits<{
  (e: "navigate", view: string): void;
}>();

const goBack = () => {
  emit("navigate", "main");
};
</script>
