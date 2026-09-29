# browse tiles 右栏切 issue 即时化（不被全尺寸渲染卡住）

完成时间：2026-09-29 15:34:00 CST

## 需求（崔总 2026-09-29）

「在 browse tiles 模式下的 right 中切换 issue，为什么要等？进 film/view 要等可以理解，
因为要载入原图，但 tiles 下不是只需要载入 thumb 图片不就行了？」

## 根因

`display-variants.ts::select()` 把两步 await **串在挂选择之前**：

1. `export_snapshots` → `raybend::export::snapshot`——纯元数据（DB 读 + 文件签名），
   毫秒级；
2. `export_variant_details` → `export_variant_details`（src-tauri）里调
   `raybend::export::render_captured(...)`——**全尺寸渲染**一次，只为拿宽高 + 直方图，
   秒级。

两步都完成才 `setChoices` → 网格 tile 的 `imageKey` 才换 → 缩略图请求才开始发。
也就是说：tiles 明明只要 thumb，却被「为了直方图/宽高的全尺寸渲染」挡在前面。

## 改法（全部在 `src/features/browse/display-variants.ts`，未触碰并行会话施工中的文件）

- `select()` 在 **snapshot 返回后立即挂上选择**（毫秒级）：tile 立刻换 imageKey、
  缩略图请求即刻发出；`select` 的 Promise 也在这一步就 resolve（`issueBusy` 窗口
  收窄到毫秒级，选择器立刻可再点）。
- `export_variant_details`（全尺寸渲染）**退到后台**补齐：回来后再更新
  `natural`（变体宽高，裁切过的定稿会与基准不同）与 `histogram`。
- **details 失败不回滚**：缩略图已经能用，回滚等于抹掉用户刚做的选择；空直方图
  与基准 aspect 保留为退化信号；真正的渲染失败会顺着缩略图/大图路径浮出来。
- `DisplayChoice.natural` 改为可选（缺省 = aspect 退回基准照片）；
  `histogram` 保持必填但缺省是**空直方图**（`bins:0`）—— 不能传 `undefined`：
  `HistogramPanel.countsOverride !== undefined` 的语义会把缺省落回**基准照片**的
  直方图（按路径取数），切了变体却显示原图数据，比空态更骗人。
- `adapt().ensureNatural` 同步调整：只有 details 已到的选择才跳过基准宽高补读。

## view / film 为什么不受影响

大图走 `'screen'` 装载路径（`loadDisplay(key,'screen')`，key 里带 snapshot），与
details 无关；该等还是等，等的是它自己的原图。

## 验证

- `display-variants.test.ts`：既有 3 条契约（latest/上下文重置不复活、迟到丢弃、
  快照失败保留原选择）**原样通过**；新增 2 条：
  - snapshot 即挂、details 后补（aspect 1.5 → 0.5、直方图空 → 真）；
  - details 失败不回滚、直方图保持空。
- 干净 worktree（`4900b60` + 仅本两文件）：`pnpm typecheck` / 定向测试 5 过 /
  `pnpm build` 全绿。

## ⚠️ 并行会话说明

与 `implementations/2026-09-29_finalize-pending-placeholder.md` 同期：另一会话在
browse/editor 的多个文件（`BrowseWorkspace.tsx`、`BrowsePanels.tsx`、
`grid-source.ts` 等）有未提交施工。本次改动刻意**只落在干净的
`display-variants.ts` / `.test.ts`** 两个文件里，不需要动那些文件；主树因并行
半成品报的 `tileNatural` / `thumb-queue` 错误与本次无关。

## 未由 Agent 声称验证的 E2E

- 真机：切 issue 后 tile 换图的即时感；缩略图本身仍要渲染时的占位观感；
  裁切定稿的 aspect 二次校正是否可察觉。

## 遗留问题

- 无（`memory/` 规格落点同前一条记录：等并行会话落地后一并补）。
