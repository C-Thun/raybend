-- catalog.db（每库一个）schema v18：库级回收站
--
-- 出处：`specs/library-recycle-bin.md` §3。删除 = 移入库内 `recycle/`；
-- 文件层是真实移动（rename），数据层只加标记：
--   * assets.deleted_at        —— 是否在回收站（Unix ms；NULL = 不在）
--   * asset_files.recycled_from —— 进回收站前的原 rel_path（还原用；NULL = 不在回收站）
--
-- 不变量：**asset_files.rel_path 永远描述文件的真实位置；deleted_at 描述它是否在回收站。**
-- 评级 / 标签 / 编辑栈 / issue / 桶成员一律不动 —— 还原后一切照旧
-- （所以本迁移**只**加这两列与一个索引，别的都不碰）。
ALTER TABLE assets ADD COLUMN deleted_at INTEGER;
ALTER TABLE asset_files ADD COLUMN recycled_from TEXT;

-- 入口行的「有内容才出现」与列表查询都按 deleted_at 判 —— 局部索引让它在整库尺度上也便宜。
CREATE INDEX idx_assets_deleted_at ON assets(deleted_at) WHERE deleted_at IS NOT NULL;
