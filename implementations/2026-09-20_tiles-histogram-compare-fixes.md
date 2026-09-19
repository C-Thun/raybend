# tiles、直方图与 Compare 问题修复
完成时间：2026-09-20 01:01:29 CST

## 范围

- 把照片网格的 `px-2 py-2` 内边距收进唯一的 `PhotoGrid` 实现，删除 Browse 调用点的特例；Import 与 Browse 现在引用完全相同的网格外框。
- 直方图保留 52 点与七色显式分层，但删除自写的单调三次插值，改为逐采样点直连的填充折线。
- 修复 Compare 多图取图、画框比例、同步缩放/拖动、浮动控件、点击切换当前照片与右栏联动。
- 将新需求原文追加进 `docs/user-requirements.md`。

## 涉及文件

- 网格：`src/features/photo-grid/PhotoGrid.tsx`、`src/workspaces/browse/BrowseWorkspace.tsx`
- 直方图：`src/lib/histogram.ts`、`src/lib/histogram.test.ts`、`src/components/ui/Histogram.tsx`
- Compare / Viewer：`src/components/ui/viewer/{CompareView,Viewer,ViewerControls,interaction,store,index}.*`
- 工作区接线：`src/workspaces/{browse,import}/*Workspace.tsx`
- 对比几何：`src/lib/viewer-compare.ts`、`src/lib/viewer-compare.test.ts`
- 浏览器冒烟：`scripts/check-browse-boot.mjs`

## 关键决策与原因

1. **网格间距归唯一组件所有**：此前两侧已经共用 `PhotoGrid`，但 Browse 在调用点额外传 `px-2 py-2`，Import 没传，因而 Import 左边切边。默认间距现在只定义一次，两侧不再有隐式差异。
2. **直方图改直线不是替换第三方库**：仓库没有引入曲线库，异常来自 `histogram.ts` 自写的三次插值。每条曲线单独看虽然不过冲，但色带的 top / bottom 分别拟合后可能在采样点之间交叉，形成数据里没有的细线或尖刺。逐点折线不会编造中间形状。
3. **多图仍走同一个 ViewerStore**：没有在 Compare 里另写取图实现。Store 新增有限容量的逐照片 URL 表与在途请求合并；每幅画框按自己的照片取 URL，关闭时统一回收。
4. **画框用明确像素尺寸保持比例**：根据 Compare 网格的行列、间距、容器尺寸与第一幅的比例算 contain 尺寸，ResizeObserver 跟随窗口变化，避免 `height: 100% + max-width` 在约束冲突时破坏比例。
5. **Compare 使用相对适配倍率**：基础图片已经铺满画框，渲染倍率使用 `当前 store 倍率 / 进入 Compare 时的适配倍率`；拖动使用画框 CSS 像素，一次鼠标位移对应同量画面位移，并按放大后的画框溢出量夹取。
6. **控件命中与显隐只保留一套**：`interaction.ts` 同时供 Viewer / Compare 使用。按钮命中时不启动 pointer capture，避免“能 hover、不能 click”；左上与右下沿用单图的角落唤醒规则。
7. **绿色边框表示当前照片**：点击 Compare 画幅只调用 viewer `focus()` 并移动 Browse 选择锚点，不散掉多选，也不重置共同缩放/平移；Browse 右栏与底部状态随当前照片切换。

## 验证

- `pnpm test`：59 个测试文件全部通过；新增/更新了多图 URL 唯一性与回收、焦点切换保留变换、画框比例、CSS 像素拖动边界、控件命中/角落显隐、直方图折线路径等边界测试。
- `pnpm typecheck`：通过。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过。
- `pnpm build`：通过。
- `pnpm check:browse`：通过；浏览器里验证两幅 Compare 使用两个不同 URL、画框同比例、控件默认隐藏并能靠近唤醒、放大按钮改变倍率、当前画幅/右栏切换及返回退出。
- `pnpm smoke:ui http://localhost:1420/dev/kitchen-sink`：通过。
- Windows：用 `--features custom-protocol` 重编成功；`pnpm check:win` 确认 exe 晚于 dist 且 4 个引用资源全部嵌入。

## 未经 Agent 验证

- Windows 真机上的视觉正确性与手感仍需人类目视：Import / Browse 左右边距是否一致、真实照片直方图是否不再出现异常细线、不同尺寸/方向的 2–4 张照片是否无拉伸、拖动体感与角落唤醒是否符合预期。

## 遗留问题

- 本工作单元没有已知代码遗留。工作区原有的并行会话文件保持未动、未纳入本次改动。
