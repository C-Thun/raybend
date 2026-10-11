-- catalog.db（每库一个）schema v19：显示尺寸（与方向一样记在库里）
--
-- 出处：`specs/display-size-source.md` §3。多版本尺寸：
--   * 每个文件一份（bitmap / raw 各自的方向与展示尺寸）；
--   * latest 一份（挂在编辑栈行上，与栈同一笔事务更新）；
--   * 每条命名定稿一份（成片输出尺寸，已含方向与几何）。
--
-- 展示尺寸 = oriented_size(原始轴, 方向)；`assets.width/height` 的原始轴语义不变
-- （那是「基准文件」的读数，DTO 边界才归一 —— 见 `browse.rs` 的注释）。
-- 全部 NULL-able：NULL = 未知（旧行），消费方回退旧行为；**无数据重写**（只 ALTER）。
ALTER TABLE asset_files ADD COLUMN display_width INTEGER;
ALTER TABLE asset_files ADD COLUMN display_height INTEGER;
ALTER TABLE asset_files ADD COLUMN orientation INTEGER;      -- 1..8；NULL = 未知
ALTER TABLE develop_stacks ADD COLUMN output_width INTEGER;  -- latest 成片输出尺寸
ALTER TABLE develop_stacks ADD COLUMN output_height INTEGER;
ALTER TABLE issues ADD COLUMN output_width INTEGER;          -- 定稿成片输出尺寸
ALTER TABLE issues ADD COLUMN output_height INTEGER;
