-- app.db（全局库）schema v1
--
-- 存什么：应用设置 / 库注册表 / 任务队列。
-- 真相源分工见 AGENTS.md §6.4 与 LIBRARY.md §2：**库自己的数据在各自的 catalog.db 里**，
-- 这里只记「有哪些库、它们的路径、它们的在线状态」。
--
-- 约定：
--   * 时间一律 Unix 毫秒（INTEGER），UTC（store::time）
--   * 路径存两份：`path`（原始，展示用）与 `path_folded`（NFC + 小写，比较/索引用）
--     —— 见 store::path_semantics

-- 库注册表：一个库一条记录（身份 = catalog.db 里的 library_id）
CREATE TABLE libraries (
    id              TEXT    PRIMARY KEY,          -- 库唯一 ID（可排序的 16 位 base62，LIBRARY.md §2.5）
    name            TEXT    NOT NULL,             -- 展示名（用户可改）
    import_template TEXT,                         -- 导入模版缓存（真相源在 catalog.db，离线时也要能显示）
    photos_dir      TEXT    NOT NULL DEFAULT 'photos',  -- 库内落地目录名（FUTURE G14：将来可配）
    created_at      INTEGER NOT NULL,
    last_opened_at  INTEGER                       -- 最近打开时间，用于列表排序
);

-- 库 → 路径：一个库可登记多条路径（同库多路径），一个路径也可登记多个库（同路径不同库）
CREATE TABLE library_paths (
    library_id   TEXT    NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path         TEXT    NOT NULL,                -- 原始路径（展示/打开用）
    path_folded  TEXT    NOT NULL,                -- NFC + 折叠（比较用）
    added_at     INTEGER NOT NULL,
    last_seen_at INTEGER,                         -- 最近一次确认「该路径下确实有这个库」
    status       TEXT    NOT NULL DEFAULT 'unknown',  -- online / offline / unknown
    PRIMARY KEY (library_id, path_folded)
);

CREATE INDEX idx_library_paths_folded ON library_paths(path_folded);

-- 应用设置（KV，值是 JSON 文本）
CREATE TABLE settings (
    key        TEXT    PRIMARY KEY,
    value      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 持久化任务队列：扫描、缩略图、导入都用它（M1-3 / M1-6）
CREATE TABLE jobs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT    NOT NULL,                  -- scan / thumbnail / import / ...
    payload    TEXT    NOT NULL DEFAULT '{}',     -- JSON：任务参数
    state      TEXT    NOT NULL DEFAULT 'pending',-- pending / running / done / failed / cancelled
    attempts   INTEGER NOT NULL DEFAULT 0,
    priority   INTEGER NOT NULL DEFAULT 0,        -- 大的先跑
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    error      TEXT                               -- 失败原因（给用户看）
);

-- 取下一个待跑任务、统计状态都要快
CREATE INDEX idx_jobs_state_priority ON jobs(state, priority DESC, id);
CREATE INDEX idx_jobs_kind ON jobs(kind, state);

-- 应用级元信息：首次运行时间、上次完整性检查、上次缓存回收……
CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
