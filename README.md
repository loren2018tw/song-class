# song-class 爽課啦～

「爽課啦～」發音近似台語的「上課囉」，也代表這個專案的初衷：
讓老師使用數位載具上課時，可以更直覺、更穩定、更放心，不再被工具焦慮綁架。

song-class 是一個以「課堂即時互動」為核心的跨平台教學系統，支援教師端與學生端同步白板、課堂互動與連線管理。

## 專案願景

- 降低老師數位教學門檻：打開就能教，不必事先準備教材。
- 強化課堂即時互動：讓教師與學生在同一節奏下學習。
- 提供可擴充的教學基礎建設：兼顧桌面應用與網頁端場景。

## 主要功能（目前規格）

- 教師端白板與學生端即時白板
- 即時廣播教師畫面到學生端
- 聯絡簿及作業繳交管理
- LINE 官方帳號 rich menu 同步（每班可獨立設定，手動一鍵同步）
- Teacher/Student 雙端即時通道（Realtime Channel）
- 即時問答互動
- 教師端與 Web 端共用 UI 元件，降低重複開發成本

## 技術架構

### 桌面應用（Tauri）

- Tauri 2
- Rust（後端邏輯、系統整合、內建 Web Server）
- Vue 3 + TypeScript（前端 UI）

### 前端

- Vue 3（Composition API + `<script setup>`）
- Vuetify（UI 元件框架）
- Vite（開發與建置）
- pnpm（套件管理）

### 後端

- Rust + Cargo
- 內建 HTTP 服務，提供遠端登入與互動頁面

## 開發原則

- 以共用元件為優先：Tauri 視窗 UI 與 Web UI 能共用就不重寫。
- 規格先行：功能以 OpenSpec 規格驅動，減少實作偏差。
- 穩定優先：即時互動場景下，資料一致性與連線體驗為第一優先。

## 專案結構

- `src/`：Vue 前端程式碼（元件、視圖、composables、型別）
- `src-tauri/`：Tauri/Rust 程式碼與設定
- `openspec/`：需求規格、變更提案、設計與任務文件

## 資料庫儲存位置

### Linux

~/.local/share/boats.loren.song-class/song-class.sqlite3

### windows

%APPDATA%\Local\boats.loren.song-class/song-class.sqlite3

## 快速開始

### 1. 安裝依賴

```bash
pnpm install
```

### 2. 啟動前端開發模式

```bash
pnpm dev
```

### 3. 啟動 Tauri 開發模式

```bash
pnpm tauri dev
```

### 4. 打包桌面應用

```bash
pnpm tauri build
```

## LINE 官方帳號同步設定

### 前置準備

1. 在 [LINE Developers Console](https://developers.line.biz/console/) 建立或選取一個 Messaging API channel。
2. 取得 **Channel Access Token**（長期權杖）與 **Channel Secret**。
3. 將該 channel 設定為可使用 rich menu。

### 教師操作步驟

1. 在主控台「班級編輯畫面」點選班級的「編輯」按鈕。
2. 在彈出對話框的 **LINE 官方帳號設定** 區塊：
   - 開啟「啟用 LINE 同步」開關。
   - 填入 Channel Access Token 與 Channel Secret。
   - 點選「儲存」。
3. 進入該班的「聯絡簿管理」模組。
4. 確認當日聯絡簿內容已編輯完成。
5. 點選工具列上的「同步到官方帳號」按鈕。
6. 等待同步完成，系統會顯示成功或失敗訊息。

### 同步行為說明

- 每次同步會以**當下畫面顯示的聯絡簿任務**作為同步來源。
- 系統會自動檢查該班是否已有 rich menu；若無則自動建立。
- rich menu 的「聯絡簿」按鈕使用固定訊息動作，內容為最新的同步文字。
- 若同步文字超過 300 字，系統會自動摘要並提示教師。
- 若 rich menu 失效，系統會自動重建並回填新的 richMenuId。
- 同步不依賴 webhook 或外部常駐服務。

### 注意事項

- Token 與 Secret 為敏感資訊，讀取 API 時會自動遮罩顯示。
- 每班可獨立設定不同的 LINE 官方帳號。
- 若需關閉 LINE 同步功能，可在班級設定中關閉「啟用 LINE 同步」開關。

## 建議開發環境

- VS Code
- Vue - Official（Volar）
- Tauri 擴充套件
- rust-analyzer

## 規格文件（OpenSpec）

本專案採用 OpenSpec 管理功能需求與變更流程，主要內容位於 `openspec/`：

- `openspec/specs/`：目前生效的功能規格
- `openspec/changes/`：變更提案、設計與任務
- `openspec/changes/archive/`：已完成並封存的變更

建議開發流程：

1. 先讀規格，確認行為與邊界。
2. 再做實作，過程中對齊變更任務。
3. 完成功能後回補測試與文件。

## 品牌語氣

song-class = 爽課啦～

不是「輕鬆隨便」，而是「工具到位、教學更自在」。
我們希望老師把焦點放回教學本身，而不是被設備與軟體牽著走。
