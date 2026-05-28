## Context

此變更目標是在現有 Tauri 2 + Rust + Vue 3 專案中建立可驗證的教師端/學生端連線骨幹，並調整角色分工：Tauri 僅作為後端主控小控制台；教師端與學生端互動頁面統一以瀏覽器載入 Vue 網頁。系統需同時具備 HTTP 靜態頁提供、WebSocket 訊號交換與 WebRTC 連線建立能力，但 WebRTC 僅由瀏覽器端承擔，以避開部分 Tauri WebView 平台能力差異。

主要限制如下：

- 前端 UI 使用 Vue 3 + Vuetify，並將教師端與學生端優先設計為瀏覽器端頁面。
- 後端以 Rust 管理服務生命週期，需可由 Tauri 應用啟動/停止。
- 僅支援教室/同網段的初始驗證場景，不涵蓋 TURN 佈署與複雜 NAT 穿透策略。

## Goals / Non-Goals

**Goals:**

- 建立可啟動的內建 Web Server 與 WebSocket 服務，提供學生端連線入口。
- Tauri 主控台可顯示可用連線資訊（IP、URL、QR Code）與即時學生暱稱清單。
- Tauri 主控台可一鍵開啟作業系統預設瀏覽器的教師端頁面。
- 學生端加入流程包含暱稱輸入、送出、加入結果回饋。
- 提供瀏覽器教師端與瀏覽器學生端的 WebRTC 建立流程，完成端到端最小連線驗證。
- 建立清楚的模組邊界，讓後續教學功能可在此基礎上擴充。

**Non-Goals:**

- 不實作課堂互動功能（出題、搶答、同步教材、檔案傳輸等）。
- 不提供帳號系統、持久化身份驗證或資料庫儲存。
- 不處理大規模連線調校、TURN 高可用佈署與網路品質優化。
- 不保證跨網際網路環境的連線成功率。

## Decisions

1. 以 Rust 單一服務容器承載 HTTP 與 WebSocket

- 決策：在 Tauri 後端維護單一 server runtime，同時註冊靜態頁路由與 WebSocket 訊號端點。
- 理由：降低跨程序協調複雜度，便於與桌面 UI 共享狀態（IP、連線清單、服務狀態）。
- 替代方案：
  - 分離成獨立 Node/Rust Web 服務程序：可解耦但增加部署與進程管理成本。
  - 僅使用 HTTP 輪詢：實作較簡單但無法支撐低延遲訊號交換。

2. WebRTC 訊號交換以 WebSocket JSON 事件模型實作

- 決策：定義最小事件集合（join、offer、answer、ice-candidate、peer-connected、peer-disconnected）透過 WebSocket 傳遞。
- 理由：WebSocket 適合作為 signaling；實際媒體與資料通道由瀏覽器 WebRTC 承擔，降低平台相容風險。
- 替代方案：
  - 使用 SSE + HTTP POST：可行但雙向互動與狀態同步較繁瑣。
  - 直接引入第三方 signaling 服務：可加速，但與本地教室離線可用目標不一致。

3. 前端採「Tauri 主控台 + 瀏覽器端別頁」結構

- 決策：Tauri 視窗僅保留主控資訊與「開啟教師端」操作；教師端與學生端皆為瀏覽器 Vue 頁面，並共享通訊與表單元件。
- 理由：避免 Tauri WebView 在特定平台缺少 RTCPeerConnection 時造成教師端不可用，並維持開發一致性。
- 替代方案：
  - 完全獨立兩套前端：短期較直覺但長期容易行為分歧。

4. 學生暱稱清單以記憶體會話狀態維護

- 決策：在後端以 in-memory 結構管理目前連線學生，透過事件推播更新教師端。
- 理由：符合本次基礎架構範圍，避免過早引入資料庫。
- 替代方案：
  - 持久化到 SQLite/外部 DB：對本階段需求過重。

## Risks / Trade-offs

- [Risk] 教師端本機 IP 判定錯誤（多網卡/虛擬網卡）導致學生無法連線。→ Mitigation：提供可選網卡與手動覆寫 URL 的設計預留。
- [Risk] WebRTC 在部分網路環境建立失敗。→ Mitigation：明確限定本階段支援情境，保留未來 TURN 設定擴充點。
- [Risk] 使用者誤在 Tauri 視窗期待直接承載教師 WebRTC。→ Mitigation：主控台明確提示並提供一鍵開啟瀏覽器教師端。
- [Risk] 學生快速重複連線造成名單狀態不一致。→ Mitigation：以連線 ID + 暱稱做去重，斷線時觸發清理事件。
- [Trade-off] 不做持久化可降低複雜度，但服務重啟會遺失會話狀態。→ Mitigation：本階段接受此限制。

## Migration Plan

1. 新增後端 server 模組與 WebSocket signaling 模組，接入 Tauri 啟動流程。
2. Tauri 主控台保留服務資訊與名單摘要，新增「開啟教師端」按鈕。
3. 新增瀏覽器教師端頁與學生端頁（純 Vue）。
4. 串接 WebRTC signaling 事件骨架，完成瀏覽器對瀏覽器最小端到端連線。
5. 以本機多裝置（或多瀏覽器）執行冒煙驗證：學生加入、教師端名單更新、連線建立。

回滾策略：

- 以 feature flag 或路由切換保留既有空白頁/舊入口。
- 若整合失敗，先回退 Tauri 啟動時的 server 初始化，避免影響桌面殼層啟動。

## Open Questions

- 教師端是否需要同時顯示多個可用網址（IPv4/IPv6/hostname）？
- 初版是否允許重名暱稱，或需在加入時即阻擋？
- 教師端瀏覽器頁是否需支援自訂房間碼，以便同伺服器多班級並行？
