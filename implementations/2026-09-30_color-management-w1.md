2026-09-30 23:43:15 CST

# CM-W1：色彩契约、设置入口与系统 adapter 接缝

## 改动范围与文件

- `specs/color-management.md` 改为崔总已授权开工；`specs/cm-w1.md` 明确首波验收。已进入排期的色彩任务留 `memory/PLAN.md`，远期余项留 `memory/FUTURE.md`。
- `design/main.pen` / `.md`：深色紧凑与浅色宽松设置弹窗、flowbar 齿轮；按崔总追加意见改为标准齿轮，设置与全屏默认无底无边，悬浮辅色底、设置窗口开启时主色底，16px 图标与四个 flow 图标一致。画布旧磁铁占位同步改全屏。`design/editor.pen` / `.md`：第三组增加色彩管理页签和输入/工作/显示/打样分区。Pencil 工具内已截图并查布局裁切；崔总表示 `.pen` 最终保存由他明天手动执行。
- `src/shell/GlobalSettingsDialog.tsx`、`src/App.tsx`、`src/shell/FlowBar.tsx`、`src/components/ui/Button.tsx` / `Dialog.tsx`、`src/styles/tokens.css`、`src/i18n/*`：菜单、命令面板、`Mod+,`、flowbar 共用全局设置弹窗；点击区紧凑 32px / 宽松 38px，图标 16px。配置文件尚不能安全导入时明确说明能力状态，不提供假成功操作。
- `src/features/commands/ShortcutSettingsDialog.tsx` / `index.ts`、`catalog.ts`：快捷键编辑逻辑抽成同一 `ShortcutSettingsPanel` 嵌入全局设置；保留帮助菜单旧入口，默认 `Mod+,` 移给全局设置，旧入口明确不占默认键。`src/dev/KitchenSink.tsx` 更新组件实例。
- `crates/raybend/src/color/`、`crates/raybend/src/lib.rs`：照片源色彩身份、工作/输出目标与显影版本契约；旧数据缺字段时仍为旧 sRGB 处理版本。系统显示读取采用 `ColorSystemAdapter` trait、显式不可用降级、无头 fake 与统一快照校验；没有 Tauri 依赖、没有把屏幕身份写进照片。
- `scripts/ui-smoke.mjs`、`scripts/check-flowbar.mjs`、`src/features/commands/catalog.test.ts`：入口、尺寸、弹窗选中态与命令回归。

## 决策与边界

- 全局设置是设备级入口；照片输入指定属于编辑栈，导出目标属于导出配方，显示器配置仅属于当前系统环境。W1 只建立类型与 adapter 接缝，**尚未转换像素、解析 ICC 或声称屏幕显示准确**。
- 新的核心色彩类型与旧 `DevelopProfile` 暂不耦合。只有 W2 的输入/输出链真正接通时才把新处理版本写进持久照片状态，以免空 schema 改动导致旧 issue 变色。系统 adapter 的 Windows 真正探测在 W3 接入，当前设置页明确显示未启用。
- 设置弹窗复用已有 `Dialog` 与 `IconButton`，只增加尺寸变体；快捷键编辑沿用原草稿/冲突检测，不复制业务逻辑。全局设置组装放在 shell 层，遵守 feature 间不可互相引用的架构规则。
- 依据崔总 2026-09-30 夜间指示，**本轮新增改动不提交**；此前按其要求完成的提交 `1913d7a` 不受影响。

## 已验证（Agent 冒烟 / 自动检查）

- Pencil：两份设置稿与 editor 第三页签截图自查，无节点裁切；磁铁替换与设置按钮样式已更新画布。`.pen` 最终磁盘保存由崔总复核。
- `pnpm typecheck`、`pnpm test`（123 个文件）、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build` 均通过。受环境中 pnpm@12.3.4 身份在线校验无法连接 registry 的影响，使用本机 pnpm 12.6.0 并设置 `npm_config_manage_package_manager_versions=false` 运行同一脚本。
- `pnpm smoke:ui http://127.0.0.1:1420` 通过，`problems: []`；新增设置断言实测宽松 38px / 图标 16px、弹窗可开关、按钮常亮能复原。`pnpm check:flowbar http://127.0.0.1:1420/` 的 38 项通过。
- `cargo check --workspace`、`cargo test --workspace` 通过。新 `color` 文件单独用 `rustfmt` 格式化；全仓 `cargo fmt --all -- --check` 因大量既有文件格式差异失败，本波没有批量改动其它任务的文件。

## 未经人类验证与后续

- Windows 真机的设置弹窗观感、DPI / 多屏位置、颜色正确性均未由崔总确认；本记录不把浏览器冒烟当成真机 E2E。
- ICC 输入、浮点工作空间、导出标记、Windows 双路径显示管理、批量/预设/XMP 色彩字段和软打样属于 CM-W2～W5，继续按 `memory/PLAN.md` 实施。XMP 相关文件此刻另有并行开发，后续接入前须基于其最终 schema 核对，不覆盖在途改动。
