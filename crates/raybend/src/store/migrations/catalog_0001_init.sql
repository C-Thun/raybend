-- catalog.db（每库一个）schema v1
--
-- 这是**库的真相源**：库身份、资产、元数据、导入模版、序号计数（LIBRARY.md）。
-- 与 app.db 的分工：这里只谈这一个库自己的事，不关心「用户装了哪些库」。
--
-- 约定：
--   * 时间一律 Unix 毫秒（INTEGER），UTC
--   * 库内路径存**相对路径**（`photos/2026-08-15/MYP0001.png`），分隔符统一 '/'
--     —— 这样整个库目录可以整体搬走/改盘符而不失效
--   * 路径同样存两份：原始 + 折叠（见 store::path_semantics）

-- 库元信息（KV）。必填键：
--   library_id     —— 库身份（LIBRARY.md §2.5），决定「同路径不同库 / 同库多路径」能否成立
--   import_template—— 导入模版（默认 `:CYEAR-:CMONTH-:CDAY/MY:FILENAME`）
--   created_at     —— 建库时间
CREATE TABLE library_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 资产：一张**照片**（逻辑实体，不是文件）
CREATE TABLE assets (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    taken_at        INTEGER,                      -- 拍摄时间（可空：有些文件没有）
    taken_at_source TEXT,                         -- exif / video / file_mtime / filename / unknown
    camera_make     TEXT,
    camera_model    TEXT,
    lens            TEXT,
    focal_mm        REAL,
    f_number        REAL,
    exposure_ms     REAL,                         -- 快门时间（毫秒，便于排序与统计）
    iso             INTEGER,
    width           INTEGER,
    height          INTEGER,
    orientation     INTEGER,                      -- EXIF orientation（1..8）
    rating          INTEGER NOT NULL DEFAULT 0,   -- 0..5
    flag            TEXT    NOT NULL DEFAULT 'none',  -- none / pick / reject
    imported_at     INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX idx_assets_taken_at ON assets(taken_at DESC, id DESC);
CREATE INDEX idx_assets_rating ON assets(rating) WHERE rating > 0;
CREATE INDEX idx_assets_flag ON assets(flag) WHERE flag <> 'none';

-- 物理文件：一个资产可以有多个文件（位图 + RAW；将来还有侧车）
CREATE TABLE asset_files (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id           INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    role               TEXT    NOT NULL,          -- bitmap / raw / sidecar
    rel_path           TEXT    NOT NULL,          -- 库内相对路径（原始大小写，展示用）
    rel_path_folded    TEXT    NOT NULL,          -- NFC + 折叠（唯一索引与比较用）
    source_path        TEXT,                      -- 导入来源（溯源；导入后不再使用）
    source_path_folded TEXT,
    ext                TEXT    NOT NULL,          -- 小写扩展名，不含点
    size_bytes         INTEGER,
    mtime_ms           INTEGER,
    volume_serial      INTEGER,                   -- 文件身份（AGENTS.md §7.3）
    file_id            BLOB,                      -- 16 字节；与 volume_serial 一起构成身份
    missing_since      INTEGER,                   -- 非空 = 该文件已不在磁盘上（磁盘被拔/被删）
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    UNIQUE (rel_path_folded)
);

CREATE INDEX idx_asset_files_asset ON asset_files(asset_id, role);
-- 「这个文件我见过吗」——避免重复导入时按身份查（AGENTS.md §7.3）
CREATE INDEX idx_asset_files_identity ON asset_files(volume_serial, file_id)
    WHERE volume_serial IS NOT NULL;

-- 派生索引（缩略图等）不进这张表：见 AGENTS.md §6.5（缓存有独立的键与淘汰策略）

-- 全文检索用的索引（AGENTS.md §7.2）
--   * trigram：**中文唯一可用的分词器**（unicode61 对中文等于没分）
--   * 用普通（非 contentless）表，简单可靠；数据量在照片元数据这个尺度上很小
--   * ⚠️ trigram 按 3 个字符切分 ⇒ **查询词少于 3 个字符搜不到**
--     （如「合影」「R5」）。搜索功能实现时，短词必须回退到 `LIKE '%…%'`。
--     这一点有测试守着：store::migration::tests::fts5_trigram_finds_chinese_substrings
CREATE VIRTUAL TABLE assets_fts USING fts5(
    file_name,          -- 主文件名（含扩展名）
    camera,             -- 品牌 + 型号
    lens,
    tokenize = 'trigram'
);

-- 序号计数（LIBRARY.md §3.3）：**同一目录下，不同宽度各自独立计数**
CREATE TABLE seq_counters (
    directory TEXT    NOT NULL,                   -- 库内相对目录（photos/2026-08-15）
    width     INTEGER NOT NULL,                   -- 3 / 4 / 5 …
    value     INTEGER NOT NULL,                   -- 最近分配到的值（从 1 开始，写满回绕）
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (directory, width)
);

-- 导入批次（M1-6）：一次导入一条，用于历史与「导入后撤销定位」
CREATE TABLE import_runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_root     TEXT    NOT NULL,
    template        TEXT    NOT NULL,             -- 本次实际使用的模版（模版会变，要留痕）
    include_subdirs INTEGER NOT NULL DEFAULT 1,   -- 是否透传子目录
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    state           TEXT    NOT NULL DEFAULT 'running',  -- running / done / cancelled / failed
    imported        INTEGER NOT NULL DEFAULT 0,
    skipped         INTEGER NOT NULL DEFAULT 0,
    failed          INTEGER NOT NULL DEFAULT 0,
    note            TEXT
);

CREATE INDEX idx_import_runs_started ON import_runs(started_at DESC);

-- 每个批次导入了哪些文件（便于「这次导入都干了什么」与将来撤销）
CREATE TABLE import_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id        INTEGER NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    asset_id      INTEGER REFERENCES assets(id) ON DELETE SET NULL,
    source_path   TEXT    NOT NULL,
    target_rel    TEXT,                           -- 落地后的库内相对路径（失败时为空）
    status        TEXT    NOT NULL,               -- imported / skipped / failed
    reason        TEXT,                           -- skipped/failed 的原因（给用户看）
    created_at    INTEGER NOT NULL
);

CREATE INDEX idx_import_items_run ON import_items(run_id, status);
