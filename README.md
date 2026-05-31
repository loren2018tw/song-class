# song-class 爽課啦～

「爽課啦～」發音近似台語的「上課囉」，也代表這個專案的初衷：
讓老師使用數位載具上課時，可以更直覺、更穩定、更放心，不再被工具焦慮綁架。

song-class 是一個以「課堂即時互動」為核心的跨平台教學系統，支援教師端與學生端同步白板、課堂互動與連線管理。

## 專案願景

- 降低老師數位教學門檻：打開就能教，不必先變成 IT 專家。
- 強化課堂即時互動：讓教師與學生在同一節奏下學習。
- 提供可擴充的教學基礎建設：兼顧桌面應用與網頁端場景。

## 主要功能（目前規格）

- 教師端白板與學生端白板即時同步
- 教師可管理學生加入與課堂身份
- 學生白板畫廊瀏覽與內容協作
- Teacher/Student 雙端即時通道（Realtime Channel）
- Quick QA 即時問答互動
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

...

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
