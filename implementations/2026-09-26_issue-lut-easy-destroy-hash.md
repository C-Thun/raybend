# 定稿 / LUT 删除保护与 LUT 内容去重
完成时间：2026-09-26 14:10:35 CST

## 范围与结果

用户要求两类删除都使用确认框和 easy destroy，并加入 LUT 文件哈希防重复导入。本次已实施、通过冒烟并重建 Windows debug；真机交互与视觉正确性仍待人类确认。

- 定稿删除：普通点击先确认，Shift 点击直接执行；沿用已有 `createEasyDestroy` / `EasyDestroyHost`。删除目标绑定点击时的库、照片和 issue，切库 / 切照片取消未确认请求；异步响应也只更新原上下文。删除配置快照及其独立缩略图 / 预览，当前 latest 与原照片保留。
- LUT 移除：普通点击先确认，Shift 点击直接执行；复用 `EasyDestroyButton`。确认框明确说明应用内文件保留，已有照片 / 定稿不受影响。文案由“删除”调整为“移除”，使用现有禁行图标。
- 共用确认框支持可见说明，Shift 快通道提示由原先隐藏文本改为可见正文；取消 / Esc / 遮罩关闭沿用通用 Dialog，不执行待确认动作。
- **现在及改动之前，LUT 移除均不删除任何 LUT 文件**：仅设置 `hidden=1`。私有 LUT、封面、封面原图和外部导入原件保留。未提供垃圾回收 / 硬删除，也不扫描其它 catalog 判断是否还被引用。
- 图库响应保留 hidden 条目的文件可用性；同一数据源经纯适配过滤后展示可见条目 / 计数。移除当前使用的 LUT 后不会被误判为文件缺失，已有引用仍能解析原 ID。

## LUT 文件身份与导入规则

1. `app.db` v7 通过既有迁移框架加入可空 `luts.file_hash` 与索引；只接受 64 位小写十六进制 SHA-256。没有启动临时 ALTER、另开迁移通道或清库。
2. 按文件原始字节计算 SHA-256，文件名、原路径、分类不参与身份；同内容改名 / 跨分类导入跳过。只有文件字节相同时判重，注释 / 换行不同可能创建不同条目，本次不做颜色语义等价合并。
3. 旧记录读取图库 / 导入时，在 DB 写线程之外从私有文件补录哈希，再事务写回，仅更新 NULL。旧重复记录保留 ID；索引不设 UNIQUE，避免静默破坏既有 issue / latest 引用。命中多个旧条目时优先可见，再按创建时间 / ID 稳定选择。
4. 移除后的同内容文件重导恢复原 ID，归入本次选中的分类；保留原名称、路径字段、预览和照片引用。已有可见同内容条目保留原分类。
5. 先做哈希快筛，正常重复无需解析 / 烘焙封面；最终仍在 IMMEDIATE 事务中判重 / 恢复，防止并行导入产生第二个条目。
6. 新导入按实际写入私有目录的字节快照计算哈希并解析；复用现有原子写入，文件上限 128 MiB，哈希流式读取。同名 CUBE 伴生封面识别移到已有核心导入模块，扩展名不区分大小写。
7. 对已有哈希的记录，私有副本丢失 / 损坏后重导相同内容会修复原 ID 的文件；缺失封面也可重建。新建但未入库的候选目录会清理，不删除已存在记录的文件。
8. 面板显示本轮导入 / 恢复 / 重复跳过 / 失败数量；错误仍走现有编辑器错误提示。

### 边界与限制

- 旧记录的私有文件已经丢失，且尚未补过哈希时，保持 NULL。仅凭原文件名不能安全恢复其内容身份；重导无法据此确认旧 ID，已有丢失引用继续保留并提示缺失。
- 已移除条目文件持续占用应用数据目录；这是保护历史引用的既有行为。本次不承诺释放磁盘空间。
- 定稿删除是配置快照删除，使用 Shift 快通道是用户本轮明确要求；不扩展到照片回收站或空目录删除。

## 复用、设计与命令

