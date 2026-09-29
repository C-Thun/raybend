# tiles 小图读取与慢盘导入

2026-09-29 崔总要求：tiles/film 优先 thumb，缺失才复用 preview；editor 主视口继续原图显影。export 的原始 RAW 排在 SOOC 后，显示内嵌图并仅在 UI 暗化、降饱和；真实导出仍从 RAW 生成。import 不为显示小格子扫描整目录图像内容。

## 读取约束

- 保持进入目录重新列举磁盘，不引入目录过期缓存。
- import 列举完成即展示；tiles 比例使用已解码小图尺寸，真实尺寸仅 view/compare 按需补读。显式按拍摄时间分组仍需读时间，分批、可停止旧目录后续批次。
- 共用缩略图队列撤销未开始的离屏请求；切目录不能把仍在飞的请求从并发计数中抹掉。
- 未编辑 JPG 优先 EXIF JPEG 缩略图；不存在时才解码 JPEG。RAW 小图优先有界读取内嵌 JPEG，兼容兜底仍在 worker 内且禁止传感器解码。
- export 用既有 issue thumb、latest thumb 与 preview 缓存。只缺小图不能生成额外 export 专属 1920 图；缺失全部派生缓存时保留正确重建能力。
- 原始 RAW 显示效果不写回缓存/编辑栈，不改变输出文件。排序为命名定稿、SOOC、RAW（已有操作提升仍适用于命名定稿）。
- 复用 Tile、ThumbQueue、TIFF 解析器、thumbnail cache；不新增命令/热键（既有读取/展示行为优化）。

## 验收

队列换目录并发、离屏取消、过期结果；import 无全目录 metadata 门、view 仍补自然尺寸；内嵌 JPEG 越界/损坏/方向/缺失；RAW 小图禁止完整解码；export 命中小图不读 preview，RAW 末位。前端质量门、Rust 单测与 workspace check。真实旧机械盘首屏/拖动速度与 RAW 格子观感由崔总确认。

本会话无可用 pi todo；本文件为规格，不作替代进度表。

## 追加：定稿卡片与生成时机

保留 import 取方向要求，图像首次出现时即按 Rust 提供的小图方向/比例显示。SOOC / RAW / latest 三个空白卡片接入共用取图。导入收尾消费已持久化任务，生成 SOOC 1920 和 384/192；初始 latest 显示复用 SOOC。选中进入编辑后后台生成原始 RAW 两档 thumb（不额外编码 RAW 1920）；export 优先真实 RAW thumb，缺失才内嵌图加 CSS。更新与失效按源版本/栈签名；不增加数据库迁移通道。
