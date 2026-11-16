# M3-W6c：LUT 与不可变定稿
完成时间：2026-09-26 05:44:40 CST

## 范围与文件

- Pencil 更新 `design/editor.pen`、`design/browse.pen` 并同步同名说明文档。编辑器左侧 LUT tile 与右侧定稿，浏览右侧展示定稿选择落地。
- `app.db` v6 新增 LUT 分类/条目，`catalog.db` v11 新增不可变 issue 与 latest 的 LUT ID/开关；迁移沿用 `store/migration.rs`，v10 旧 latest 保留，目录接入自动升级并留迁移前快照。设备偏好 `raybend.editor.v1` 分类一次性导入 `app.db`，新键为 v2。
- `develop/lut.rs` + `lut_import.rs`：1D/3D/shaper CUBE、PNG/TIFF Hald、三级目录扫描、同名伴生图或内置样片封面、768/384 两档 4:3 WebP Q80。导入文件按稳定 ID 放应用私有目录，原文件移动或重名不影响；删除仅隐藏库条目并保留已被 issue 引用的文件。
- LUT 在同一 Rust 显影/小图/预览管线求值；选中 ID 和应用开关都写入 latest。缺失文件有可见提示，profile 引用保持。
- `store/issues.rs`、`src-tauri/src/issues.rs`：不可变完整 profile、schema_version、规范哈希加全配置匹配、保留名校验、十档影调×十档色彩名称建议与每张照片内的 base36 序号。创建命名定稿时独立写 384/192 AVIF 缩略图和 1920 AVIF 预览；缓存缺失时按需重建。
- 选定稿、撤销、重做统一更新 latest 工作副本和图片/控件状态；定稿按钮只在当前配置与任一现有 issue、SOOC、RAW 不相同时启用。浏览侧切稿也走同一编辑栈提交与撤销历史。

## 决策

- `SOOC`/`RAW` 是源文件衍生的特殊版本，`latest` 是可变工作副本；选中态由当前 profile 推导，不持久化“选中哪个定稿”。命名定稿不可覆盖，用户可另存后删旧稿。
- 命名定稿缩略图在当前 `_sources/thumbs.db` 中以库、照片、定稿 ID 分键；1920 预览在库根 `cache/full/<asset>/` 下按定稿 ID 与 profile 哈希命名。latest 失效不删除命名快照。
- `editor.issue.finalize` 已接统一命令注册表；默认热键留空，避免编辑过程中误触弹窗。LUT 分类/导入/隐藏、issue 删除、浏览右栏选择均是相应控件内的上下文动作，不加全局命令或默认键。

## 验证与遗留

- Rust 核心 1083 项通过、3 项忽略；前端 954 项通过；Windows debug 主程序和 RAW worker 完整构建，`pnpm check:win` 核对资源通过。七个 `C:\src\resource` CUBE 样本均通过解析冒烟，封面测试验证 VP8 有损 WebP 与 4:3 尺寸。迁移演练从 catalog v1 自动升 v11，旧标记数据与备份保留。
- Agent 未做真实 Windows GUI、色彩正确性或真实照片 E2E。定稿切换的视觉流畅度、封面与画面颜色、真实照片快照需人类验收，步骤见 `plans/M3-W7.md`。
