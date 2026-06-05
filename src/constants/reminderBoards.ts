import type { ReminderBoard } from "../types/reminderBoard";

export const PRESET_BOARDS: ReminderBoard[] = [
  // 移動類別
  {
    category: "移動",
    title: "到操場排隊",
    subtitle: "請攜帶水壺/毛巾，戶外活動",
    icon: "mdi-run-fast",
  },
  {
    category: "移動",
    title: "到電腦教室",
    subtitle: "資訊課程",
    icon: "mdi-laptop",
  },
  {
    category: "移動",
    title: "到走廊排隊",
    subtitle: "椅子靠好，到走廊安靜排隊",
    icon: "mdi-walk",
  },
  // 作息類別
  {
    category: "作息",
    title: "晨間閱讀",
    subtitle: "## 請安靜閱讀，享受書本樂趣",
    icon: "mdi-book-open-variant",
  },
  {
    category: "作息",
    title: "午休時間",
    subtitle: "請趴下休息，保持安靜",
    icon: "mdi-sleep",
  },
  {
    category: "作息",
    title: "老師開會中",
    subtitle: "請安靜進行班級活動",
    icon: "mdi-account-group",
  },
  // 溫馨提醒類別
  {
    category: "溫馨提醒",
    title: "口說好話",
    subtitle: "請用溫和的語氣和同學老師互動",
    icon: "mdi-heart-outline",
  },
  {
    category: "溫馨提醒",
    title: "不要做危險動作",
    subtitle: "請遵守安全規則，避免碰撞與奔跑",
    icon: "mdi-alert-circle-outline",
  },
  {
    category: "溫馨提醒",
    title: "愛惜學習用品",
    subtitle: "請珍惜教室物品，保持整潔有序",
    icon: "mdi-book-open-page-variant",
  },
];
