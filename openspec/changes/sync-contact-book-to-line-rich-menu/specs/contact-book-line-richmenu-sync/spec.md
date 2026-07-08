## ADDED Requirements

### Requirement: 聯絡簿同步必須採教師手動觸發

系統 MUST 僅在教師於聯絡簿管理畫面按下同步按鈕時執行同步，且同步內容 MUST 來自按鈕觸發當下畫面顯示的聯絡簿項目。

#### Scenario: 以當下畫面內容同步

- **WHEN** 教師在聯絡簿管理畫面按下「同步到官方帳號」
- **THEN** 系統 MUST 使用當下顯示的聯絡簿內容生成同步文字
- **AND** 系統 MUST 不要求教師額外指定日期參數

### Requirement: 系統必須直接更新 rich menu 聯絡簿按鈕內容

系統 MUST 直接使用 LINE Messaging API 更新 rich menu，確保存在「聯絡簿」按鈕，並以固定訊息動作承載最新同步內容。

#### Scenario: 既有聯絡簿按鈕時覆寫內容

- **WHEN** rich menu 已存在「聯絡簿」按鈕
- **THEN** 系統 MUST 將該按鈕動作文字覆寫為本次同步內容

#### Scenario: 缺少聯絡簿按鈕時補建

- **WHEN** rich menu 不含「聯絡簿」按鈕
- **THEN** 系統 MUST 新增可觸發聯絡簿訊息的按鈕區塊

### Requirement: 同步流程不得依賴 webhook 或常駐外部服務

系統 MUST 在同步時一次性完成對 LINE API 的更新，MUST NOT 依賴 LINE 回呼到本應用程式。

#### Scenario: 無 webhook 設定仍可運作

- **WHEN** 班級未設定任何 webhook 對外網址
- **THEN** 教師仍 MUST 可完成聯絡簿同步
- **AND** 同步成功後家長點選聯絡簿按鈕 MUST 看到最後一次同步文字

### Requirement: 聯絡簿訊息超限時必須有降級策略

當同步文字超過 LINE 動作欄位限制時，系統 MUST 使用可預期的降級策略避免同步失敗。

#### Scenario: 內容超出限制時自動摘要

- **WHEN** 系統判定聯絡簿同步文字超過限制
- **THEN** 系統 MUST 改為摘要內容並提示教師已套用摘要策略
- **AND** 同步流程 MUST 仍可成功完成
