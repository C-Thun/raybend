-- app.db（全局库）schema v2：标签词典
--
-- 为什么在 app.db：BROWSE.md §7.1 —— 标签是**跨库公用**的（在 A 库建的词，
-- 在 B 库打标签时也该能选到）。各库的 catalog.db 里只存 `asset_tags` 关联
-- （见 migrations/catalog_0002_marking_tags_geo.sql）。

CREATE TABLE tags (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,           -- 展示名：保留用户输入的大小写与写法
    name_folded TEXT    NOT NULL,           -- NFC + 小写折叠；**唯一性判据**
    use_count   INTEGER NOT NULL DEFAULT 0, -- 使用次数；由上层按各库的关联数同步
    created_at  INTEGER NOT NULL,
    UNIQUE (name_folded)
);

-- 折叠的语义：与路径折叠同一套「相等」判据（`store::path_semantics`）——
-- NFC 规范化 + 小写。所以 `Trip` 与 `trip` 是同一个标签，不会并存两条。

-- 常用标签提示：按使用次数倒序
CREATE INDEX idx_tags_use ON tags(use_count DESC, name);
