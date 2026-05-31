## Why

目前課堂加入流程依賴學生自行輸入暱稱，無法對應班級名單與座號，教師也缺乏在主控台管理班級成員的能力。現在需要導入以班級名單為核心的加入流程與資料持久化，才能在實際教學中穩定管理 30 位學生與其連線狀態。

## What Changes

- 在主控台新增「班級管理」區塊，支援選擇目前班級、顯示目前班級名稱，並提供「編輯班級成員」入口（可修改座號與暱稱）。
- 導入 SQLite 班級資料模型與 DDL 檔案，啟動時若資料庫沒有任何班級則自動建立一個預設班級。
- 系統協助建立的首個班級自動建立 30 位學生名單，座號固定為文字 `01` 至 `30`，暱稱欄位允許空白且預設為空字串；該班級與一般班級行為一致且班級名稱可編輯。
- 將學生端加入流程由「輸入暱稱」改為「點選班級學生按鈕」；已連入的學生按鈕需顯示為不可操作。
- 教師端頁面上方顯示目前班級，讓教師能即時辨識目前操作班級。
- 網頁介面中所有顯示學生暱稱的位置，統一改為顯示「座號暱稱」。
- 切換目前班級時，系統強制登出該班所有已連入學生，避免跨班狀態殘留。

## Capabilities

### New Capabilities

- `class-roster-management`: 定義班級與學生名單的資料結構、SQLite DDL、預設班級初始化與主控台班級管理操作。
- `roster-based-student-join`: 定義學生端依班級名單按鈕加入課堂與已連線學生按鈕鎖定行為。

### Modified Capabilities

- `student-join-and-identity`: 將學生加入前置流程由暱稱輸入改為班級學生名單選擇，並更新身分建立規則。
- `teacher-access-bootstrap`: 擴充教師/主控台啟動後資訊區，新增目前班級顯示與班級管理入口要求。

## Impact

- 前端：`src/views/StudentView.vue`、`src/views/TeacherView.vue`、`src/views/WebTeacherView.vue`、主控台相關元件與班級管理 UI（含名稱顯示格式統一）。
- 後端：`src-tauri/src/lib.rs` 與 Web Server/連線狀態邏輯，需新增班級資料查詢與連線占用判斷。
- 資料：新增 SQLite 資料庫檔案與版本 0 DDL（班級、學生、連線對應欄位）。
- 規格：新增 capability specs，並更新既有 `student-join-and-identity`、`teacher-access-bootstrap` 要求。
