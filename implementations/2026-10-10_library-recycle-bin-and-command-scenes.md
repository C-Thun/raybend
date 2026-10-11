# 库级回收站 + 快捷键场景化（实施记录）

2026-10-10 22:49:11 完成。

规格：`specs/library-recycle-bin.md`、`specs/command-scenes.md`；画稿：`design/browse.pen`
（`Shell / Browse / Recycle`、`Shell / Browse / Tiles · 拖动删除`、`Components / 回收站入口两态`、
`States / 回收站 · 粉碎与清空` 四帧 + 命令面板/快捷键设置两处场景列）。

---

## 一、库级回收站

### 改动范围

**Rust 存储层**

- `crates/raybend/src/store/migrations/catalog_0018_recycle.sql`（新）：`assets.deleted_at`、
  `asset_files.recycled_from`、局部索引 `idx_assets_deleted_at`。**只加这两列与一个索引** ——
  数据层「只是标记」的落实。
- `crates/raybend/src/store/recycle.rs`（新，替代 `delete.rs`）：`move_to_recycle_checked` /
  `restore_assets_checked` / `purge_assets_checked` / `empty_recycle_checked` / `summary`；
  `_RAW/` 与 sidecar 伴随、还原冲突 `_01` 改名（bitmap/RAW 同步）、空目录清理、
  移动失败回滚、`check_session` 闸门。`delete.rs` 与 `SystemTrash` 删除。
- `crates/raybend/src/store/query.rs`：`Visibility { Live, Recycled }`（在 `where_clause` 开头
  无条件生成 —— 唯一收口）、`SortKey::DeletedAt`、`AssetRow.recycled_from`（ROW_COLUMNS 末尾
  子查询，不动既有列索引）。
- `crates/raybend/src/xmp/{mod,path}.rs`：注释引用 `store::delete` → `store::recycle`。

**命令层（src-tauri）**

- `browse_delete` 改为移入回收站；新增 `browse_recycle_restore / purge / empty / summary`；
  `BrowseQueryDto.recycle` 标志 + `deletedAt` 排序键。
- 契约同步：`contract.rs`、`src/api/dto-contract.json`、`dto-contract.test.ts`、`types.ts`
  （`RecycleResult` / `RestoreResult` / `PurgeResult` / `RecycleSummaryView`；`AssetItem.recycledFrom`）。
- `organization.rs` 的三处 `Query` 字面量补 `visibility`。

**前端**

- `src/api/browse.ts`：`browseDelete`（RecycleResult）+ 四个回收站函数。
- `src/features/browse/store.ts`：回收站模式（`enterRecycle` / `exitRecycle` / `openPhotosScope` /
  `removeIds` / `purgeIds` / `restoreSelected` / `emptyRecycle` / `recycleSummary`）；
  回收站视图固定「删除时间倒序」、不筛选；换库退出回收站。
- `src/features/browse/BrowsePanels.tsx`：左列回收站入口（有内容才出现、红垃圾桶 + 数量、
  可展开回收站树、与 photos 树分隔）+ 右列「原位置」。
- `src/workspaces/browse/BrowseWorkspace.tsx`：工具条切换（回收站里只有 `[还原选中的照片] │ [清空回收站]`）、
  tiles 低浓辅色底、FilterBar 隐藏、清空强确认弹窗、失败清单复用、拖动接入。
- `src/App.tsx`：回收站工具条分支、CommandDeps 注入、命令白名单随命令拆分更新。
- `src/features/browse/photo-drag.ts`（新）：**pointer 统一拖动** —— 热区命中、桶卡片命中
  （`[data-photo-drop-bucket]`）、卡片堆叠预览、`dragIdsFor` / `pointInRect` 纯函数；
  `photo-drag.test.ts`（新）。`organization-drag.ts`（HTML5 通道）删除。
- `PhotoGrid` / `PhotoViewingStage`：`onPhotoDragStart`（HTML5）→ `onPhotoPointerDown`（pointer）+
  `draggingIds` 半透明。
- `src/styles/tokens.css`：`--tint-recycle`（深 0.08 / 浅 0.10）与 `--state-recycle` + `@theme` 映射。
- i18n：回收站全套文案（zh/en）+ `cmd.browse.delete`（替换 `cmd.edit.delete`）。

### 关键决策与理由

