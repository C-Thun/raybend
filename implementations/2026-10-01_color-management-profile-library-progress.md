2026-10-01 02:36:27 CST

# 色彩配置文件库阶段实施

## 改动范围

- `design/main.pen` 与同名说明：设置的配置文件页补深色紧凑、浅色宽松两帧，展示导入、资源保全、表格、缺失/隐藏等状态；Pencil 文件须由崔总手动保存，当前不提交。
- `specs/cm-w4.md`：本波资产、默认、照片级批量、预设与 XMP 的依赖顺序；未完成项保持开放。
- `crates/raybend/src/fs_asset.rs`、`develop/lut_import.rs`：将 LUT 原有受限哈希/原子快照抽成共同底座，ICC 复用，格式验证仍各自独立。LUT 旧测试改测这一个共同函数。
- `crates/raybend/src/color/assets.rs`：ICC 16 MiB 上限、RGB v2/v4 校验、输入/输出可用性试变换、按内容身份无覆盖复制，使用时重新核原件 SHA。
- `store/migrations/app_0010_color_profiles.sql`、`store/color_profiles.rs`：按既有框架迁移 app.db，内容指纹去重、隐藏与重复导入恢复；内置 sRGB/P3/Adobe 不占用户表。并发入库、非法 ID、中文名称与恢复有单测。
- `src-tauri/src/color_profiles.rs`、`src/api/color.ts`、`src/api/dialog.ts`、`src/shell/GlobalSettingsDialog.tsx`、中英语言包：批量文件选择、后台逐件导入、内置/导入/缺失列表、报告、隐藏入口。未接入照片级应用按钮。

## 决策

内容 SHA-256 是外部原件身份；同名不同字节并存，同字节不同名只一份。隐藏只改选择入口，文件保留。列表按本地磁盘是否存在作轻量状态读取，精确使用时重新校验内容与转换能力；不能把“文件存在”当成显示已校准。导入从用户选择的文件读取受限快照，校验后才原子发布，DB 仍走 app.db 单写者。若写库失败，文件可能成为未登记孤儿，但不能冒险删除可能已被并发引用的原件；后续需做安全回收。

命令体系：全局设置已通过 `settings.open`（`Mod+,`）接入菜单/命令面板；“导入配置文件”和逐行隐藏属于设置弹窗内上下文操作，默认键明确留空，不另建全局命令以免脱离当前页和文件选择器。

## 已验证与待验证

Pencil 两帧截图及布局检查通过。`cargo check --workspace`、`pnpm typecheck`、`pnpm lint:colors`、`lint:arch`、`lint:i18n`、`pnpm build`、前端 123 项测试通过；`store::color_profiles` 3 项单测通过。完整 Rust 回归、ICC 资产测试、Windows 构建、真实桌面文件选择和 GUI 目视仍待完成。新页面不代表照片渲染管线已切到 ICC。

## 遗留

CM-W4 的默认规则、照片级/批量指定、预设可选组、XMP 扩展尚未接线，依赖 W2 高精度处理与正在并行设计的 XMP schema。W3 显示端也未接，不能用当前页面的资产文件替代系统显示 adapter。资源库大量条目的 UI 体感与导入长任务、磁盘损坏/权限异常仍需真机验证。Pencil 需要崔总手动保存后再纳入提交。
