## Why

本變更旨在為教師提供更便利的課堂互動與獎勵機制。在目前的快問快答模組中，答對的學生若要獲得積點，教師需要手動逐一或批次進行積點調整。新增「答對自動積點」的功能，能讓系統在教師公布正確答案時，自動為答對的學生進行積點 +1 分，大幅降低教師的操作負擔，流暢課堂教學節奏。

## What Changes

- **佈題頁面新增勾選框**：在教師端的快問快答佈題介面上，新增一個名為「答對自動積點 +1 分」的勾選框。
- **作答結束自動積點**：
  - 當教師勾選此選項並發布題目，且在結束作答時設定了「正確答案」，系統將自動為所有答對（即提交答案與正確答案一致）的學生積點 +1 分。
  - 若教師發布題目時未勾選此選項，或在結束作答時選擇「不設定正確答案」，則不會觸發任何自動積點邏輯。
- **資料結構調整**：快問快答題目狀態（`QuickQaQuestion`）將新增 `autoAddPointsForCorrect`（布林值）屬性，用以同步此勾選狀態。

## Capabilities

### New Capabilities
<!-- Capabilities being introduced. Replace <name> with kebab-case identifier (e.g., user-auth, data-export, api-rate-limiting). Each creates specs/<name>/spec.md -->

### Modified Capabilities
<!-- Existing capabilities whose REQUIREMENTS are changing (not just implementation).
     Only list here if spec-level behavior changes. Each needs a delta spec file.
     Use existing spec names from openspec/specs/. Leave empty if no requirement changes.
-->
- `quick-qa-session-management`: 在快問快答生命週期中，新增「答對自動積點」佈題選項，並在教師結束作答且指定正確答案時，自動累加答對學生的個人積點。

## Impact

- **型別定義** (`src/types/whiteboard.ts`)：
  - 在 `QuickQaQuestion` 介面中新增欄位 `autoAddPointsForCorrect: boolean`。
- **教師端視圖** (`src/views/WebTeacherView.vue`)：
  - 佈題區域 UI 新增勾選框（使用 Vuetify 的 `v-checkbox` 或類似元件），綁定對應的響應式變數。
  - `publishQuickQaQuestion` 函數需將勾選狀態寫入 `QuickQaQuestion` 狀態中。
  - 在結束作答並設定正確答案的處理邏輯（如 `closeQuickQaWithCorrectOption`）中，判斷若 `autoAddPointsForCorrect` 為 `true`，則遍歷所有學生的答案，找出答對的學生，並對其點數進行 +1 處理。
- **學生端視圖** (`src/views/StudentView.vue`)：
  - 需配合 `QuickQaQuestion` 的型別更新，確保不會發生型別錯誤。
