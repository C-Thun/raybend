完成时间：2026-09-30 05:36:41 CST（Asia/Shanghai）

# 相片整理上期主体功能

## 改动范围与文件

- `design/browse.pen` 与 `design/browse.md`：库目录/相片桶/标签整栏三入口、桶卡片、标签树、共用自动规则弹窗。崔总在会话中确认界面稿后才进入实现。
- `crates/raybend/src/store/migrations/app_0009_photo_organization.sql`、`catalog_0014_photo_organization.sql`、`migration.rs`、`organization.rs`、`query.rs`：全局桶与目录/照片文字标签，持久照片 UID，复合库成员，规则 OR/AND，扫描补收与事件游标、崩溃重放、排除与事件日志清理。全部走现有迁移框架。
- `src-tauri/src/organization.rs` 与 `src-tauri/src/lib.rs`：Tauri 薄壳 IPC，库租约和批量 UID 解析，标签/桶查询、写入、规则预览、自动桶批次处理。
- `src/api/organization.ts`、`src/features/browse/{OrganizationPanel,BucketDialog,BucketPickerDialog,organization-source,organization-rules,organization-drag}*`、`src/workspaces/browse/BrowseWorkspace.tsx`、`src/App.tsx`：三入口左栏与共用照片浏览台，跨库复合照片键、按需元数据读取、标签树、桶加入/移出/拖入、规则草稿与筛选转换，复用原有标记、筛选、信息、查看与导出锚点交接。
- `src/features/commands/catalog.ts`、`src/i18n/{zh-CN,en-US}.ts`、`scripts/ui-smoke.mjs`：命令、双语文案和可重复的三入口/规则弹窗冒烟断言。

## 关键决定

- 目录标签只创建目录关联，绝不继承到目录内照片；照片标签由文字键直接索引。旧数字标签只有词典有名字时才同步，无法解析的不猜测、不删除。
- 切换左栏入口只换导航，保留当前照片来源；点击标签/桶/目录节点才换中列。标签父行只选择照片，箭头单独展开目录；按库聚合目录关联数避免每个标签一次查询。
- 桶在 app.db 保存库 ID + 照片 UID，整数资产 ID 只作 catalog 查询提示；查询时复核 UID，卸载库不删成员。同照片命中多个 OR 组只入一次，移出自动桶写排除；手动重新加入可解除排除。
- 规则字段结构让每类条件每组至多一次；组间 OR、组内 AND，零组是普通桶。最多 16 组，每组库范围和标签值各最多 64 个。仅选库的规则在弹窗明示“接收所选库全部照片”。筛选转规则不保留目录/标签/桶导航范围，暂态旗标等不支持字段明确拒绝转换。
- 自动桶初扫与增量事件每批 256 张；app 游标与成员同事务提交，catalog 事件只在游标提交后清理。部分库失败不隐藏其他库的照片/处理结果。
- `BrowseToolbar`、`PhotoViewingStage`、`PhotoGrid`、`TreeNode`、`ToggleBlock` 与色标/锁值表均复用已有实现。规则弹窗只用表单适配输入方式，筛选转换和 Rust 查询判定仍共用原有语义，不另造第二套匹配算法。
- 命令体系登记三入口 Alt+1/2/3；加入选中/旗标、从筛选建自动桶、移出选中照片均登记且默认热键留空（低频、已有可见入口）。桶锁/暂停/编辑/删除和目录标签是当前对象菜单动作，不设全局命令。

## 验证

- `cargo test -p raybend --lib`：1246 通过、6 忽略；含 UID 复用隔离、目录与照片标签不继承、规则边界、自动桶补扫/事件/排除/清理。
- `cargo check --workspace`：通过。
- `pnpm typecheck` 对应的本地 `tsc --noEmit`、`pnpm test` 对应的 Node 测试：123 个测试文件通过；颜色、分层、i18n 检查通过；Vite 构建通过。因仓库内直接运行 pnpm 会尝试下载锁定版本，改从 `/tmp` 通过 pnpm 调用仓内本地工具，未混用包管理器。
- `scripts/ui-smoke.mjs`：浏览器冒烟 `problems: []`；新增断言涵盖三入口互斥且不能全关、标签搜索、共用规则弹窗。
- Windows debug 交叉构建与 `scripts/check-win-artifact.mjs`：最终 exe、worker 与 `dist/` 资源核对通过。仓内直接运行 pnpm 的版本管理会等待网络，因此先用 pnpm 从 `/tmp` 完成前端构建，再调用仓内 `windowsBuildEnv` 交叉编译并运行既有产物核对脚本。未运行 release、打 tag 或推送。

## 遗留与验收边界

- 本记录完成时，跨库多选导出尚未衔接；同日后续已完成，见 `implementations/2026-09-30_photo-organization-export-handoff.md`。导出画廊仍按当前库显示，跨库选择由现有队列动作接收。
- Agent 仅完成编译、单元和 UI 冒烟；真实照片库、Windows 拖拽/目录树/规则弹窗视觉、离线重新挂载、DPI 与大库性能仍需崔总真机验收。人类反馈后 Agent 应复验并在同一记录补结论。
- 未实施 AI 推理和照片文字遮罩；它们属于 `specs/photo-organization-phase2.md`。
