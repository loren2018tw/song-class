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
