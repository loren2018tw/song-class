## Why

目前專案尚未有可供課堂即時連線的教師端與學生端互通基礎，無法驗證數位載具學習情境的最小可行流程。先建立可運作的基礎架構，可讓後續功能在明確的連線與會話骨幹上迭代開發。

## What Changes

- Tauri 僅保留為後端主控小控制台：顯示服務狀態、URL、IP、QR Code 與學生連線摘要。
- 小控制台新增「開啟教師端」按鈕，透過作業系統預設瀏覽器開啟教師端網頁。
- 教師端與學生端畫面統一為純 Vue 網頁（瀏覽器執行），不在 Tauri WebView 內承載 WebRTC。
- Rust 內建 Web Server 與 WebSocket 服務持續作為 signaling 與會話狀態中心。
- 教師端與學生端透過 WebRTC 進行媒體/資料通道，WebSocket 僅做 signaling 與控制訊息。
- 限定本次為基礎架構與最小可驗證流程，不包含教學互動進階功能。

## Capabilities

### New Capabilities

- `teacher-access-bootstrap`: Tauri 主控台提供連線入口資訊，並可一鍵開啟瀏覽器教師端。
- `student-join-and-identity`: 學生端以瀏覽器開啟 Vue 頁面並提交暱稱完成加入。
- `session-realtime-channel`: 以 WebSocket signaling + 瀏覽器 WebRTC 建立教師/學生即時通道，支援教師端掌握已連線學生。

### Modified Capabilities

- 無

## Impact

- Tauri 前端：由「教師互動端」調整為「後端主控台」，需保留開啟教師端按鈕與服務監看資訊。
- Web 前端（Vue 3 + Vuetify）：教師端與學生端均以瀏覽器運作，並共用連線流程元件。
- Rust 後端：持續提供 HTTP 路由、WebSocket signaling 與會話狀態管理。
- 風險與限制：WebRTC 依賴瀏覽器能力與網路環境，初期僅限同網段/基礎情境驗證，不含 TURN/大規模最佳化。
