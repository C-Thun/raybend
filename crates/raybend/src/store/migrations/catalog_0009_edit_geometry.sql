-- catalog.db v9：W5 无损成片几何。NULL 表示原图方向、未裁切。
-- JSON 的 rotation/crop 均在 develop::geometry 校验；撤销把它视为一项设置。
ALTER TABLE develop_stacks ADD COLUMN edit_geometry TEXT;
