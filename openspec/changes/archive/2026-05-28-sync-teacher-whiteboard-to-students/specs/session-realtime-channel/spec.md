## MODIFIED Requirements

### Requirement: 系統必須提供教師端與學生端的即時訊號交換通道

系統 MUST 提供 WebSocket 訊號交換通道與 WebRTC DataChannel 資料傳輸能力，以支援連線協商、模式同步與白板資料事件傳遞。

#### Scenario: 連線建立期間交換 signaling 訊息

- **WHEN** 任一端開始 WebRTC 連線協商
- **THEN** 系統 MUST 透過 WebSocket 傳遞對應的 signaling 訊息到目標端

#### Scenario: 已連線期間傳遞白板即時事件

- **WHEN** 教師端在 DataChannel 上發送白板模式或增量繪製事件
- **THEN** 系統 MUST 將事件傳遞到目標學生端
- **AND** 系統 MUST 保留事件順序以支援學生端重播

#### Scenario: 增量事件採批次傳送以支援約 30 位學生

- **WHEN** 教師端持續產生高頻白板增量事件且同時有約 30 位學生在線
- **THEN** 系統 MUST 將增量事件以批次封包方式傳送而非逐點即時單筆送出
- **AND** 系統 MUST 在可重播前提下控制每批次延遲於可接受的即時教學範圍
- **AND** 系統 MUST 讓學生端依批次內序號還原正確事件順序

#### Scenario: 訊號傳遞失敗

- **WHEN** signaling 訊息無法成功傳遞或目標端已離線
- **THEN** 系統 MUST 回報失敗事件給發送端

## ADDED Requirements

### Requirement: 系統必須在新加入學生連線後提供白板初始化快照

系統 MUST 在學生連線可用時提供教師端當前白板快照，使新加入學生能在短時間內重現既有畫面，再接續增量更新。

#### Scenario: 新學生連線後立即取得教師目前功能模式

- **WHEN** 新加入學生完成會話加入與 DataChannel 建立
- **THEN** 教師端 MUST 立即傳送目前功能模式事件（例如首頁或白板）給該學生
- **AND** 學生端 MUST 先套用該模式事件再顯示對應界面

#### Scenario: 新學生完成連線後接收快照

- **WHEN** 新加入學生完成會話加入與 DataChannel 建立
- **THEN** 教師端 MUST 傳送一次當前白板快照給該學生
- **AND** 學生端 MUST 先套用快照再處理後續增量事件

#### Scenario: 新學生快照必須由教師端主動推送

- **WHEN** 新加入學生連線進入可接收白板資料狀態
- **THEN** 教師端 MUST 主動推送最新白板快照給該學生
- **AND** 系統 MUST 不依賴學生端額外請求作為快照傳送前提

#### Scenario: 快照後切換為持續增量同步

- **WHEN** 學生端成功套用初始化快照
- **THEN** 系統 MUST 持續傳送後續白板增量事件
- **AND** 系統 MUST 避免重複傳送整份白板快照作為每次更新
