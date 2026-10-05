-- app.db schema v10：用户导入的 RGB ICC 原件；内置 sRGB/P3/Adobe 不占行。
-- profile_id 是原始字节 SHA-256，文件落在 app data/color-profiles/<id>.icc。
-- hidden 只移除选择入口，不能删除已经被照片或定稿引用的字节。
CREATE TABLE color_profiles (
    profile_id TEXT PRIMARY KEY CHECK (
        length(profile_id) = 64 AND profile_id NOT GLOB '*[^0-9a-f]*'
    ),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
    original_filename TEXT NOT NULL CHECK (length(original_filename) BETWEEN 1 AND 512),
    profile_class TEXT NOT NULL CHECK (
        profile_class IN ('input', 'display', 'output', 'color_space')
    ),
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_color_profiles_visible ON color_profiles(hidden, created_at, profile_id);
