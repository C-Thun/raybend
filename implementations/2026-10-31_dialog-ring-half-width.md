# 弹窗辅色边界线减半（0.5px）

> 2026-10-31 15:02:39 ｜ 崔总：辅色边「粗细减到一半，淡淡一圈就行」

## 改动范围

- 弹窗内描边宽度 **1px → 0.5px**；颜色不改（仍是 `--brand-2`）。
- 新增宽度令牌 `--dialog-ring-w: 0.5px`（`src/styles/tokens.css`），
  `DialogFrame` 与 `MigrationGate` 统一引用 `inset-ring-(length:--dialog-ring-w) inset-ring-(--dialog-ring)`，
  以后调粗细只动令牌。
- 同步文档：`memory/DESIGN.md`（§10.1 #9、令牌表、变更记录）、
  `specs/dialog-stack-and-border.md`、`design/main.md`、`AGENTS.md`、`memory/PLAN.md` §5 #11。
- 设计稿：`design/main.pen` 的 32 个弹窗帧 `strokeWidth` 1 → **0.5**。

## 涉及文件

- `src/styles/tokens.css`
- `src/components/ui/Dialog.tsx`
- `src/features/migration/MigrationGate.tsx`
- `design/main.pen`、`design/main.md`
- `memory/DESIGN.md`、`memory/PLAN.md`、`specs/dialog-stack-and-border.md`、`AGENTS.md`

## 关键决策与理由

- **用 0.5px 宽度而不是降低透明度**：崔总说的就是「粗细减到一半」。
  `inset-ring` 走 `box-shadow`，非整数 DPR（本机 1.375）下 0.5px 会被抗锯齿摊成半透明，
  自然得到「淡淡一圈」，不必再叠一层颜色 alpha（否则浅色主题本来只有约 1.9:1，会更看不清）。
- **宽度进令牌**：颜色早已是令牌，宽度也收进同一个位置，避免以后在组件里散落改数值。
- **Pencil 只改有描边的帧**：`main.pen` 的 32 个弹窗帧（14 个 `Dialog /` + 10 个
  `Global Settings /` + 8 个库设置/导入进度状态变体）从 1 改为 0.5；
  `browse.pen` / `editor.pen` / `export.pen` 的弹窗帧本来就没有描边（PLAN §5 #11 的既有欠账），
  本次不新增，保持「减半」这一件事的范围。

## 验证

- `pnpm typecheck` 通过。
- `pnpm test`：1346 tests，0 fail。
- `pnpm lint:colors` / `pnpm lint:arch` / `pnpm lint:i18n` 通过。
- `pnpm build` 通过；产物 CSS 确认生成
  `.inset-ring-\(length\:--dialog-ring-w\){--tw-inset-ring-shadow:inset 0 0 0 var(--dialog-ring-w) ...}`
  且 `--dialog-ring-w:.5px`。
- `design/main.pen` 已用脚本核对：32 个 `$brand-2` 描边帧全部 `strokeWidth: 0.5`，无 1px 残留。

## 遗留 / 需人类确认

- 真机观感（0.5px 在 125% / 150% 缩放下是否仍能分清两层弹窗）需崔总目视确认。
- `browse.pen` / `editor.pen` / `export.pen` 的弹窗帧仍无描边（PLAN §5 #11），
  下次打开 Pencil 时按 0.5px 批量补。
- Pencil 应用内的 `main.pen` 已同步为 0.5；磁盘文件也已写入同一值（应用内保存与否都不会回退）。
