# workspace 右列宽度统一成一个令牌（−15%）

完成时间：2026-09-23 02:23:10 CST

## 改动范围

人类要求：「import/browse 的 workspace right 宽度再缩减 15%，这块定一个统一宽度，
方便所有 flow 的 workspace right 宽度一起改」。

| 文件 | 改了什么 |
| --- | --- |
| `src/styles/tokens.css` | `--panel-w-right` 从两个密度块**移到基础层**，值 300/340 → **255px**（= 300 −15%）；注释写清沿革与「这里是唯一一份定义」 |
| `src/lib/layout-prefs.ts` | **删掉** `BROWSE_RIGHT_WIDTH` 常量（浏览右列的旧定义） |
| `src/App.tsx` | 不再往 BrowseWorkspace 传 `rightWidth` |
| `src/workspaces/browse/BrowseWorkspace.tsx` | 删掉 `rightWidth` prop 与内联像素宽，右列改用 `w-panel-w-right`（与导入侧同一个类名/令牌） |
| `src/styles/tokens.test.ts` | 白名单里去掉 `--panel-w-right`（它已不在密度块里） |
| `src/lib/layout-prefs.test.ts` | 注释跟上（那条「老键必须被忽略」的行为不变） |
| `scripts/ui-smoke.mjs` | 新增断言：导入右列、浏览右栏的**实测宽度必须等于令牌值** |
| `DESIGN.md` §13.5 | 「右列固定 300px」→ 令牌口径 |

## 关键决策

1. **只留一份定义**：此前 import 走 `tokens.css` 的 `--panel-w-right`（300/340，随密度），
   browse 走 `layout-prefs.ts` 的 `BROWSE_RIGHT_WIDTH = 300` 常量 —— 同一个东西两处写，
   改一处忘一处（2026-09-19 就真的把宽度加到了**导入**侧而不是浏览侧）。
   现在唯一来源 = `--panel-w-right`，两个工作区都挂 `w-panel-w-right`。
2. **值 = 255px**（当前取用的 300 × 0.85）。注意口径：浏览侧与紧凑档导入是 −15%，
   **宽松档导入是 340 → 255（−25%）** —— 这是「统一成一个宽度」的必然结果，
   不想要的话就得回到两档两个值（那就又是两处改）。
3. **代价：右列不再随密度变**（原来宽松档 340）。已经写进令牌注释，不当它是不小心丢的。
4. **它仍然是常量、不是偏好**：右列不给拖拽把手（`DESIGN.md` §8.6），
   老 profile 里那个键继续被忽略（否则「曾经存过 300」会永远压住新默认）。
5. **命令体系接入评估（`AGENTS.md` §2.15）**：宽度是令牌、既不能拖也没有命令，无需接入。

## 验证（Agent 侧：冒烟 + 单测）

- `pnpm typecheck` / `pnpm test`（792 通过）/ `pnpm lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build`：全绿。
- 构建产物：`.w-panel-w-right{width:var(--panel-w-right)}` —— 类名真的生成了。
- `pnpm smoke:ui`：全绿 `problems: []`，并且**真浏览器里量到**：
  * 导入右列 `255px`，令牌 `255px`；
  * 浏览右栏 `255px`，令牌 `255px`。
  这两条断言（宽度 == 令牌）就是这次改动的回归闸门 —— 谁再把宽度硬编码回 JS 里，它当场红。
- 没再跑 `check:browse`：本次没动网格/数据路径，而 ui-smoke 那次已经真的切进浏览工作区
  （量到两列 aside、右栏 255px、无控制台错误）。

## 未由 Agent 声称验证的 E2E

- 真机目视：255px 下导入右列的库卡片、浏览右栏的 EXIF/预览+直方图摆得下不好看；
  以及宽松密度下右列比左列窄是否可接受（这属于观感，归人类）。

## 遗留（需要人类在 Pencil 里做）

设计稿里宽度是**写死的 frame 宽度**，需要跟着改成 255：

- `design/main.pen`：`Panel / Repositories 右列` 三处 —— `Shell / Import / Dark-Compact`（300）、
  `Light-Compact`（300）、`Light-Loose`（340）；
- `design/browse.pen`：`Panel / Info 右列` 四处 —— `Shell / Browse / Tiles`、
  `Browse / View`、`Browse / Compare`、`Browse / Libs Expanded`（都是 300）。

本工作区（WSL）起不了 Pencil 会话，没有冒充已改 `.pen`。
