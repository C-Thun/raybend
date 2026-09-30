-- 编辑预设（editor presets，specs/editor-presets.md §2）。
-- 设备级资产（照 LUT 分类的口径）：预设是可复用的编辑参数包，换库仍可用。
-- `default` 目录由后端在读取库时惰性创建（id 固定 "default"，名称走语言包，前端不显示 DB name）。
CREATE TABLE preset_dirs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE presets (
    id TEXT PRIMARY KEY,
    directory_id TEXT NOT NULL REFERENCES preset_dirs(id) ON DELETE RESTRICT,
    name TEXT NOT NULL COLLATE NOCASE,
    -- 大类快照 JSON（specs/editor-presets.md §3）；后端只校验可解析，语义归前端。
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
-- 同目录内预设名唯一（大小写不敏感）。
CREATE UNIQUE INDEX idx_presets_dir_name ON presets(directory_id, name);
