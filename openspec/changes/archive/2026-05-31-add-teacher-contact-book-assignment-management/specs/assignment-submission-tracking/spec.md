## ADDED Requirements

### Requirement: 需控管任務必須提供學生繳交狀態清單

系統 MUST 對每一筆 requires_tracking=true 的任務提供學生繳交狀態，並能區分已繳交與未繳交學生。

#### Scenario: 顯示需控管任務的繳交名單

- **WHEN** 教師開啟作業繳交管理標籤並檢視一筆需控管任務
- **THEN** 系統 MUST 顯示該班級學生的已繳交與未繳交狀態

### Requirement: 非控管任務不得出現在繳交管理清單

系統 MUST 僅在作業繳交管理標籤顯示 requires_tracking=true 的任務。

#### Scenario: 任務取消控管後從繳交管理移除

- **WHEN** 教師將任務的是否需控管改為 false 並儲存成功
- **THEN** 系統 MUST 將該任務從作業繳交管理清單移除

### Requirement: 教師可快速更新任務的全班完成狀態

系統 MUST 提供任務層級的快速完成控制，用於將任務標記為已完成或未完成，且變更 MUST 反映於繳交管理檢視。

#### Scenario: 教師將任務快速標記為已完成

- **WHEN** 教師在作業繳交管理對任務執行快速全部完成
- **THEN** 系統 MUST 將該任務標記為已完成並更新清單顯示

#### Scenario: 已完成任務被改為未完成

- **WHEN** 教師將既有已完成任務改回未完成
- **THEN** 系統 MUST 在後續篩選中將其視為未完成任務

### Requirement: 任務已完成狀態需由繳交資料自動推導同步

系統 MUST 依學生繳交狀態自動推導並同步任務 is_completed，且 MUST 保留任務層已完成欄位避免班級名單變動造成歷史完成狀態混亂。

#### Scenario: 全班皆繳交時自動同步已完成

- **WHEN** 某任務在目前班級中所有需計算學生皆為已繳交
- **THEN** 系統 MUST 自動將該任務 is_completed 同步為 true

#### Scenario: 班級名單異動後仍保留任務完成快照

- **WHEN** 任務已完成後班級發生新增或刪除學生
- **THEN** 系統 MUST 維持任務層已完成狀態，避免僅因名單異動而直接回退

### Requirement: 繳交管理僅作用於目前選擇班級

系統 MUST 只顯示與更新目前選擇班級的任務繳交狀態，MUST NOT 進行跨班彙整或排序。

#### Scenario: 在班級 A 不可見班級 B 的繳交狀態

- **WHEN** 教師目前選擇班級 A 並開啟繳交管理
- **THEN** 系統 MUST 只呈現班級 A 的學生繳交資訊
