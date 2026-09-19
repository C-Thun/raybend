-- app.db schema v4：**目录级计数**（人类 2026-09-19 定的数量体系）
--
-- 两个概念（口径是人类的原话，实现与界面都按它走）：
--   * **相片数量**（`photos_count`）＝ 本目录内、**不含** `_RAW/` 里的文件；
--   * **图片数量**（`images_count`）＝ 本目录内 + 本目录下 `_RAW/` 里的**全部**文件。
--
-- 为什么放在 app.db 而不是各自的 catalog.db：
--   库列表（导入右列 / 浏览左列）要显示**所有库**的张数，
--   而 app.db 是「有哪些库」的总表 —— 放这里才能一次读完，
--   不为了几个数字去逐个打开库的 catalog.db（老实现就是那样，慢盘上几秒起步）。
--
-- 一致性：`repositories.photos_count/images_count` 是**汇总缓存**，
-- 由 `directories` 里该库所有目录的行求和而来（每次写目录就在同一个事务里重算）。

ALTER TABLE repositories ADD COLUMN photos_count INTEGER;
ALTER TABLE repositories ADD COLUMN images_count INTEGER;

-- 库里的一个目录一行。**只统计本目录 + 本目录下的 `_RAW`**，不含其它子目录
-- （子目录各有各的行；整库的数字靠 SUM 汇总）。
CREATE TABLE directories (
    repository_id   TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    rel_path        TEXT    NOT NULL,            -- 库内相对路径（`photos/2026-08-15`）
    rel_path_folded TEXT    NOT NULL,            -- NFC + 折叠（比较/索引用，见 store::path_semantics）
    photos_count    INTEGER NOT NULL DEFAULT 0,
    images_count    INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (repository_id, rel_path_folded)
);

CREATE INDEX idx_directories_repository ON directories(repository_id);
