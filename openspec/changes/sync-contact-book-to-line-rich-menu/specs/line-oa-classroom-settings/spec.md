## ADDED Requirements

### Requirement: 班級設定必須支援 LINE 官方帳號連線資訊

系統 MUST 在每個班級保存獨立的 LINE 官方帳號設定，至少包含啟用狀態、channel access token、channel secret 與 richMenuId。

#### Scenario: 教師儲存班級 LINE 設定

- **WHEN** 教師在班級編輯畫面輸入該班 LINE 設定並按下儲存
- **THEN** 系統 MUST 將設定保存到該班級資料
- **AND** 系統 MUST 不影響其他班級的 LINE 設定

### Requirement: 敏感欄位必須遮罩顯示並可覆寫

系統 MUST 對 token 與 secret 類敏感欄位採遮罩顯示，且允許教師在需要時覆寫更新。

#### Scenario: 讀取既有設定時顯示遮罩

- **WHEN** 教師開啟含既有 LINE 設定的班級編輯畫面
- **THEN** 系統 MUST 以遮罩形式顯示敏感值
- **AND** 系統 MUST 提供覆寫輸入機制

### Requirement: richMenuId 必須可自動建立與回填

當班級未設定 richMenuId 時，系統 MUST 在首次同步時自動建立 rich menu 並保存回傳的 richMenuId。

#### Scenario: 首次同步自動建立 rich menu

- **WHEN** 教師在某班首次按下同步且該班 richMenuId 為空
- **THEN** 系統 MUST 呼叫 LINE API 建立 rich menu
- **AND** 系統 MUST 將回傳 richMenuId 寫回班級設定

#### Scenario: 既有 richMenuId 失效時自動重建

- **WHEN** 同步過程中 LINE 回覆 richMenuId 無效或不存在
- **THEN** 系統 MUST 建立新的 rich menu 並更新班級保存值
