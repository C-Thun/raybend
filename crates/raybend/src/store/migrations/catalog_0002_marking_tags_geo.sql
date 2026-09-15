-- catalog.db（每库一个）schema v2：标记、标签关联、地理、EXIF 时区
--
-- 出处：BROWSE.md（浏览模式规格）§3 标记体系 / §7 标签体系 / §9 右栏；plans/M1-3.md §3.A。
-- v1 只有 rating / flag；浏览要用的色标、喜欢、锁、作者、描述、地理在本条补齐。

-- ── 1. 标记类字段（全部可空；NULL = 用户没设过）──
ALTER TABLE assets ADD COLUMN color_label TEXT;    -- red / yellow / green / blue / purple
ALTER TABLE assets ADD COLUMN like_state  TEXT;    -- like / dislike
ALTER TABLE assets ADD COLUMN lock_level  INTEGER NOT NULL DEFAULT 0;  -- 0 无 / 1 不可删 / 2 不可编辑

-- ⚠️ **旗标（flag）不在其中**（用户 2026-09-15 明确）：BROWSE.md §3.2 规定旗标只活在内存里
--    —— 跨库跨目录、关软件即清；理由是「打旗标本来就是为了临时挑一下，
--    若持久化就得再全部反打一遍」。v1 已有的 `flag` 列语义收窄为「入库判定」
--    这类需要持久化的判定，**不要**拿它当浏览时的旗标用。

-- ── 2. 可编辑的文字与地理 ──
ALTER TABLE assets ADD COLUMN author         TEXT;
ALTER TABLE assets ADD COLUMN description    TEXT;
ALTER TABLE assets ADD COLUMN gps_lat        REAL;  -- 十进制度；北纬/东经为正
ALTER TABLE assets ADD COLUMN gps_lon        REAL;
ALTER TABLE assets ADD COLUMN country        TEXT;
ALTER TABLE assets ADD COLUMN province_state TEXT;
ALTER TABLE assets ADD COLUMN city           TEXT;
ALTER TABLE assets ADD COLUMN sublocation    TEXT;

-- ── 3. EXIF 时区 ──
-- EXIF 的 DateTimeOriginal **不带时区**。口径（plans/M1-3.md §10，据 RapidRAW 调研定）：
--   * 有 OffsetTime* 标签 → 按它换算成 UTC 毫秒，并在本列记下换算用的偏移（分钟）；
--   * 没有 → 把墙上时间**原样当 UTC 存**，本列置 NULL。
--     显示端遇 NULL 就按「无偏移」渲染 —— 这样用户看到的数字与相机/其它软件一致，
--     而不是被我们本机的时区悄悄改掉。
ALTER TABLE assets ADD COLUMN taken_at_offset_min INTEGER;

-- 筛选（BROWSE.md §3.1 那一排筛选开关）要走索引
CREATE INDEX idx_assets_color_label ON assets(color_label) WHERE color_label IS NOT NULL;
CREATE INDEX idx_assets_like_state  ON assets(like_state)  WHERE like_state  IS NOT NULL;
CREATE INDEX idx_assets_lock_level  ON assets(lock_level)  WHERE lock_level  > 0;

-- ── 4. 资产 ↔ 标签 ──
-- 标签**词典**在 app.db（全局公用，见 migrations/app_0002_tags.sql），这里只存关联。
-- `tag_id` **故意不加外键**：这是跨库引用，而 catalog.db 必须能**单独搬走**
-- （REPOSITORY.md §2）。库打开时由上层做「词典对齐」：把本库用到的 tag_id
-- 在 app.db 里补登记；真成了孤儿就按「待命名」显示，不是错误。
CREATE TABLE asset_tags (
    asset_id  INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_id    INTEGER NOT NULL,
    tagged_at INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_id)
);

-- 「这个标签下有哪些照片」——按标签浏览/统计走这条
CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id, asset_id);

-- ── 5. 全文索引扩展：加入 description ──
-- FTS5 不支持 ADD COLUMN，只能重建（新建 → 搬数据 → 删旧 → 改名）。
-- **标签不进 FTS**：标签名住在 app.db，在这里冗余一份名字会漂移；
-- 搜标签时在查询侧与 app.db 的 `tags` JOIN（FUTURE.md H8）。
-- ⚠️ trigram 的 **3 字符下限**依旧成立：短词查询要回退 `LIKE` 兜底
--    （见 v1 注释与 store::migration::tests::fts5_trigram_finds_chinese_substrings）。
CREATE VIRTUAL TABLE assets_fts_v2 USING fts5(
    file_name,          -- 主文件名（含扩展名）
    camera,             -- 品牌 + 型号
    lens,
    description,        -- 用户写的描述（v2 新增）
    tokenize = 'trigram'
);

INSERT INTO assets_fts_v2(rowid, file_name, camera, lens)
    SELECT rowid, file_name, camera, lens FROM assets_fts;

DROP TABLE assets_fts;
ALTER TABLE assets_fts_v2 RENAME TO assets_fts;
