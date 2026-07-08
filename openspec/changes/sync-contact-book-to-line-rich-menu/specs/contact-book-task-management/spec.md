## ADDED Requirements

### Requirement: 聯絡簿管理頁必須提供同步到官方帳號按鈕

系統 MUST 在聯絡簿管理工具列中提供「同步到官方帳號」按鈕，且按鈕可與既有「新增任務」並存。

#### Scenario: 聯絡簿標籤顯示同步按鈕

- **WHEN** 教師進入聯絡簿管理模組
- **THEN** 工具列 MUST 同時顯示「新增任務」與「同步到官方帳號」按鈕

### Requirement: 同步操作結果必須即時回饋

系統 MUST 在同步成功或失敗時提供明確回饋訊息，讓教師可立即確認結果。

#### Scenario: 同步成功顯示成功訊息

- **WHEN** 教師按下同步且 LINE API 更新成功
- **THEN** 系統 MUST 顯示同步成功通知

#### Scenario: 同步失敗顯示可理解原因

- **WHEN** 教師按下同步但 LINE API 回傳錯誤
- **THEN** 系統 MUST 顯示失敗原因與可重試提示
