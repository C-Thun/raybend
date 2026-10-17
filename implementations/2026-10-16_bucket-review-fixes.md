# 相片桶审查修复：删桶回退 + 新建并加入原子化（附 FUTURE 登记）

完成时间：2026-10-16 03:44:44 CST

## 背景

2026-10-15/16 会话对 bucket / tag / AI 打标全链路做了一次只读审查（报告在对话中），崔总拍板：

- 第 4 项（桶改名入口缺失、`organization_bucket_rename` 死命令）与桶时间线全量物化的性能优化 → 记入 `memory/FUTURE.md` 的 after release 板块（当前 pre-release 试发期不动）。
- 第 1、2 项立即修复（本文件）。

## 改动范围

### 1. 删除「正在查看的桶」后回退并提示（审查第 1 项，规格 §9 欠账）

- `src/features/browse/OrganizationPanel.tsx`：新增 `onBucketDeleted?: (bucket) => void` prop；`confirmDelete` 成功后若被删桶正是当前选中项则触发（面板自己不知道回退策略，只报事实）。
- `src/workspaces/browse/BrowseWorkspace.tsx`：接住该回调——`setOrganizationSelection(null)`（`organizationPhotos()` 变 false，中列回到库目录浏览；`onOrganizationBucketSelection` 联动让 App 的「从桶里移出」按钮消失），并 toast（info）提示带桶名。
- i18n：新增 `org.bucketDeletedFallback`（zh/en 各一）。

### 2. 「新建桶并加入」原子化（审查第 2 项，规格 §7 欠账）

原来 `BucketPickerDialog` 是 `createPhotoBucket` + `addPhotosToBucket` 两次 IPC，第二步失败会留下空桶。现在：

- `crates/raybend/src/store/organization.rs`：`create_bucket` / `add_members` 拆出无事务内部函数 `insert_bucket_tx` / `add_members_tx`（原函数改为包一层事务调用，行为不变）；新增 `create_bucket_with_members`——校验（名称、规则、引用）全部先于事务，然后单事务里插桶 + 补扫行 + 成员，任一失败整单回滚。
- `src-tauri/src/organization.rs`：`organization_bucket_create` 命令签名扩展为 `photos: Option<Vec<AssetIdentity>>`，返回 `CreateBucketOutcome { bucket, added }`；refs 解析（`resolve_refs`）留在 `blocking` 内（与 `organization_bucket_add` 同口径，不占异步执行器）。
- `src/api/organization.ts`：`createPhotoBucket(name, rules?, photos?)` 返回 `{ bucket, added }`。
- `src/features/browse/BucketPickerDialog.tsx`：新桶路径改为**一次**调用，toast 计数用实际新增的 `added`。
- 旧调用点 `App.tsx:1075`（BucketDialog 纯创建）不用返回值，无需改动。

### 3. FUTURE 登记（不涉代码）

`memory/FUTURE.md` after release 板块新增一条，含两个子项：桶改名入口（死命令、改名触发全量重扫）与桶时间线（`query::timeline` 按时间模式取数链路）全量物化的优化方向，各带落点文件。

## 关键决策与理由

- 删桶回退的「回退到哪」= 库目录浏览的现状（不动 browse store 的库/目录），左栏面板仍停在桶面板——面板选择与照片来源分离是规格 §2.1 的定案，不因为删桶而替用户切面板。
- `create_bucket_with_members` 复用拆出的内部函数而不是另写一份 SQL（§2.12 复用纪律）；校验前置保证「失败不留空桶」覆盖非法名称/重名/非法引用三类。
- 命令体系（§2.15）评估：两个修复都是既有命令/入口的行为修正，不产生新的用户可触发动作，无需登记 `catalog.ts`。

## 验证

- `cargo test --workspace`：1574 passed / 0 failed（含新增 `store::organization::tests::create_with_members_is_atomic_and_idempotent`：正常创建、无效引用整单回滚、折叠重名整单回滚）。
- `pnpm typecheck`、`pnpm test`（1316）、`pnpm lint:i18n`、`pnpm lint:colors`、`pnpm lint:arch` 全部通过。
- 未经人类验证：删除当前桶的回退与 toast、picker 新建并加入的真机交互（Windows E2E 归崔总）。

## 遗留

- 审查报告第 3、5–15 项未动（待崔总逐条拍板）；第 4 项与时间线优化在 `memory/FUTURE.md` after release。
