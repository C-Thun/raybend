# 基础曲线辅色、两层重置与档案改名
完成时间：2026-09-26 16:55:56 CST

## 范围与行为

1. 基础曲线虚线改用辅色 `--brand-2`；保留原有虚线与透明度。Pencil 与实现同步。
2. “重置修改”按实际参数判断阶段，不以 dirty/rev 是否变化作为可用性判断。
   - 有额外修改：确认后恢复整套自动调整结果，保留当前基础曲线 ID 和点快照；手动曲线、LUT、裁切与旋转复原。确认中显示“不重置基础曲线，再次点击可重置基础曲线”。
   - 只剩自动调整或基础曲线：直接清除整套自动调整，基础曲线回到 NULL / 未选择档案。
   - 无调整：工具栏与统一命令均禁用。重置保留当前 RAW/SOOC 编辑来源。
3. 自动调整基线覆盖当前允许的参数、镜头选择/开关与降噪方式；只记录实际生成项，已有手动设置不被收编为自动结果。当前自动算法继续生成曝光、反差、饱和度与匹配镜头，未新增调色算法。
4. 基础曲线选择列表的档案行末增加铅笔按钮，复用 Dialog 输入名称。名称预填，取消/保存，保存中防重复提交；最长 80 个 Unicode 字符，禁止空名称与控制字符，首尾空白修剪。名称允许相同，档案始终按稳定 ID 区分。
5. 改名只更新 app.db 现有 name 字段，不改变机型、曲线、样本数、创建时间或照片引用；作为全局档案管理动作，不进入照片的调色撤销历史。
6. 界面只在第一层确认中显示本次明确要求的说明；没有新增常驻快捷操作教学。

## 存储、迁移与状态一致性

- `DevelopStack.auto_adjust` / IPC `autoAdjust` 保存自动结果基线；photo latest、issue 快照和撤销记录共用该字段。
- catalog v12 经现有迁移框架增加 `develop_stacks.auto_adjust TEXT`，启动/接入库时沿用版本闸门、备份和 MigrationGate。app.db 改名复用原字段，不另增 schema。
- 旧栈与旧定稿的可选基线为 NULL；旧 JSON、画面配置与定稿哈希保持兼容。迁移不推断未知来源。
- 自动来源不进入像素缓存签名与定稿匹配哈希；同一画面仅来源不同仍匹配已有定稿。issue JSON 保留来源，以便重新载入后执行两层重置。
- 基线本身即使没有当前像素调整也保留在数据库中，删除最后一个参数不会把来源一并丢掉。
- 两层重置统一走 `develop_commit`，移除已无调用的另一路全清重置 IPC。前端提交、定稿匹配草稿与离开照片时的保存使用同一快照构建函数。
- 切照片保存兜底在离开时立即捕获完整 profile，队列任务只提交该快照，避免延迟读到下一张的设置。
- 重置确认和改名弹窗绑定照片/编辑源上下文；切换后关闭，迟到请求不会更新新照片的列表或重置新照片。

## 复用与命令体系

- 弹窗、按钮、图标按钮、编辑参数契约、数据库写者、迁移框架与撤销 Op 均复用既有实现。
- 重置复用 `editor.develop.reset`，工具栏、命令面板与菜单共享 canReset。默认热键留空：第二层直接清除自动调整，避免误触。
- 改名属于档案条目的上下文动作，未注册缺少目标档案的全局命令或热键。
- Pencil 节点 `cpnMK`（两层重置与档案改名）、`AFy5y`（确认）、`Bkill`（改名）与列表行铅笔按钮；已检查截图与布局。

## 涉及文件

- `src/features/editor/CurveEditor.tsx`、`store.ts` / `store.test.ts`、`panels.tsx`、`toolbar.tsx`、`actions.ts`。
- `src/workspaces/editor/EditorWorkspace.tsx`、`src/App.tsx`、`src/features/commands/catalog.ts` / `catalog.test.ts`。
- `src/api/editor.ts` / `editor.test.ts`、`src/api/types.ts`、`src/i18n/zh-CN.ts` / `en-US.ts`。
- `crates/raybend/src/store/base_curve.rs`、`develop.rs`、`issues.rs`、`migration.rs`、`migrations/catalog_0012_auto_adjust.sql`。
- `src-tauri/src/base_curve.rs`、`develop.rs`、`lib.rs`。
- `DESIGN.md`、`design/editor.pen` / `editor.md`。
- 附带局部启动修正：`src/workspaces/export/ExportWorkspace.tsx`，将依赖 base 的 assets memo 移到 base 声明后，消除冒烟发现的初始化前访问错误；导出业务保持原实现。

## 已验证（冒烟与单元）

- `pnpm typecheck`：通过。
- `pnpm test`：1006 通过。
- 编辑 store / 命令 / API 相关独立回归：30 通过。
- `pnpm lint:colors`、`lint:arch`、`lint:i18n`：通过。
- `cargo test -p raybend --lib`：1138 通过，4 ignored；包含改名边界、自动基线往返/校验、来源独立于像素签名、旧 JSON、定稿匹配与 v11→v12 迁移。
- `cargo test -p raybend-desktop --lib`：90 通过，1 ignored；包含实际数据库中两层重置的撤销、重做与 DTO 往返。
- `CDP_PORT=9396 pnpm smoke:ui http://localhost:1420/dev/kitchen-sink`：通过，problems 为空。独立 CDP 端口避免与其他冒烟进程互相干扰。
- `pnpm debug:win`：前端生产构建、Windows 主程序/RAW worker 构建与 `check:win` 核验通过。并行构建曾替换 dist 或改动 Rust 源码，按现有脚本重新同步后通过。
- 最终主程序：`C:\rb-target\raybend\debug\raybend-desktop.exe`，2026-09-26 16:53:51 CST。
- RAW worker：2026-09-26 16:52:57 CST，包含 `raybend-worker-proto-v3`；内嵌前端 4 个资源全部命中。
- 修改文件的 `git diff --check`：通过。未提交或推送。

## 实际限制与待目视确认

- 旧照片/旧定稿此前没有保存自动调整的原始参数，无法可靠区分既有参数哪些是自动、哪些是手动。旧配置完整保留；重新点击一次自动调整后建立可恢复基线。来源未知时，第一层恢复参数默认值并保留基础曲线。
- 尚未经 Windows 真机 GUI E2E：虚线辅色观感、两层点击体验、改名与切换照片、真实照片的重开/切稿/撤销画面同步，仍由崔总目视确认。浏览器冒烟不代表视觉或色彩正确性已验证。
