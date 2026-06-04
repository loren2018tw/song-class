## Context

目前教師在快問快答結束時，若設定了正確答案，答對的學生不會自動獲得積點。教師必須切換到積點模組手動加分，這降低了課堂教學的流暢度。本設計旨在快問快答發布時，提供一個「答對自動積點 +1 分」的選項。當勾選並發布題目、且在結束時指定正確答案後，系統將自動且安全地為所有答對的學生加 1 分。

## Goals / Non-Goals

**Goals:**
- 在教師端快問快答發布卡片中，新增一個「答對自動積點 +1 分」的勾選框。
- 記錄並同步此勾選狀態至題目狀態中。
- 當結束作答且設定正確答案時，自動且高效地更新所有答對學生的積點點數。
- 在後端新增批次更新積點 API，避免前端發送多個獨立 HTTP 請求造成競態條件（Race condition）。

**Non-Goals:**
- 不支援設定加分大於 1 分或其他分數的自訂設定（固定為 +1 分）。
- 學生端不需要特別顯示該題是否開啟了自動積點。

## Decisions

### 1. 後端新增批次更新點數 API
- **決策**：在後端新增 `/api/student-points/adjust-multiple` API，接收 `student_ids: Vec<i64>` 與 `delta: i64`。
- **原因**：快問快答可能有多個學生答對，在前端並行發送 N 個獨立的 `/api/student-points/adjust-student` 請求，除了增加網路開銷，最嚴重的是每次 API 回傳都會透過 `applyClassroomState` 覆寫前端的點數狀態，極易造成競態覆蓋與前端閃爍。後端使用 SQLite 交易（Transaction）批次執行更新，並只返回一次最終的 classroom state，是最安全且有效率的方案。
- **替代方案**：
  - *方案 A*：在前端序列化地（使用 `await` 一個接一個）呼叫 `/api/student-points/adjust-student`。缺點是效率極差，如果答對學生多，耗時會很長。
  - *方案 B*：在前端使用 `Promise.all` 並發呼叫。缺點是如上所述，存在嚴重的競態條件與狀態覆蓋問題。

### 2. 擴充 `QuickQaQuestion` 資料結構
- **決策**：在 `QuickQaQuestion` 型別定義中新增選用欄位 `autoAddPointsForCorrect?: boolean`。
- **原因**：為了記錄與發布此佈題狀態，該欄位必須能透過 WebSocket 廣播至所有端（特別是 Console 端與歷史記錄）。

## Risks / Trade-offs

- **[Risk]** 當班級人數多且全部答對時，後端交易寫入可能耗時較長。
  - **Mitigation**: 使用 SQLite 交易（Transaction）能將多次寫入優化為單次提交，且答對人數在一般班級（最多數十人）規模下非常輕量，寫入通常低於 10ms，因此效能影響微乎其微。
