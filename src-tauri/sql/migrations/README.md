# 資料庫 Migration 規範

- 已發布的基準結構使用 `001_release_baseline.sql`。
- 資料庫版本由 SQLite `PRAGMA user_version` 管理。
- 程式會在建置時自動掃描 `sql/migrations/` 下符合 `NNN_*.sql` 的檔案，依版本號排序後自動套用。
- `next_release.sql` 只作為開發草稿，不會被自動套用。

## 開發中（尚未發布）

- 所有資料表調整先寫在 `next_release.sql`。
- 開發期間可持續覆蓋同一份草稿，不需要每次改欄位就加新版本號。
- 確保草稿內容可由「上一版 release 資料庫」升級到「下一版目標結構」。

## 發版前

1. 將 `next_release.sql` 複製為正式版本檔，例如 `002_xxx.sql`。
2. 清空或重建 `next_release.sql`，供下一輪開發使用。
3. 執行測試，驗證舊版資料庫可升級到最新版。
4. 發版。

## 命名建議

- 正式 migration：`NNN_描述.sql`（例如 `002_add_task_tags.sql`）
- 開發草稿：`next_release.sql`
