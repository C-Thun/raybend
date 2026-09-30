完成时间：2026-09-30 03:33:35 +0800

# 最近更新质量检查与修复

## 检查范围

按提交时间检查 2026-09-29 15:22 至 2026-09-30 03:22（北京时间）的五次提交（`4900b60`、`c65f0a7`、`ce511f8`、`2b383ec`、`a94a4bf`），并检查同一时段工作区尚未提交的编辑预设实现。重点核对定稿异步回填、迁移状态轮询与遮罩、外部应用选择、预设存储及拖拽；现有其它工作区改动保留。

## 修复

- `src/workspaces/editor/EditorWorkspace.tsx`：定稿生成期间暂停旧的延时定稿列表查询，完成后按当前照片重新读取；切照片立即清掉旧列表，旧照片任务失败不把错误显示到新照片上。此前旧查询可能晚于新定稿写入并覆盖列表。
- `src/features/migration/monitor.ts`、`monitor.test.ts`：把快照适配器的同步异常转成 Promise 拒绝，让降级轮询的错误处理能接住并继续下一轮；新增同步异常后恢复的回归测试。
- `src/features/external-editor/ExternalEditorDialog.tsx`：应用网格改为按钮组，保留按钮的无障碍角色；此前按钮上的 `role=listitem` 覆盖了按钮语义。
- `src/features/editor/preset-panel.tsx`：拖拽时支持 `Escape` 取消；屏蔽投放后由指针捕获生成的 click，避免多选移动完成后被误改成单选；卸载时清理正在进行的拖拽。
- `crates/raybend/src/store/presets.rs`：即使其它目录存在也确保默认目录能补回；名称重复检查覆盖 Unicode 大小写（SQLite `NOCASE` 只覆盖 ASCII），并补测试。

预设是面板内交互，未新增命令或默认热键；其余修复均是现有功能的正确性调整，也无新增命令。

## 验证

已验证（冒烟与单元）：

- `cargo test --workspace`：通过（修复前基线）；`cargo test -p raybend store::presets --lib`：新增边界测试通过。
- `node --test "src/**/*.test.ts"`：119 个测试文件通过；迁移轮询的新增回归测试单独通过。
- `./node_modules/.bin/tsc --noEmit`、`node scripts/check-hardcoded-colors.mjs`、`node scripts/check-architecture.mjs`、`node scripts/check-i18n.mjs`、`./node_modules/.bin/vite build`：通过。构建仍提示既有的 500 kB 大 chunk。
- 发布脚本测试 10 组中，9 组在沙箱通过；`check-win-artifact.test.mjs` 在沙箱中受 `spawnSync EPERM` 影响，使用沙箱外的同一测试命令通过。
- `git diff --check`：通过。

`pnpm` 本机启动器在当前受限网络下报 `ERR_PNPM_PNPM_ENGINE_IDENTITY_UNVERIFIABLE`；前端质量门使用仓库已安装的工具直接执行，未改包管理配置或锁文件。`cargo fmt --check` 对仓库已有的大范围未格式化代码报差异，本次未全仓重排。

## 遗留与人工确认

GUI 交互、拖拽的实际指针行为、Windows 真机和真实照片库效果仍需崔总在产品环境确认。编辑预设与照片整理工作区文件仍有并行中的未提交改动，本次检查没有替它们做完整功能验收。