- **数据层「只是标记」的准确边界**：`rel_path` 前缀改写 + `recycled_from` 记录原路径 +
  `deleted_at`；不变量 =「rel_path 永远描述文件真实位置」。评级/标签/编辑栈/issue/桶成员
  **一律不动** → 还原后一切照旧。
- **文件先走、记录后走**：逐资产移动 + 单资产事务；任一文件移动失败 → 回滚已移动的、不改 DB。
- **可见性收口**在 `where_clause` 开头：新增查询自动带上，不靠逐处记忆；rebuild 重扫经核对安全
  （它按文件存在性兜底确认 missing，回收站文件在 `recycle/` 下真实存在，不会误标）。
- **拖动统一 pointer**：删掉 HTML5 通道（两套会抢同一次 pointerdown）；预览与热区实时高亮得以实现，
  与 export 的拖动同源（`lib/pointer-drag`）。
- **真删只有拖动一条路**（崔总定）：无按钮、无快捷键、不进命令注册表；清空必须强确认。

### 验证（Agent 侧）

- `cargo test --workspace`：**1476 + 132 + 7 + 2 + 2 全过**（`store::recycle` 10 条新测试：
  移入（含 `_RAW`/sidecar/关联保留/可见性）、锁阻挡、文件缺失仍标记、幂等、还原、冲突改名、
  真删（含 cache 目录）、不碰 live、清空（含未知残留）、概况）。
- `pnpm typecheck && pnpm test`：**1264 条全过**；`pnpm lint:colors / lint:arch / lint:i18n` 全绿；
  `pnpm build` 成功。
- `pnpm smoke:ui`（后台 `pnpm dev`）：`problems: []`。
- 契约测试（Rust + TS 两侧）通过。

---

## 二、快捷键场景化

### 改动范围

- `src/lib/commands.ts`：`CommandScene` + `ALL_SCENES` + `scenesOf` / `scenesOverlap`；
  `detectConflicts` 的阻断判据改为 **场景相交 ∧ 作用域相交**。
- `src/features/commands/catalog.ts`：`spec()` 强制「有默认键位的命令必须显式标场景」（当场 throw）；
  无键命令按 id 前缀推断（editor→edit、export→export、import→import、
  browse/recycle/ai/organization/nav/mark→browse、其余→全场景）；
  30+ 条有键命令显式标注；`edit.delete` 拆为 `browse.delete`（browse）与 `export.remove`（export），
  两者同绑 `Delete` 且并存。
- UI：`CommandPalette`（场景列）、`ShortcutSettingsDialog`（场景列 + 场景说明）、
  `messages.ts::sceneText`；i18n（`scene.all`、`shortcuts.scenesNote`、`sharedNote` 文案）。
- 测试：`src/features/commands/scenes.test.ts`（跨场景放行 / 同场景仍拦 / 作用域不相交放行 /
  真实注册表体检：有键命令必须标场景、`Delete` 双命令零阻断冲突）。

### 验证

同上一节的质量门（测试含新增 4 条场景用例）。

---

## 三、文档同步（已完成）

- `AGENTS.md`：§11.3 术语表（移除/排除/删除/还原/真删/清空 + 确认规则）、§6.4 库结构、
  §6.5 缓存清理口径。
- `memory/DESIGN.md` §12.11 #4（删除确认的新口径）。
- `memory/FUNCTION-BROWSE.md` §3.2（工具条）、§5.9（删除表格）、§10 问题 4（修订记录）。
- `memory/FUNCTION-REPOSITORY.md` §1（`recycle/` 保留目录）。
- `memory/FUTURE.md`：#10 结项 + 时间线一行。
- `specs/xmp-sidecar.md` §3.1.1（库内移动的 sidecar 伴随）；`specs/xmp-write-toggle.md` §2.2。
- `todos/2026-10-10-library-recycle-bin.md` 归档删除。

## 四、遗留（归崔总 / 后续）

- **真机 E2E（崔总）**：拖动热区手感与命中、批量 1000+ 的耗时体感、回收站视觉（低浓辅色底）、
  还原后各视图一致性、Windows 文件占用场景；Windows 侧构建（`pnpm debug:win` / `check:win`）。
- 拖动开始时自动切到相片桶面板的便利未接（`beginPhotoDrag` 预留了 `onStart`）。
- 真删无键盘入口（按崔总定的减法；若要补，单独评审）。
- `store::assets::counts` 仍含回收站记录（界面照片数不读它，无用户可见影响）。
