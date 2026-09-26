-- M3-W6a: 全局机型基础曲线档案。档案曲线不可变；命中只增加样本计数。
CREATE TABLE camera_base_curves (
    id INTEGER PRIMARY KEY,
    camera_make TEXT NOT NULL,
    camera_model TEXT NOT NULL,
    make_key TEXT NOT NULL,
    model_key TEXT NOT NULL,
    name TEXT NOT NULL,
    points TEXT NOT NULL,
    sample_count INTEGER NOT NULL DEFAULT 1 CHECK (sample_count >= 1),
    created_at INTEGER NOT NULL
);
CREATE INDEX camera_base_curves_model ON camera_base_curves(make_key, model_key, id);
