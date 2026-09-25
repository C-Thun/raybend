-- 旧色差拉杆同时移动红 (+d) 与蓝 (-d)。拆分后保留相同通道映射。
INSERT INTO develop_params(asset_id, param_id, value)
SELECT asset_id, 'chromaticBlue', -value FROM develop_params
WHERE param_id = 'chromatic' AND value <> 0
ON CONFLICT(asset_id, param_id) DO NOTHING;
