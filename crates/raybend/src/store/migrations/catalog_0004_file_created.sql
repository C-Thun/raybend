-- catalog.db（每库一个）schema v4：文件的**创建时间**
--
-- 出处：人类 2026-09-19 的右栏规格 —— 「文件基础信息」要显示**创建日期**。
-- 它和 `assets.taken_at`（拍摄时间，EXIF）不是一回事：那是照片被拍下的时刻，
-- 这是这个文件在磁盘上被创建的日期（拷来拷去之后两者会差很远）。
--
-- 为什么单独一列而不是复用 `mtime_ms`：修改时间会被「打开一次就写回」的软件改掉，
-- 拿它当创建日期会随软件行为漂移。取不到出生时间（某些文件系统没有）时留 NULL，
-- 显示端**退回 mtime**（至少有个日期可看），这个 fallback 写在查询里。

ALTER TABLE asset_files ADD COLUMN file_created_ms INTEGER;
