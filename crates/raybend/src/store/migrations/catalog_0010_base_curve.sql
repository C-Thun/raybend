-- M3-W6a: NULL = 新照片尚未选择；none = 明确不用；其它 = 全局档案 ID。
ALTER TABLE develop_stacks ADD COLUMN base_curve_profile TEXT;
ALTER TABLE develop_stacks ADD COLUMN base_curve_points TEXT;
-- 旧照片的编辑画面原本没有基础曲线，显式归到「不使用」。
UPDATE develop_stacks SET base_curve_profile = 'none';
