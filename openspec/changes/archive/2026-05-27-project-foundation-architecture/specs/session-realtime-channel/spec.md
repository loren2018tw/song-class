## ADDED Requirements

### Requirement: 系統必須提供教師端與學生端的即時訊號交換通道

系統 MUST 提供 WebSocket 訊號交換通道，以支援瀏覽器教師端與瀏覽器學生端建立 WebRTC 連線所需的 offer、answer 與 ICE candidate 事件傳遞。

#### Scenario: 連線建立期間交換 signaling 訊息

- **WHEN** 任一端開始 WebRTC 連線協商
- **THEN** 系統 MUST 透過 WebSocket 傳遞對應的 signaling 訊息到目標端

#### Scenario: 訊號傳遞失敗

- **WHEN** signaling 訊息無法成功傳遞或目標端已離線
- **THEN** 系統 MUST 回報失敗事件給發送端

### Requirement: 教師端必須即時看到已連入學生暱稱清單

系統 MUST 在學生成功加入或離開時，即時更新教師端顯示的學生暱稱清單。

#### Scenario: 學生加入後更新名單

- **WHEN** 學生完成暱稱確認並加入成功
- **THEN** 教師端畫面 MUST 將該學生暱稱加入清單

#### Scenario: 學生離線後移除名單

- **WHEN** 已加入學生中斷連線或主動離開
- **THEN** 教師端畫面 MUST 從清單移除該學生暱稱

### Requirement: 系統必須維護會話層級的連線狀態一致性

系統 MUST 以唯一連線識別管理學生會話，避免短時間重連造成教師端名單重複或殘留。

#### Scenario: 同一學生短時間重連

- **WHEN** 同一連線識別在短時間內重複建立連線
- **THEN** 系統 MUST 以最新有效連線覆蓋舊狀態
- **AND** 教師端名單 MUST 不產生重複項目

### Requirement: 教師端與學生端 WebRTC 必須由瀏覽器端承擔

系統 MUST 讓教師端與學生端在一般瀏覽器中建立 WebRTC，不以 Tauri WebView 作為必要的 WebRTC 端點。

#### Scenario: Tauri WebView 缺少 RTCPeerConnection

- **WHEN** Tauri 主控台所在 WebView 不支援 RTCPeerConnection
- **THEN** 系統 MUST 仍可透過瀏覽器教師端頁面完成與學生端的 WebRTC 連線
