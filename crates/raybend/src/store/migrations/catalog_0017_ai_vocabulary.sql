-- 每库首次固定 concept→文字映射；界面语言变化不重命名、不绕过同文字禁止。
CREATE TABLE photo_ai_vocabulary (
    concept_key TEXT PRIMARY KEY,
    tag_key TEXT NOT NULL REFERENCES tag_terms(tag_key)
);
