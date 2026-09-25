-- catalog.db v7: latest issue 的唯一源。旧栈没有记录用户当时的 SOOC/RAW 选择；
-- 迁移按当时编辑器默认 RAW 推断：有可用 RAW 就归 RAW，否则归 SOOC。
-- 这是旧数据的不可消除歧义，不改任何用户的参数和镜头设置。
ALTER TABLE develop_stacks ADD COLUMN source_base TEXT NOT NULL DEFAULT 'raw'
    CHECK (source_base IN ('raw', 'sooc'));
UPDATE develop_stacks SET source_base = 'sooc'
WHERE NOT EXISTS (
    SELECT 1 FROM asset_files
    WHERE asset_files.asset_id = develop_stacks.asset_id
      AND asset_files.role = 'raw'
      AND asset_files.missing_since IS NULL
);
