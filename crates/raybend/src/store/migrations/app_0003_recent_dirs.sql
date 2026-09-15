-- app.db（全局库）schema v3：最近导入过的目录
--
-- 出处：design/main.md §3.1.1（左列「最近」：不需要用户收藏，自动记录最近 50 条，
-- 每条背后存完整路径）+ plans/M1-5.md §3.2。
--
-- 为什么单独一张表，而不是塞进 `settings` 的 JSON：
--   * 要按「最近使用」排序、要能**单条移除**、要能在每次记一条时**裁剪到上限** ——
--     这三件事都是集合语义，用表比用一段 JSON 更直接，也不会出现「读改写」的并发窗口；
--   * 与 `repository_paths` 一样，路径存两份：`path`（原始，展示/打开用）
--     与 `path_folded`（NFC + 小写折叠，**唯一性判据**）—— 见 store::path_semantics。
--
-- 注意：这张表只是「用户最近挑过哪些目录」的便利记录，**不是库的登记**
-- （库的登记在 `repositories` / `repository_paths`）。删掉整张表只影响便利性。

CREATE TABLE recent_dirs (
    path            TEXT    NOT NULL,            -- 原始路径（保留第一次记下时的拼写）
    path_folded     TEXT    NOT NULL PRIMARY KEY, -- NFC + 小写折叠
    include_subdirs INTEGER NOT NULL DEFAULT 0,  -- 勾选这条目录时的「包含子目录」状态
    used_at         INTEGER NOT NULL,            -- 最近一次使用（Unix 毫秒）
    use_count       INTEGER NOT NULL DEFAULT 1   -- 累计使用次数（将来可按 frecency 排序）
);

-- 取「最近 N 条」是这张表唯一的读模式
CREATE INDEX idx_recent_dirs_used ON recent_dirs(used_at DESC, path_folded);
