## MODIFIED Requirements

### Requirement: 教師端必須提供學生可連線的入口資訊

系統 MUST 在主程式控制台啟動且服務就緒後，顯示可供學生連線的 URL、對應 IP 位址、可掃描的 QR Code，並同時顯示目前班級資訊。

#### Scenario: 服務啟動後顯示入口資訊與目前班級

- **WHEN** 主程式控制台啟動內建 Web Server 並進入可接受連線狀態
- **THEN** 主程式控制台 MUST 顯示至少一組可連線 URL 與其 IP 位址
- **AND** 系統 MUST 產生與該 URL 一致的 QR Code 供學生掃描
- **AND** 主程式控制台 MUST 顯示目前班級名稱

#### Scenario: 入口資訊或目前班級變更時更新顯示

- **WHEN** 系統偵測可用網路介面變更、服務綁定位址改變或目前班級被切換
- **THEN** 主程式控制台 MUST 更新 URL、IP、QR Code 與目前班級顯示

## ADDED Requirements

### Requirement: 教師端頁面上方必須顯示目前班級

系統 MUST 在教師端頁面頂部顯示目前班級名稱，讓教師可辨識當前課堂名單上下文。

#### Scenario: 教師端載入後顯示目前班級

- **WHEN** 教師端頁面完成載入並取得課堂狀態
- **THEN** 頁面上方 MUST 顯示目前班級名稱

#### Scenario: 主控台切換班級後同步更新教師端

- **WHEN** 主控台切換目前班級
- **THEN** 教師端頁面上方顯示的班級名稱 MUST 同步更新
- **AND** 更新期間 MUST 保持教師端主要功能可用

### Requirement: 教師端 Web 畫面顯示學生名稱時必須使用座號暱稱

系統 MUST 在教師端 Web 畫面中，將學生名稱顯示統一為「座號暱稱（無分隔）」。

#### Scenario: 教師端學生清單顯示格式統一

- **WHEN** 教師端渲染任一學生名稱欄位
- **THEN** 系統 MUST 顯示「座號暱稱（無分隔）」，例如 `01王小明`
- **AND** 系統 MUST 不得僅顯示暱稱
