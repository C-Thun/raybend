# tiles 状态条统一（import / browse 同一个组件）

完成时间：2026-09-19 17:33:12 CST

## 范围

人类 2026-09-19 08:00 那批里关于 tiles 状态条的部分：两侧**几乎没一个元素是一样的**，
逐条给了统一口径。本轮把「tiles = 网格 + 状态条是一个整体」落成组件，
并把清单里每条都实现掉。

## 改动

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/tiles/TilesControlBar.tsx`（新） | 由 `features/photo-grid/GridControlBar.tsx` 移来并扩成**可配置**的共用条 |
| `src/components/ui/tiles/TilesShell.tsx`（新） | 外框：上面是网格/看图区（children），下面是状态条（`bar` 配置） |
| `src/components/ui/tiles/index.ts`（新） | 出口 |
| `src/features/photo-grid/GridControlBar.tsx` | **删除**（不再有第二份） |
| `src/features/photo-grid/PhotoGrid.tsx` | 交出状态条（工作区用 `TilesShell` 拼）；网格只管画格子 |
| `src/workspaces/import/ImportWorkspace.tsx` | 中列改 `TilesShell`，把配置传进去 |
| `src/workspaces/browse/BrowseWorkspace.tsx` | 删掉**内联**的那份状态条，改同一个 `TilesShell`；`currentLabel` → `currentLead`（文件名交给组件拼） |
| `src/lib/format.ts` + `.test.ts` | 新增 `fileNameOf（路径末级名字）`：导入侧的「当前那张」是路径，状态条要的是名字 |

## 逐条对照（人类原话 → 实现）

| 人类要求 | 落地 |
| --- | --- |
| 左边张数**含选中计数**，import 里为什么不能有 | 两侧都显示 `N 张` + `已选 n`（`>0` 才出现） |
| 中间：import 漏了选中文件名；**9 点图标去掉**；browse 把目录路径换成库名 | 中间 = 容器 + `· 文件名`；`IconGridDots` 从这条上删掉；browse 传 `label`（库名 / 最后一级目录），import 传 `dir`（`PathText` 缩写） |
| 按时间：**矮高度** + **保留图标** | 条高统一 `h-8`；`ToggleBlock` 带 `IconClock` |
| 排序：browse 保留、与 import 有差异、以后随时能开 | 排序做成 `sort` **可配置项**：browse 传，import 不传 ⇒ 整块不出现（`data-sort` 是断言钩子） |
| 缩放：import 中的更好 | 用 import 那档（两端加减号图标、`w-40`、9 档离散） |
| 两侧档位**各自记录**，但组件必须同一个 | 组件同一个；档位仍分别走各自的 store（browse 的 `tileStep` / import 的 `GRID_SETTING_KEYS.tileStep`） |
| 📌「我们沟通语境下的 tiles，就是包括下面状态条的……封装起来供外框架整体引用」 | `TilesShell` + `TilesControlBar`，两个工作区各传配置；**没有第二份状态条实现** |

顺带收敛掉一处重复：`groupingLocale`（`locale → GroupingLocale` 的映射）原先在
导入侧与网格里各有一份 —— 现在状态条**自己认界面语言**（`locale` 属性变成可选覆盖），
调用方不再抄。

## 验证（Agent 侧冒烟）

```text
pnpm typecheck → 0
pnpm test → 672 passed（新增 fileNameOf 的 2 条，含末尾分隔符/纯分隔符/中文名等边界）
pnpm lint:colors / lint:arch / lint:i18n → 全绿
pnpm build → 通过
pnpm smoke:ui → problems: []（新增：导入侧状态条用的是共享组件、且**排序未开启**）
pnpm check:browse → 通过（新增：浏览侧 `[data-tiles-control-bar]` 在位、
  信息开关 / 排序 / 缩放 / 按时间都在）
```

**未经人类验证**：两条状态条在两主题、紧凑/宽松两档密度下的观感与对齐
（尤其「已选 n」与中间那段挤在一起时的观感、矮高度下按钮命中区域）—— 属目视范畴。
