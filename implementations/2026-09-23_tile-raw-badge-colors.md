# tile 的 RAW 角标：纯 `RAW` 改辅色底

完成时间：2026-09-23 01:38:52 CST

## 改动范围

人类要求：「tile 上右下的 RAW 标记，纯 `RAW` 改成辅色底，`+RAW` 保持主色底」。

| 文件 | 改了什么 |
| --- | --- |
| `src/components/ui/Tile.tsx` | 角标底色改成条件类名：`local.raw === "plus" ? "bg-brand" : "bg-brand-2"`（一行）+ 一句注释说明 |
| `DESIGN.md` §12.9.1 | 角落覆盖层那张表里补上两种角标的底色口径 |

## 关键决策

1. **辅色用令牌 `--brand-2`（琥珀金 `#F0B033`）= `bg-brand-2`**，不写字面色值 ——
   色值只许出现在 `tokens.css`（`pnpm lint:colors` 盯着）。
2. **文字色两种都不动**：`--fg-on-brand`（深字 `#202226`）。`DESIGN.md` §4.5 已经定过
   「品牌色块上的文字统一深色，主色 7.60:1 / 辅色 8.32:1」，辅色实底同样够对比度，
   所以不需要为它另造一套前景色。
3. **显示规则一个字没动**：仍与信息条二选一（选中 / 指向 / 键盘聚焦 / `marks-name` 强制时退场）。

## 验证（Agent 侧：冒烟）

- `pnpm typecheck`、`pnpm lint:colors`、`pnpm test`（792 通过）、`pnpm build`：全绿。
- **`bg-brand-2` 这个工具类真的存在**（写了不生效是这类改动唯一的坑）：
  构建产物 `dist/assets/*.css` 里量到
  `.bg-brand-2{background-color:var(--brand-2)}` 与 `.bg-brand{background-color:var(--brand)}`。
- 没跑浏览器冒烟：本次只换了一个颜色令牌，`ui-smoke` 量的结构（在角落层里 /
  不在照片盒里 / 与预留位镜像）与颜色无关，跑了也验不到这次的变化。

## 未由 Agent 声称验证的 E2E

- 真机上辅色（琥珀金）底与主色（青绿）底并排时的观感、以及深色 / 浅色两个主题下的对比。
  画廊里两张样例（4:3 的 `P1000025.RW2` 带 `raw="raw"`、3:2 的 `P1000027.JPG` 带
  `raw="plus"`）并排放着，一眼就能对照。

## 遗留（需要人类在 Pencil 里做）

`design/browse.pen` 里有 **30 处 `RawBadge`**，fill 全是 `$brand`，而且**没有 `+RAW` 变体** ——
设计稿需要：① 纯 `RAW` 的 `RawBadge` 改成 `$brand-2`；② 补一个 `+RAW` 变体（保持 `$brand`）。
本工作区（WSL）起不了 Pencil 会话（见 2026-09-21 胶片带记录的同一处说明），
所以**没有**冒充已改 .pen。
