-- 旧条目保留 ID 与引用；缺失文件暂时无法补哈希，保持 NULL。
ALTER TABLE luts ADD COLUMN file_hash TEXT CHECK (
    file_hash IS NULL OR (length(file_hash) = 64 AND file_hash NOT GLOB '*[^0-9a-f]*')
);
-- 旧版本可能已有同内容的多个条目，不能静默合并它们的照片引用。
CREATE INDEX idx_luts_file_hash ON luts(file_hash);
