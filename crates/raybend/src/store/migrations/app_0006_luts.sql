CREATE TABLE lut_categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE luts (
    id TEXT PRIMARY KEY,
    category_id TEXT NOT NULL REFERENCES lut_categories(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    original_path TEXT NOT NULL,
    file_rel_path TEXT NOT NULL UNIQUE,
    cover_rel_path TEXT NOT NULL UNIQUE,
    format TEXT NOT NULL CHECK (format IN ('cube','hald')),
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_luts_category ON luts(category_id, hidden, created_at);
