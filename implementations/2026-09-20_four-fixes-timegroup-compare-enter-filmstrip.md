# 四条修复：按时间分组 / compare 直进 focus / view 回车统一 / 胶片带

完成时间：2026-09-20 18:53:23 CST

## 范围

人类 2026-09-20 报的四条（原文已归档进 `docs/user-requirements.md`「第四批」，本批只修问题、不推进里程碑）：

1. 库目录下全部照片同属一个时段时，「按时间」开关无可见效果；
2. 进 compare 不能直接进 focus 态（否则 film 下选到第二张就被锁住，选不了第三张）；
3. 单图 view 的回车：import 能进能退、browse 退不出来 —— 逻辑必须统一；
4. 胶片带：整条尺寸 +20%；禁止原生横向滚动条（出现与否不得改变照片尺寸），滚轮转横向滚，配全局无感滚动条组件。

## 涉及文件

| 文件 | 改动 |
| --- | --- |
| `src/features/browse/grid-source.ts` | 显示序缓存键加入「按时间」开关；修复 `offset:` 死字段（应为 `offsetMinutes`，此前时区偏移被静默丢弃、未取回的条目会被误判成 UTC） |
| `src/features/browse/grid-source.test.ts` | 新增：开关切换后 `slices()` 必须立刻重算（同一时间线数组） |
| `src/components/ui/tiles/rows.test.ts` | 新增 6 条：平铺切行、**单日单片也必须出日+片标题**（人类的硬要求）、同日两片共用日标题、未知组无片标题、空片跳过、行键唯一 |
| `src/workspaces/browse/BrowseWorkspace.tsx` | 进 compare 不再自动 `setCompareStrip(true)`；删掉 focusNudge 信号与传参 |
| `src/workspaces/import/ImportWorkspace.tsx` | 同上一条（两侧同一条规则） |
| `src/components/ui/viewer/interaction.ts` | 新增 `takeViewerFocus(host)`：看图面打开时接管键盘焦点 |
| `src/components/ui/viewer/Viewer.tsx` | host `tabindex="-1"` + `outline-none`，onMount 里接管焦点 |
| `src/components/ui/viewer/CompareView.tsx` | 同上（对比面同理） |
| `src/features/photo-grid/PhotoGrid.tsx` | 焦点回归收进组件自己（盯 `viewer.state().active` 跃变），删 `focusNudge` prop |
| `src/components/ui/SubtleScrollbar.tsx` | 新组件：2px 横向进度指示（左到头宽 0、右到头占满、装得下不可见），绝对定位不占布局空间，`data-subtle-scrollbar` 供断言 |
| `src/styles/scrollbar.css` | 新增 `.scrollbar-none`（原生滚动条整个隐藏） |
| `src/components/ui/viewer/FilmStrip.tsx` | 结构改为「外层 relative + 滚动容器 + 指示条」；125 高 / 缩略 115×96（+20%）；`scrollbar-none` + `overscroll-x-contain`；`onWheel` 纵转横（deltaMode=1 行模式 ×16；横向分量留给原生；装得下不拦） |
| `scripts/check-browse-boot.mjs` | 新断言：胶片带 125 高 / scrollbar-width none / 指示条在 / 压窄视口后滚轮横滚 + 指示条按比例；进 compare 胶片带应为 all |
| `BROWSE.md` / `design/browse.md` / `DESIGN.md` | §5.5 重写（尺寸、无滚动条、滚轮、进对比默认全目录）；设计稿数值同步；组件表登记 `SubtleScrollbar`（#17） |

## 关键决策与理由

1. **回车统一 = 焦点归属，不是再加一层键位判断**。根因（浏览器探针实证）：browse 里看图打开后焦点留在底下 tile 上，tile 把回车当「激活」吃掉（preventDefault + 重新打开），看图件挂在 window 的「回车退出」永远收不到；import「能退」只是重渲染恰好把 tile 换掉、焦点掉到 body 的偶然行为。修法：**看图面（单张/对比）打开即接管焦点**（一份实现），关闭后由 PhotoGrid 把焦点收回（一份实现，删掉 focusNudge prop——它以前要两个工作区各接一遍线）。compare 的回车走命令分发器（会 preventDefault），与看图件内建回车互不干扰，行为不变。
2. **进 compare 不自动切胶片带**：默认全目录，回车（`viewer.compareOnly`）才切 focus 态；「只剩一幅自动退出 focus 态」的收尾保留。
3. **无感滚动条做成全局组件**（人类点名）：进度式（不是传统滑块）——宽度 = scrollLeft/max，天然满足「左到头不显示、右到头占满」；绝对定位贴底、不占布局空间，因此「可滚/不可滚」都不会改变缩略图尺寸。原生滚动条用 `.scrollbar-none` 整体隐藏（scrollbar.css 里已有滚动条体系，就近扩展）；滚轮纵转横写在 FilmStrip（只有它需要，不塞进通用组件）。
4. **顺手修掉 `offset:` 死字段**：`TimePhotoLike` 没有 `offset` 字段（对象字面量里带 spread 绕过了 TS 多余属性检查），导致浏览侧时区偏移全部被丢弃。改为只在取到时传 `offsetMinutes`（`undefined` = 未取回 → 本机时区兜底，`null` = 相机没写 → UTC，两者语义本就不同）。

## 验证

- `pnpm typecheck` 0；`pnpm test` **773**（新增 7 条全过）；`lint:colors` / `lint:arch` / `lint:i18n` ✓。
- 浏览器探针（`check-browse-boot.mjs` 的临时副本 + 探针段，`/tmp/probe-enter.mjs`，修复前后各跑一轮对照）：
  - ① 按时间：分组标题行 0 → 2 → 0（开关往返都生效；修复前 toggle 前后 scrollHeight 纹丝不动）；
  - ③ browse 回车：打开后焦点 = 看图件（修复前 = tile `DIV[option]`）→ **回车关闭 = true**（修复前 false）→ 关闭后焦点回到 tile（Enter 可再进）；
  - ② 进 compare：`stripMode="all"`（修复前自动变 `"compare"`）→ 再回车切 `"compare"`、只显示 2 张；
  - ④ 胶片带：高 125、`scrollbarWidth:"none"`、指示条在；压窄视口（620px）后 `wheelMoved=240`、指示条 `width:33.1492%`（= 240/724，数学吻合）；
  - import 对照：回车进/退仍正常（焦点现落在看图件上，行为从「偶然正确」变成「确定正确」）。
- `pnpm check:browse`（仓库门，含新断言）：**新断言全过**；失败清单从修复前 17 条降到 7 条，其中 10 条 compare/胶片带断言因本批转绿；剩余 7 条（显示偏好持久化 ×2、直方图 ×3、菜单 ×2）属于同工作树里**别的未完成 WIP**，与本批无关。

## 遗留

- Windows 真机 E2E 归人类（`AGENTS.md` §2.8）：真鼠标滚轮手感、触屏左右划动、2px 指示条的视觉浓度（当前 `bg-brand/70`）、125 高的整体观感。
- `SubtleScrollbar` 目前只有胶片带在用；以后别的横向滚动区（若也要求无滚动条）直接复用，不要各写一份。
- 探针脚本 `/tmp/probe-enter.mjs` 是一次性的，未入库；它的可复用部分已沉淀进 `check-browse-boot.mjs` 的新断言。
- check:browse 剩余 7 条失败等对应 WIP（显示偏好 / 直方图 / 菜单）收口，不在本批。
- 未 commit（等人类审）。
