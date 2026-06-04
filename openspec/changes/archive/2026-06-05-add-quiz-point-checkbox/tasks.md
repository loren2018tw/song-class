## 1. 資料結構與後端 API 實作

- [x] 1.1 修改 `src/types/whiteboard.ts` 的 `QuickQaQuestion` 型別定義，新增 `autoAddPointsForCorrect?: boolean`。
- [x] 1.2 在 `src-tauri/src/lib.rs` 中定義 `UpdateMultipleStudentPointsRequest` 結構體。
- [x] 1.3 在 `src-tauri/src/lib.rs` 中實作 `adjust_multiple_students_points_handler` 處理函數，使用 SQLite 交易（Transaction）進行批次更新。
- [x] 1.4 在 `src-tauri/src/lib.rs` 的 Axum router 中註冊 `/api/student-points/adjust-multiple` 路由。

## 2. 教師端與學生端前端邏輯實作

- [x] 2.1 在 `src/views/WebTeacherView.vue` 中宣告 `quickQaAutoAddPoints` 的 `ref` 響應式變數，預設為 `false`。
- [x] 2.2 修改 `publishQuickQaQuestion` 函數，將 `autoAddPointsForCorrect: quickQaAutoAddPoints.value` 寫入新發布 the 題目資料結構中。
- [x] 2.3 修改 `clearQuickQaDraft` 函數，在清除草稿時將 `quickQaAutoAddPoints.value` 重置為 `false`。
- [x] 2.4 在 `src/views/WebTeacherView.vue` 中實作 `changeMultipleStudentPoints(studentIds: number[], delta: number)` 函數，呼叫 `/api/student-points/adjust-multiple` API。
- [x] 2.5 修改 `closeQuickQaQuestion` 函數，當結束作答指定正確答案且此題有開啟 `autoAddPointsForCorrect` 時，收集所有答對學生的 `student_id` 並呼叫 `changeMultipleStudentPoints` 來自動為他們加分。
- [x] 2.6 在 `src/views/WebTeacherView.vue` 佈題編輯器 UI 中（選項輸入框下方、發布按鈕上方），新增一個 `v-checkbox` 元件，雙向綁定 `quickQaAutoAddPoints`，Label 設為「答對自動積點 +1 分」。

## 3. 測試與驗證

- [x] 3.1 啟動開發伺服器進行人工驗證。
- [x] 3.2 驗證當勾選「答對自動積點 +1 分」發布題目，並設定正確答案結束作答後，答對學生是否自動獲得積點 +1。
- [x] 3.3 驗證當未勾選此選項，或結束作答時不設定正確答案時，學生積點是否不受影響。
