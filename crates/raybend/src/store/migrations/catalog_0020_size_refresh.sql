-- catalog.db（每库一个）schema v20：老尺寸记录置空，等待按新口径重建
--
-- 出处：`specs/display-size-lifecycle.md` §4。
--
-- 为什么需要：RAW 的展示尺寸口径改了 —— 从「我们自己解文件头得到的整帧」
-- （RW2 实测 5264×3904）改成「**rawler 出图尺寸**」（5184×3888，相机 4:3 有效区）。
-- 崔总 2026-10-11：
--   「图片尺寸以 rawler 回传为主……对 RAW 来说不能用私自解算像素数来替代正式
--     develop 输出的真实位图像素数。」
-- 老库里基于旧口径写下的值与新口径差 1.5%（甚至错值：真库有一条 RAW 基的定稿
-- 存了位图的尺寸），必须重建。
--
-- 做法（崔总点头的便宜方案）：**只置 NULL，不在迁移里开 RAW**（大库慢、还要在
-- 迁移里调 rawler）。置空后由运行期按新口径补回：
--   * `store::sizes::ensure_sizes`（进 editor / 取快照时顺手补，逐张、幂等、只写空值）；
--   * 整库一次补齐的维护动作（`backfill-metadata --sizes`）。
-- 补回的算法只有一套：`RawlerBackend::output_size`（rawler 出图尺寸）。
--
-- 边界：
--   * **位图行不动**（它们的源没变，我们自己的文件头解析就是权威）；
--   * `source_base='sooc'` 的编辑栈 / 定稿不动（源尺寸没变，输出尺寸仍然成立）；
--   * 新库不受影响（导入时就按新口径写）；
--   * 置空的是**派生值**，不是真相源 —— 原图、栈本体、定稿配置一个字节都没动。

UPDATE asset_files
   SET display_width = NULL, display_height = NULL
 WHERE role = 'raw';

UPDATE develop_stacks
   SET output_width = NULL, output_height = NULL
 WHERE source_base = 'raw';

UPDATE issues
   SET output_width = NULL, output_height = NULL
 WHERE source_base = 'raw';
