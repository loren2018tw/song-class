# 專案技術棧

## 應用框架

- **Tauri 2**：桌面應用程式框架，前端以 Vite + Vue 3 建置，後端為 Rust
- **Rust**：後端邏輯、系統整合、內建 Web Server

## 前端（Tauri 視窗 UI）

- **Vue 3**（Composition API + `<script setup>`）
- **Vuetify**：UI 元件框架，所有介面一律使用 Vuetify 元件撰寫
- **TypeScript**
- **Vite**：建置工具

## 內建 Web Server（遠端登入網頁界面）

- 由 Rust 後端提供 HTTP 服務，供遠端瀏覽器連線使用
- 網頁介面同樣使用 **Vue 3 + Vuetify** 撰寫
- 可與 Tauri UI 共用的介面元件，一律設計為 **Vue 元件**，避免重複實作

## 開發原則

- 共用 UI 邏輯抽離為 Vue 元件，供 Tauri 視窗與 Web 介面共同使用
- 套件管理使用 **pnpm**
- Rust 依賴管理使用 **Cargo**

## 資料庫 Migration 規範

- 已發布的基準結構使用 `001_release_baseline.sql`。
- 資料庫版本由 SQLite `PRAGMA user_version` 管理。
- 程式會在建置時自動掃描 `sql/migrations/` 下符合 `NNN_*.sql` 的檔案，依版本號排序後自動套用。
- `next_release.sql` 只作為開發草稿，不會被自動套用。

### 開發中（尚未發布）

- 所有資料表調整先寫在 `next_release.sql`。
- 開發期間可持續覆蓋同一份草稿，不需要每次改欄位就加新版本號。
- 確保草稿內容可由「上一版 release 資料庫」升級到「下一版目標結構」。

### 發版前

1. 將 `next_release.sql` 複製為正式版本檔，例如 `002_xxx.sql`。
2. 清空或重建 `next_release.sql`，供下一輪開發使用。
3. 執行測試，驗證舊版資料庫可升級到最新版。
4. 發版。

### 命名建議

- 正式 migration：`NNN_描述.sql`（例如 `002_add_task_tags.sql`）
- 開發草稿：`next_release.sql`
