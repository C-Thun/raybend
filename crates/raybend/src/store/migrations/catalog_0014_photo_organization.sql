-- 本库自己的照片身份与标签文字真相。旧 asset_tags 保留，用于迁移和既有命令兼容。
ALTER TABLE assets ADD COLUMN organization_uid TEXT;
UPDATE assets SET organization_uid = lower(hex(randomblob(16))) WHERE organization_uid IS NULL;
CREATE UNIQUE INDEX idx_assets_organization_uid ON assets(organization_uid);
CREATE TRIGGER assets_organization_uid_insert AFTER INSERT ON assets
WHEN NEW.organization_uid IS NULL
BEGIN
    UPDATE assets SET organization_uid = lower(hex(randomblob(16))) WHERE id = NEW.id;
END;

CREATE TABLE tag_terms (
    tag_key TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    legacy_tag_id INTEGER UNIQUE
);

CREATE TABLE asset_tag_sources (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_key TEXT NOT NULL REFERENCES tag_terms(tag_key),
    source TEXT NOT NULL CHECK (source IN ('manual', 'ai')),
    tagged_at INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_key, source)
);
CREATE INDEX idx_asset_tag_sources_tag ON asset_tag_sources(tag_key, asset_id, source);

-- 目录标签只创建一个目录入口，不让其中的照片继承这个词。
CREATE TABLE directory_tags (
    directory_key TEXT NOT NULL,
    tag_key TEXT NOT NULL REFERENCES tag_terms(tag_key),
    tagged_at INTEGER NOT NULL,
    PRIMARY KEY (directory_key, tag_key)
);
CREATE INDEX idx_directory_tags_tag ON directory_tags(tag_key, directory_key);

-- 与照片标记同事务记变化；后台按游标重放，即使崩溃也不漏事件。
CREATE TABLE photo_organization_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id INTEGER NOT NULL,
    kind TEXT NOT NULL
);
CREATE INDEX idx_photo_organization_events_asset ON photo_organization_events(asset_id, id);

CREATE TRIGGER photo_organization_asset_insert AFTER INSERT ON assets BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES (NEW.id, 'asset');
END;
CREATE TRIGGER photo_organization_asset_mark AFTER UPDATE OF rating, color_label, like_state, lock_level ON assets BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES (NEW.id, 'mark');
END;
CREATE TRIGGER photo_organization_tag_attach AFTER INSERT ON asset_tags BEGIN
    INSERT OR IGNORE INTO asset_tag_sources(asset_id, tag_key, source, tagged_at)
        SELECT NEW.asset_id, tag_key, 'manual', NEW.tagged_at
        FROM tag_terms WHERE legacy_tag_id = NEW.tag_id;
END;
CREATE TRIGGER photo_organization_tag_detach AFTER DELETE ON asset_tags BEGIN
    DELETE FROM asset_tag_sources WHERE asset_id = OLD.asset_id AND source = 'manual'
        AND tag_key IN (SELECT tag_key FROM tag_terms WHERE legacy_tag_id = OLD.tag_id);
END;
-- All tag producers (legacy manual now, AI later) emit the same effective-change event.
CREATE TRIGGER photo_organization_source_attach AFTER INSERT ON asset_tag_sources BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES (NEW.asset_id, 'tag');
END;
CREATE TRIGGER photo_organization_source_detach AFTER DELETE ON asset_tag_sources BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES (OLD.asset_id, 'tag');
END;
