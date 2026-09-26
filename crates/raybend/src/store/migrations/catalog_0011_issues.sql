ALTER TABLE develop_stacks ADD COLUMN lut_id TEXT;
ALTER TABLE develop_stacks ADD COLUMN lut_enabled INTEGER CHECK (lut_enabled IN (0,1));
CREATE TABLE issues (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL,
    profile_json TEXT NOT NULL,
    profile_hash TEXT NOT NULL,
    source_base TEXT NOT NULL CHECK (source_base IN ('raw','sooc')),
    created_at INTEGER NOT NULL,
    UNIQUE(asset_id, name)
);
CREATE INDEX idx_issues_asset_time ON issues(asset_id, created_at DESC, id DESC);
CREATE INDEX idx_issues_hash ON issues(asset_id, profile_hash);
