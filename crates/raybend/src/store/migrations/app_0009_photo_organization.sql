-- 相片桶归应用所有；成员跨库，卸载库不清理。
CREATE TABLE photo_buckets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    name_folded TEXT NOT NULL UNIQUE,
    pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
    rule_json TEXT,
    rule_revision INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE photo_bucket_members (
    bucket_id INTEGER NOT NULL REFERENCES photo_buckets(id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL,
    photo_uid TEXT NOT NULL,
    asset_id INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (bucket_id, repository_id, photo_uid)
);
CREATE INDEX idx_bucket_members_photo ON photo_bucket_members(repository_id, photo_uid);

-- 手动移出自动桶后，任何规则组都不能把这张照片补回来。
CREATE TABLE photo_bucket_exclusions (
    bucket_id INTEGER NOT NULL REFERENCES photo_buckets(id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL,
    photo_uid TEXT NOT NULL,
    excluded_at INTEGER NOT NULL,
    PRIMARY KEY (bucket_id, repository_id, photo_uid)
);

-- 保存新 revision 和待补扫范围在同一个 app.db 事务中完成。
CREATE TABLE photo_bucket_scans (
    bucket_id INTEGER NOT NULL REFERENCES photo_buckets(id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    after_asset_id INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (bucket_id, repository_id)
);

-- 各 catalog 的变化日志独立递增；这里记录已经确认入桶的进度。
CREATE TABLE photo_organization_cursors (
    repository_id TEXT PRIMARY KEY,
    event_id INTEGER NOT NULL DEFAULT 0
);
