export interface ReminderBoard {
  id?: number;
  category: "移動" | "作息" | "自訂" | "溫馨提醒";
  title: string;
  subtitle: string;
  icon: string;
}

export interface ReminderBoardState {
  isVoiceEnabled: boolean;
  currentBoard: ReminderBoard | null;
}
