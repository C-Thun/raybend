-- catalog.db（每库一个）schema v21：asset_files 的内容指纹（导入判重第三级判据）
--
-- 出处：`specs/import-dedup-fingerprint.md` §4 与 §5。
--
-- 为什么需要：源身份（`source_volume_serial` / `source_file_id`）与「源路径 + 大小 + mtime」
-- 只能认出「同一个来源」，认不出「同一份内容」——同一批照片换个目录再导一次（拷贝一份）
-- 时两条判据都不命中，于是整批重复导入。内容指纹补齐这一级：
--   目标名相同 + 大小相同 → 比指纹；相同就跳过，不同才按重名规则让位（`_01`）。
--
-- 口径（由媒体类型决定、**不随大小变**）：
--   * `sha256-full`   —— 整文件字节的 sha256（位图）；
--   * `sha256-sample` —— `size ‖ head 1 MiB ‖ mid 1 MiB ‖ tail 1 MiB`（RAW 及其它大文件，
--                        单张 10–25 MB 全文哈希换不来判重收益）。
--
-- 两列都可空：老库里没有指纹（NULL = 还没建立）。
-- 建立时机（spec §6）：
--   * 导入的新文件 —— 复制完对库内副本算一次，随登记一起写进来；
--   * 库内老照片 —— **命中同名候选时**才实时算一次，随后回填这两列（不做全库批量回填）。
-- 所以这两列是**派生缓存**，删了不会丢真相（下次判重时会重新建立）。

ALTER TABLE asset_files ADD COLUMN content_hash      TEXT;
ALTER TABLE asset_files ADD COLUMN content_hash_kind TEXT;
