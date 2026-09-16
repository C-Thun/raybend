-- catalog.db（每库一个）schema v3：文件记录上的**源身份**列
--
-- 出处：REPOSITORY.md §4.3（避免重复导入）、plans/M1-6.md §3.3。
--
-- 为什么不能复用现有的 volume_serial / file_id：那两列是**库内那个文件**的身份
-- （M1-3 的差分、改名识别、缺失检测全靠它）。而「这张源文件之前导进来过吗」
-- 问的是**源文件**的身份 —— 一个列对服务不了两件事：把源身份写进去，
-- 库内差分下次会把每个文件都当成「被替换过」（见 AGENTS.md §7.3 的身份语义）。
--
-- 判重顺序（REPOSITORY.md §4.3）：先比源身份；身份读不到时（网络盘、权限不足、
-- 非 NTFS/inode 的 FS）退化为「source_path_folded + size_bytes + mtime_ms」三项比对 ——
-- 那两项 v1 就在表上了，这里只补身份。

ALTER TABLE asset_files ADD COLUMN source_volume_serial INTEGER;
ALTER TABLE asset_files ADD COLUMN source_file_id       BLOB;    -- 16 字节

-- 身份查询走这条；只索引「真的记下了身份」的行（NULL 行不占索引）
CREATE INDEX idx_asset_files_source_identity
    ON asset_files(source_volume_serial, source_file_id)
 WHERE source_volume_serial IS NOT NULL;
