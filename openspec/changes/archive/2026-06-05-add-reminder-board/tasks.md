## 1. 資料與後端基礎

- [x] 1.1 定義內建看板常數 `src/constants/reminderBoards.ts`（純代碼定義）
- [x] 1.2 建立資料庫 Migration 以建立 `reminder_boards` 資料表（僅儲存自訂內容，移除 `is_preset`）
- [x] 1.3 在 Rust 端實作自訂看板的 CRUD Tauri Commands

## 2. 介面與語音組件開發

- [x] 2.1 實作 `ReminderBoardClock.vue` 組件，處理即時更新的時間與日期
- [x] 2.2 實作 `ReminderBoardDisplay.vue` 組件，支援動態內容展示
- [x] 2.3 實作語音朗讀邏輯（`useSpeechSynthesis` composable 或直接調用 API）
- [x] 2.4 實作下方導航列，包含：
  - [x] 左側喇叭開關按鈕
  - [x] 中間類別切換（自訂、移動、作息）
  - [x] 右側設定按鈕

## 3. 功能視圖集成

- [x] 3.1 建立 `ReminderBoardView.vue` 主視圖，整合各項功能
- [x] 3.2 實作看板切換時的語音觸發邏輯
- [x] 3.3 建立 `ReminderBoardSettingsView.vue` 頁面，僅用於編輯資料庫中的自訂看板

## 4. 路由與最終整合

- [x] 4.1 在 `TeacherView.vue` 或導航選單中新增「提醒看板」入口
- [x] 4.2 進行 UI 調校，確保在全螢幕模式下的字體大小與對比度符合需求
- [x] 4.3 驗證資料持久化與重置預設值功能
