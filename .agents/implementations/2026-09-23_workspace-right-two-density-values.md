# workspace 右列宽度：两档两个值（255 / 267），但仍然只有一份定义

完成时间：2026-09-23 02:33:05 CST

人类 2026-09-23 的修正意见：「宽松的横向 padding 还是要增加的，这个不能完全一样，
但可以不加得太多（300 到 340 也太多了），所以这个可以为了两种[密度]模式提供 2 套配置，
但仍然所有 right 共用」。

> 本记录**修正** `2026-09-23_workspace-right-width-unified.md` 里那条
> 「代价：右列不再随密度变」—— 它只活了一个提交，人类当场指出宽松档不该与紧凑档同宽。

## 改动范围

| 文件 | 改了什么 |
| --- | --- |
| `src/styles/tokens.css` | `--panel-w-right` 回到**两个密度块**：紧凑 **255px** / 宽松 **267px**（不再放在基础层），值的关系写在注释里 |
| `DESIGN.md` §13.5 | 右列宽度口径改成两档两值，并写明「宽松只多出内边距的差」 |

`src/lib/layout-prefs.ts`（常量已删）、`BrowseWorkspace.tsx`（已改挂 `w-panel-w-right`）、
`scripts/ui-smoke.mjs`（「宽度必须等于令牌」的断言）**都不用再动** —— 这正是
「一份定义 + 两档两个值」的好处：两边都在读同一个令牌，令牌随密度解析。

## 关键决策

1. **宽松档不是把 340 搬回来，而是「内边距多的那一点」**：`--panel-pad` 紧凑 6px /
   宽松 12px（每侧 +6px），所以 267 = 255 + 2×6 —— **内容区宽度两档一致**，
   宽松档只多出呼吸空间。人类否掉的是「整圈大一圈」那种（300→340，+13%），
   而不是「宽松档不许变宽」。
2. **两档两个值 ≠ 两份定义**：定义点仍然只有 `tokens.css` 一处（`w-panel-w-right` 类名），
   import / browse 及以后的 edit / export 都读它 —— 人类要的「所有 right 共用」没有被破坏。
3. **两档必须都定义这个键**（否则缺的那一档会掉到 auto、整列塌掉）——
   `src/styles/tokens.test.ts` 的「两档密度定义的是**同一批**令牌」那条测试盯着它，
   本次 `pnpm test` 通过即为证据。

## 验证（Agent 侧：冒烟 + 单测）

- `pnpm typecheck` / `pnpm test`（792）/ `pnpm build`：全绿。
- 构建产物里两档都在：
  `[data-density=loose]{… --panel-pad:12px; --panel-w-left:360px; --panel-w-right:267px …}`，
  紧凑块是 `255px`；`.w-panel-w-right{width:var(--panel-w-right)}` 也在。
- `pnpm smoke:ui`：全绿 `problems: []`，真浏览器（宽松档）量到
  **导入右列 267px、浏览右栏 267px，令牌 267px**。
  紧凑档的 255px 由「构建产物里紧凑块的值 + 同一条 `width == 令牌` 断言」覆盖
  —— 那条断言与档位无关，哪一档量都必须是当前档的令牌值。

## 未由 Agent 声称验证的 E2E

- 真机目视：255 / 267 两档下库卡片、EXIF 与预览+直方图的摆布（观感归人类）。

## 遗留（需要人类在 Pencil 里做）

设计稿里宽度是写死的 frame 宽，仍待改成两档新值：

- `design/main.pen`：`Panel / Repositories 右列` × 3 —— `Import/Dark-Compact` → 255、
  `Import/Light-Compact` → 255、`Import/Light-Loose` → 267；
- `design/browse.pen`：`Panel / Info 右列` × 4（`Browse/Tiles`、`Browse/View`、
  `Browse/Compare`、`Browse/Libs Expanded`）→ 255（浏览器侧只有紧凑那几张的话就都是 255）。

本工作区（WSL）起不了 Pencil 会话，没有冒充已改 `.pen`。
