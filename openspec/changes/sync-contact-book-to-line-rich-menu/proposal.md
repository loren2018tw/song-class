## Why

目前聯絡簿僅在教師端網頁內可見，老師需要一個可手動發布到各班 LINE 官方帳號的流程，讓家長可從既有圖文選單快速查看最近一次同步的聯絡簿內容。此需求同時要求應用程式不依賴常駐對外服務，避免部署與維運成本。

## What Changes

- 新增每班 LINE 官方帳號設定欄位，讓老師可在班級設定中維護該班 Messaging API 所需資訊。
- 在聯絡簿管理畫面新增「同步到官方帳號」按鈕，按下時以當下顯示的聯絡簿內容作為同步來源。
- 同步流程直接呼叫 LINE Messaging API 管理 rich menu，不使用 webhook 或外部回呼。
- 若未有可用 rich menu，系統自動建立並取得 richMenuId；若已有 richMenuId 則直接更新。
- rich menu 的「聯絡簿」按鈕採固定訊息動作，內容每次同步時覆寫為最新同步文字。
- 增加訊息長度超限時的降級策略，避免同步失敗。

## Capabilities

### New Capabilities

- `line-oa-classroom-settings`: 定義班級層級的 LINE 官方帳號連線設定、驗證規則與儲存方式。
- `contact-book-line-richmenu-sync`: 定義聯絡簿同步到 LINE rich menu 的觸發流程、richMenuId 生命週期與訊息覆寫規則。

### Modified Capabilities

- `contact-book-task-management`: 聯絡簿管理介面由單一新增任務按鈕調整為含同步按鈕，並定義同步來源為當下畫面內容。

## Impact

- 前端：教師主控台班級編輯畫面、聯絡簿管理畫面需新增設定欄位與同步按鈕。
- 後端：班級資料模型與 API 需新增 LINE 設定欄位與同步端點。
- 資料庫：`classrooms` 需新增 LINE 相關欄位並提供 migration。
- 外部依賴：新增對 LINE Messaging API rich menu 相關端點呼叫與錯誤處理。
- 安全：需避免在 UI 與 log 洩漏敏感 token，並定義遮罩與儲存原則。
