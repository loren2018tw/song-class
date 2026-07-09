-- Add LINE Official Account settings columns to classrooms.
-- line_enabled: whether LINE sync is enabled for this class.
-- line_channel_access_token: LINE Messaging API channel access token (sensitive).
-- line_channel_secret: LINE Messaging API channel secret (sensitive).
-- line_rich_menu_id: cached richMenuId for this class (auto-managed).

ALTER TABLE classrooms ADD COLUMN line_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE classrooms ADD COLUMN line_channel_access_token TEXT NOT NULL DEFAULT '';
ALTER TABLE classrooms ADD COLUMN line_channel_secret TEXT NOT NULL DEFAULT '';
ALTER TABLE classrooms ADD COLUMN line_rich_menu_id TEXT NOT NULL DEFAULT '';
