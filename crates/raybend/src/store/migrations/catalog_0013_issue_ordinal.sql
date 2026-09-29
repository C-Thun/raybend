-- catalog.db v13: issue 导出序号（specs/export-issue-ordinal.md）。
-- 尾号 I00–I99 每资产唯一；assets.issue_counter 是「下一候选」游标（0–99 循环）。
-- 存量回填：每资产按 (created_at, id) 升序 0,1,2,…；游标 = 该资产 issue 数 % 100。
ALTER TABLE issues ADD COLUMN ordinal INTEGER;
ALTER TABLE assets ADD COLUMN issue_counter INTEGER NOT NULL DEFAULT 0;
UPDATE issues SET ordinal = (
    SELECT COUNT(*) FROM issues AS older
    WHERE older.asset_id = issues.asset_id
      AND (older.created_at, older.id) < (issues.created_at, issues.id)
);
CREATE UNIQUE INDEX idx_issues_asset_ordinal ON issues(asset_id, ordinal);
UPDATE assets SET issue_counter = (
    SELECT COUNT(*) FROM issues WHERE issues.asset_id = assets.id
) % 100;
