# M2 欠账清单（2026-09-21 收口核验后重写）

> 原版（2026-09-19）是 PhotoGrid 合并暂停时的待办真源。2026-09-21 M2 收口时逐条对过代码，
> **已消化的段落按本文件纪律删掉**，这里只留真正未了的。原版的完成证据见文末对照表。

---

## 1. 还开着的欠账

### 1.1 左列目录树仍是两份（原 §4.2，未做）

* `workspaces/import/LeftColumn.tsx`（SplitStack：最近 / 已选目录 / 来源树，`features/dir-tree/DirTree` 渲染行）
  vs `features/browse/BrowsePanels.tsx` 内联目录树行（选中 scope、行尾 `⋯` 菜单、无勾选圈）。
* 共享的部分已经下沉：库卡片是同一份 `components/ui/RepositoryCard.tsx`；行原语 `components/ui/TreeNode.tsx`
  两侧却都没全用（browse 的行要 `⋯` 菜单与不同结构，自己写的 div）。
* **为什么还开着**：两侧交互模型真不同（勾选 vs 选中 + 行内菜单），硬并一份有 §2 教训的风险
  （PhotoGrid 合并曾中途污染）。动它之前先写清「一份行组件 + 两侧适配」的方案再动手。

### 1.2 film 胶片带的像素级锚定（原 §4.5，按计划等人类）

* 现状只保证「当前那张在视野里」；横向锚定策略等**网格锚定经人类真机确认后**再定。

### 1.3 标签保存链路的真机验证（原 §4.6，归人类）

* Rust 侧 `asset_tags` 路径有单测；**真机**「打标签 → 保存 → 关软件重开还在」没验过。
  已并入 M2 收口的人类验收清单（`plans/HANDOFF-2026-09-20.md` §3.1 同批）。

---

## 2. 原版各段的消化对照（证据）

| 原段落 | 结论 | 证据 |
| --- | --- | --- |
| §2 PhotoGrid 合并（BrowseGrid → 删除） | ✅ 已完成 | `src/` 下无 `BrowseGrid.tsx`；唯一 `features/photo-grid/PhotoGrid.tsx`，import/browse 两侧经 `TilesSource` 适配接入（`AGENTS.md` §11.4） |
| §3 浏览右栏（分区 + 可编辑字段） | ✅ 前端已完成 | `BrowsePanels.tsx`：`EditableField`（作者/描述/地理四项，走 `store.setText` 进撤销栈）、创建日期 `createdMs`、所属库名、view 模式预览+视口框在直方图前 |
| §3 的「EXIF Artist 导入时自动带出」 | ↪ 转为远期意图 | `marking.rs`：`author` 注释「将来支持某库导入时自动填」；登记在 `FUTURE.md` 的库级默认 author 意图，不在 M2 |
| §4.1 库卡片两份 | ✅ 已统一 | `components/ui/RepositoryCard.tsx` 一份，`RepositoryList` 与 `BrowsePanels` 两侧引用 |
| §4.3 工具条两份 | ✅ 已按插槽收敛 | `shell/ToolsBar.tsx`（条带 + 居中）+ `flow.ts` 简单工具目录 + children 槽（浏览标记系列从 `BrowseToolbar` 进槽）——一份条带，各流控件即配置 |
| §4.4 `repository_rebuild` 进度事件 | ✅ 已做 | `src-tauri/src/repo.rs`：`RebuildProgressDto` + `emit_progress`（阶段/done/total） |
| §5 提醒（右列宽度常量、三条目视项） | ✅ 已了结/移交 | 右列宽度仍为常量（人类未提可拖需求，不再挂账）；三条目视项并入 2026-09-20 换手文档 §3.1 的真机验收清单 |