- 删除沿用现有 easy destroy / ConfirmDialog，没有新建第二套确认控制器或弹窗。
- Pencil 已连接，沿用 editor.pen 现有弹窗与令牌，新增 `Components / Editor / 删除保护`（`aPhpa`）的两种确认态；应用开关下方新增导入结果（`pMsjq`）。已检查截图与布局边界，同名 editor.md 同步。
- 复用现有 LUT 核心加载、封面烘焙、原子文件写入和数据库读池 / 单写者；新增直接使用的 sha2 0.10.9 已是锁文件中的间接依赖，许可登记 MIT OR Apache-2.0。
- 命令体系评估：本次是列表条目的上下文删除 / 移除，不适合单独登记全局动作；未新增全局热键，避免与已有照片删除命令冲突。Shift 是既有 easy destroy 的事件修饰规则，确认 / 取消沿用弹窗键盘交互。

## 涉及文件

- `src/components/ui/Dialog.tsx`、`EasyDestroy.tsx`：复用确认框说明与动作文案。
- `src/features/editor/lut-panel.tsx`、`panels.tsx`、`src/workspaces/editor/EditorWorkspace.tsx`：两处删除接入、捕获 issue 上下文、导入统计。
- `src/lib/lut-library.ts`、同名测试：隐藏条目展示适配与引用可用性保留。
- `src/api/lut.ts`、`src/i18n/zh-CN.ts`、`en-US.ts`：哈希 / 导入结果契约、中英文文案。
- `crates/raybend/src/develop/lut_import.rs`：流式 SHA-256、原子字节快照、伴生文件检测与边界测试。
- `crates/raybend/src/store/luts.rs`：统一记录映射、哈希检索 / 补录、事务去重 / 恢复与并发测试。
- `crates/raybend/src/store/migrations/app_0007_lut_hash.sql`、`store/migration.rs`：v7 schema、旧库升级回归。
- `src-tauri/src/lut.rs`：IPC 编排、快筛、候选清理与丢失副本修复。
- `crates/raybend/Cargo.toml`、`Cargo.lock`、`THIRD-PARTY-NOTICES.md`：直接依赖与许可。
- `design/editor.pen` / `editor.md`、`DESIGN.md`、`IMAGING.md`：设计、删除语义与存储规则。

## 已验证：冒烟 / 单元测试

- `pnpm typecheck`：通过。
- `pnpm test`：958 通过，0 失败；包括既有 easy destroy 的普通点击 / Shift / 取消 / 重复请求与执行错误回归，以及新增隐藏 LUT 适配测试。
- `pnpm lint:colors`、`lint:arch`、`lint:i18n`：通过。
- `cargo test -p raybend -p raybend-desktop --lib`：核心 1103 通过、4 ignored；桌面壳 87 通过、1 ignored。核心实际测试耗时 9.50 秒。
- LUT 相关测试：29 通过、2 ignored（过滤包含相关核心 / 文件 / 迁移回归），耗时 0.15 秒。覆盖 SHA-256 标准向量、空输入 / 大小边界、字节变化、中文文件名、拷贝后来源变化、缺失 / 损坏副本修复、大小写伴生文件、跨分类重复、隐藏恢复、旧重复 ID、补录幂等、非法哈希、外键失败回滚、4 个独立 SQLite 连接并行导入。
- Windows 原生 `cargo test -p raybend --lib lut`：29 通过、2 ignored，耗时 0.15 秒；构建环境复用 `scripts/lib/dav1d-win.mjs`。
- `pnpm smoke:ui http://127.0.0.1:1420/dev/kitchen-sink`：完成，`problems: []`。临时 Vite 服务已关闭。这是组件运行时冒烟，不是产品真机 E2E。
- `pnpm debug:win`：成功，含前端生产构建与 `pnpm check:win`；嵌入的 4 个资源全部匹配，RAW worker 含当前 `raybend-worker-proto-v3`。exe 更新时间 **2026-09-26 14:07:11 CST**。
- `git diff --check`：本次相关文本文件通过。

Windows 调试产物：`C:\rb-target\raybend\debug\raybend-desktop.exe`。

未直接操作真实 AppData 文件或 catalog；升级 / 补录交由新版本正常启动与图库读取执行。

## 待人类验证

在 Windows 调试版确认两处普通点击 / 取消 / Shift 快通道、切照片时未确认请求取消、删除当前定稿保留当前编辑；测试同目录重导、文件改名、跨分类重导、移除后重导恢复，以及旧定稿使用已移除 LUT 的显示效果。实际界面反馈与真机色彩正确性不作为 Agent 已完成验证。
