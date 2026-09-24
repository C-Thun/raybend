-- catalog.db（每库一个）schema v6：镜头校正与降噪方式（M3-W4）
--
-- 三列都遵循同一套「只存非默认值」口径：**NULL = 没动过**。
-- 这样「重置这一组」就是 DELETE 掉那几列的值，与参数的语义一致。
--
-- # `lens_profile`：镜头配置文件（lensfun 的 `maker|model` 稳定键）
--
-- * `NULL`   —— 没动过 ⇒ 用 EXIF **自动识别**（识别不到就没有配置文件）
-- * `'none'` —— 用户**显式关掉了自动匹配**（与 NULL 不同：NULL 是「还没表态」）
-- * 其它     —— 用户从下拉里选的那一支（键的形态见 `lens::LensProfile::key_of`）
--
-- 为什么不把 `'none'` 做成 NULL：自动识别是**会变的**（同一张照片换了 EXIF 读法、
-- 或库里新增了这支镜头）—— 用户说「我不要自动匹配」这件事必须能存住。
--
-- # `lens_enabled`：配置文件那一半的开关
--
-- * `NULL` —— 默认（**开**）
-- * `0`    —— 用户关掉了「启用校正」的**配置文件部分**
--
-- ❗ 它**只管配置文件**：三根手动拉杆（畸变 / 暗角 / 色差）不受它影响
-- （人类 2026-09-25 拍板，与 Lightroom 的 Lens Corrections 面板一致）。
--
-- # `nr_method`：降噪方式
--
-- * `NULL`   —— 快速档（默认；`develop::denoise` 的多尺度保边收缩，实时跟手）
-- * `'high'` —— 高质量档（BM3D，后台任务，见 `plans/M3-W4.md` §2.1）
--
-- 为什么它必须进库（而不能是全局偏好）：同一张照片选不同方式 ⇒ **画面不同**，
-- 而缩略图缓存键（`DevelopStack::signature`）只能看见这张表里的东西 ——
-- 放全局偏好就会「改了设置、缩略图还是旧的」（毒缓存）。
--
-- 为什么不用一个通用的「设置表」：这三项都属于**这一张照片的编辑**，
-- 与 `develop_params` 是同一层的东西；放别处会让「重置全部」漏掉它们。

ALTER TABLE develop_stacks ADD COLUMN lens_profile TEXT;
ALTER TABLE develop_stacks ADD COLUMN lens_enabled INTEGER;
ALTER TABLE develop_stacks ADD COLUMN nr_method TEXT;
