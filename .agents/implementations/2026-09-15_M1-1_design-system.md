# M1-1 设计系统落地（令牌 / i18n / 字体 / 通用算法）

完成时间：2026-09-15 18:56:03 CST（**补记** —— 对应提交 `e77af79`）

> ⚠️ 这份记录是**事后补写**的：当时那次改动直接提交了，没有按 `AGENTS.md` §5.2 留下记录。
> 补记内容依据提交差异与 `plans/M1.md` §4.4 的勾选状态回溯，时间的准确性只到提交时间。
> 本文件与 `2026-09-15_M1-1_ui-components.md`（那次真正实时写的）是两个不同的工作段。

---

## 1. 范围

`plans/M1.md` §4.4 的第 6 ～ 12 项：把 `DESIGN.md` 的规格落成代码里的**唯一事实来源**，
为后面的外壳与导入工作区提供公共地基。**不含**组件实现（那部分在另一份记录里）。

## 2. 涉及文件

| 文件 | 作用 |
| --- | --- |
| `src/styles/tokens.css` | 颜色令牌（四级面 + 前景 + 品牌 + 状态）+ 密度令牌 + `@theme` 语义工具类映射 |
| `src/styles/fonts.css` | Inter Variable（拉丁/数字）+ Noto Sans SC Variable（CJK），均 SIL OFL 1.1 |
| `src/index.css` | 引入顺序与基础重置；删掉 M0-1 的临时令牌 |
| `src/i18n/{index.ts,zh-CN.ts,en-US.ts}` | 轻量 i18n 运行时 + 中英语言包（`DESIGN.md` §11.2 key 主列表） |
| `src/lib/shortpath.ts` + `shortpath.test.ts` | 路径缩写两层算法（`DESIGN.md` §12.3） |
| `src/lib/tile-flow.ts` + `tile-flow.test.ts` | Tile 流换行数学与尺寸档位（`DESIGN.md` §12.6） |
| `scripts/check-hardcoded-colors.mjs` | 「色值只允许出现在 `tokens.css`」的检查脚本 |
| `src/App.tsx` | 外壳骨架：验证语义工具类、表面分层与「无边线设计」 |

## 3. 关键决策与理由

| 决策 | 理由 |
| --- | --- |
| 令牌三层链路：`DESIGN.md` → `tokens.css` → `@theme inline` | 用户明确要求「前端也要有统一定义点，方便修改」。`@theme inline` 是关键 —— 非 inline 会在构建期把值解析成字面量，主题就切不动了 |
| 状态底用 `color-mix(in oklab, …)` 实时合成，浓度（`--tint-*`）独立成参数 | `DESIGN.md` §5.2：主色后面还要按出图效果微调，浓度不能写死 |
| 自己写 i18n 而不引第三方库 | key 规模小（~60 条）、无复数/日期需求；语言包完整性由 TS 类型静态保证（`en-US` 声明为 `Record<MessageKey, string>`，漏译编译期就报错） |
| 字体只引 `wght` 轴 | 界面不需要斜体，省一半体积；按 `unicode-range` 分片，WebView 只取用到的片 |
| `shortpath` 两层算法（整体重缩，而不是在第一次结果上截断） | 若首级/末级本身很长，一次缩略后生成的 `...` 会在某一侧「戳出来」，产出完全错误的截断 |
| 加 `scripts/check-hardcoded-colors.mjs` 而不是靠自觉 | 「主辅色还要调」是明确需求，漏改会直接导致两套颜色并存 |

## 4. 验证方式与结果

当时实测：

- `pnpm test`：`shortpath` 与 `tile-flow` 的用例通过
- `pnpm typecheck` / `pnpm build` 通过
- `pnpm lint:colors`：无硬编码色值
- WSLg 窗口目视（人类，见 `ASSISTANCE.md` B2 的记录）：三栏布局与深色令牌正常

## 5. 遗留问题

- `Design/main.pen` 与 `tokens.css` 的**手工同步**是这条链路唯一的脆弱点（`DESIGN.md` §9.1 已写明）。
- 当时**没有**任何「页面真的渲染出来了」的程序化冒烟手段 ——
  这一缺口后来由 `scripts/ui-smoke.mjs` 补上（见 `2026-09-15_M1-1_ui-components.md`）。
