# 定稿确认后先出占位条目，预览位转圈

完成时间：2026-09-29 15:12:00 CST

## 需求（崔总 2026-09-29）

editor 点定稿弹窗确认后，定稿**立即**出现在定稿面板（占位），预览图位置显示旋转的
「处理中」图标；后端生成（缩略图 / preview / AVIF）完成后换成真图。旧况：确认后
「似乎无事发生」，几秒后新定稿才突然出现——特别离谱。

## 根因

`finalize()` 要 `await createIssue`（`issue_create` 里**同步**跑整套定稿渲染 + AVIF 编码，
数秒）之后才 `setIssueLibrary` 回填列表——那几秒列表毫无动静。

## 改动

| 文件 | 改了什么 |
| --- | --- |
| `src/workspaces/editor/EditorWorkspace.tsx` | 新增 `pendingIssue` 信号：确认后立刻 `setPendingIssue({name, sourceBase})` 并立刻 `setIssueFocusTick+1`（切到定稿页签看得见占位出现）；`finally` 清占位（成功=真实条目已进列表 / 失败=不能一直转圈）；换照片的 effect 一并清 |
| `src/features/editor/panels.tsx` | `EditorPanels`/`IssuesTab` 新增 `pending` prop；占位条目渲染在命名定稿列表**最顶**（列表按 `created_at` 倒序，接替时无跳变）：缩略图位 = `IconLoader2 animate-spin`，名称行 = 定稿名 + `SOURCE · 预览生成中…`；不可点选；空态文案在占位期间不显示 |
| `src/i18n/en-US.ts` / `zh-CN.ts` | 新键 `editor.issue.generating`（"Generating preview…" / 「预览生成中…」） |

## 验证（在干净 worktree 上——见下「并行会话」节）

- `pnpm typecheck` / `pnpm test`（1114 全过）/ `lint:i18n` / `pnpm build`：全绿。
- 基线 = HEAD `3844fb4` + 仅本改动四文件（并行会话的未提交内容已剔除后再验）。

## ⚠️ 并行会话碰撞（重要，写明处置）

提交时另一会话在同一工作区活跃施工（`specs/tiles-disk-read.md`、缩略图管线重做等
102 个未提交文件），且与本次改动**同文件碰撞**：`EditorWorkspace.tsx` / `panels.tsx`
被同时编辑（他们在把 `IssueThumb` 换成 `issueThumbs: ThumbQueue` + `issueThumbKey`）。
处置：

- 本次提交只包含**我的 hunk**（干净 worktree 重建 + `git update-index --cacheinfo`
  暂存），他们的未提交改动原样留在工作区，一个字没动；
- 主树 `pnpm typecheck` / `pnpm test` 当前的报错（`photo-grid/source.ts` 的
  `tileNatural`、`thumb-queue` / `photo-grid` 6 条失败）全部来自并行会话的半成品，
  与本次改动无关（干净基线上验证过）。

## 未由 Agent 声称验证的 E2E

- 真机点定稿：占位即时出现、转圈、几秒后换真图的完整观感；失败路径（后端报错）的
  占位回收与错误提示。

## 遗留问题

- `memory/` 规格（FUNCTION-IMAGING 的定稿段、editor 相关记忆）**暂未同步**：目标记忆
  文件同样处于并行会话的未提交修改中，为避免 hunk 级纠缠，等那边落地后再补一行
  「定稿面板占位先行」的口径；本记录先行留痕。
