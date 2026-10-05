-- 文本标签的屏蔽盖住所有来源；AI 的历史证据留存。
CREATE TABLE asset_tag_masks (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_key TEXT NOT NULL REFERENCES tag_terms(tag_key),
    masked_at INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_key)
);
CREATE TABLE photo_ai_results (
    asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    source_key TEXT NOT NULL,
    result_json TEXT NOT NULL,
    valid INTEGER NOT NULL CHECK(valid IN (0, 1))
);
-- begin/cancel/commit 在 catalog 单写者事务内串行；token 是不可复用的随机值。
CREATE TABLE photo_ai_attempts (
    asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    token TEXT NOT NULL,
    photo_uid TEXT NOT NULL,
    source_key TEXT NOT NULL
);
CREATE VIEW effective_photo_tags AS
SELECT DISTINCT s.asset_id, s.tag_key FROM asset_tag_sources s
WHERE NOT EXISTS (SELECT 1 FROM asset_tag_masks m WHERE m.asset_id=s.asset_id AND m.tag_key=s.tag_key)
AND (s.source='manual' OR EXISTS (SELECT 1 FROM photo_ai_results r WHERE r.asset_id=s.asset_id AND r.valid=1));
-- 未对齐的旧 ID 仍作为兼容读取；对齐之后只使用同一个文字投影。
CREATE VIEW effective_photo_tag_ids AS
SELECT e.asset_id, t.legacy_tag_id AS tag_id FROM effective_photo_tags e
JOIN tag_terms t ON t.tag_key=e.tag_key WHERE t.legacy_tag_id IS NOT NULL
UNION
SELECT a.asset_id, a.tag_id FROM asset_tags a
WHERE NOT EXISTS (SELECT 1 FROM tag_terms t WHERE t.legacy_tag_id=a.tag_id);
CREATE TRIGGER photo_organization_mask_attach AFTER INSERT ON asset_tag_masks BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES(NEW.asset_id, 'tag');
END;
CREATE TRIGGER photo_organization_mask_detach AFTER DELETE ON asset_tag_masks BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES(OLD.asset_id, 'tag');
END;
CREATE TRIGGER photo_organization_ai_insert AFTER INSERT ON photo_ai_results BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES(NEW.asset_id, 'tag');
END;
CREATE TRIGGER photo_organization_ai_update AFTER UPDATE ON photo_ai_results BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES(NEW.asset_id, 'tag');
END;
CREATE TRIGGER photo_organization_ai_delete AFTER DELETE ON photo_ai_results BEGIN
    INSERT INTO photo_organization_events(asset_id, kind) VALUES(OLD.asset_id, 'tag');
END;
-- 所有老手动入口（含 marking::Op）增加标签时都解除同文字屏蔽。
CREATE TRIGGER photo_manual_unmask AFTER INSERT ON asset_tags BEGIN
    DELETE FROM asset_tag_masks WHERE asset_id=NEW.asset_id AND tag_key IN
        (SELECT tag_key FROM tag_terms WHERE legacy_tag_id=NEW.tag_id);
END;
