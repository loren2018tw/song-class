## Why

目前教師授課時缺乏可直接廣播電腦主機畫面的能力，導致操作示範與軟體教學效率受限。既有白板模式與快問快答皆為獨立教學模組，若同時執行將造成介面與事件互相干擾，因此需要加入明確的模組互斥控制，並以 WebRTC 實作真正可觀看的教師螢幕串流。

## What Changes

- 教師端左側新增「廣播教師畫面」按鈕，啟動時開啟螢幕擷取並透過 WebRTC 串流給所有學生。
- 教師啟動廣播時，教師主畫面顯示大型提示字串「教室畫面廣播中」，並可停止廣播。
- 學生端在廣播期間自動切換為教師串流接收畫面，顯示教師螢幕影像。
- 建立模組互斥機制，白板模式、快速問答、教師畫面廣播同一時間僅允許一種模組為啟用狀態。
- 定義廣播控制事件、WebRTC 媒體協商流程與加入同步行為，確保新加入學生可進入正確模式並接收串流。

## Capabilities

### New Capabilities

- `teacher-screen-broadcast`: 教師可啟動/停止 WebRTC 螢幕串流廣播，學生端可切換到教師串流接收畫面。

### Modified Capabilities

- `teacher-whiteboard`: 教師視圖需新增廣播控制按鈕與廣播中提示狀態呈現。
- `student-whiteboard-sync`: 學生端需在接收到教師廣播事件時切換到串流接收畫面，並與白板模式互斥。
- `session-realtime-channel`: 即時通道需支援教師廣播控制事件與 WebRTC 媒體協商，並在加入同步時攜帶目前啟用模組。
- `quick-qa-session-management`: 快速問答流程需與教師廣播互斥，避免同時啟動造成教學流程衝突。

## Impact

- 前端 Vue 畫面：教師端與學生端主視圖與側邊控制區。
- 即時通訊模型：session 模組狀態、控制事件、WebRTC 媒體協商流程、加入時初始狀態。
- 可能影響檔案：src/views、src/components、src/composables/usePeerConnection.ts、src/types/session.ts、src/types/whiteboard.ts。
