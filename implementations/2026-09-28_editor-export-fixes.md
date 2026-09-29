# editor/export 修一批：定稿保存重构、对比过滤、预设删除、右栏跳顶

完成时间：2026-09-28 21:30:00 CST

## ① 定稿保存：先关窗再干活（#14/#20，崔总两次报）

**症状**：点「保存」后按钮一直按下、弹窗关不掉；手动关掉后定稿列表里没有新定稿，但也存不了
（哈希已存在）；直到进出对比模式列表才刷新。

**根因（前端侧）**：`issue_create` 里**同步跑整套定稿渲染**（管线 + AVIF），`finalize` 把弹窗押在
这个 await 上——后端慢/卡时弹窗就「死」；回填 `setIssueLibrary` 又没有换图守卫，晚到的响应会把
别的照片的定稿列表盖上来。列表不刷新是失败路径（catch）下没有任何重拉。

**重构**（`EditorWorkspace.finalize`）：
1. 校验通过**立即关窗**（`setFinalizeOpen(false)`，崔总的 UI 先行口径）；`finalizeBusy` 只拦重复保存
   （工具条「定稿」按钮的 `canFinalize` 同时加 `!finalizeBusy()`）。
2. 完成后回填列表——带 `stillCurrent()` 守卫（换过库/照片就不回填）。
3. **成败都再拉一次** `getIssueLibrary`（失败路径也要让「其实已经存进去的定稿」立刻现身）。

后端侧（渲染同步在 `issue_create` 里）维持现状；真机若仍见长耗时，属 `issues.rs::ensure_snapshots`
的管线耗时问题，另行登记。

## ② 对比模式：不许自己比自己（#22，崔总）

对比选择器（右下角选参考）把**右侧正在显示的那个定稿**滤掉——按 **profileHash** 而不是名字
（同名定稿可能是不同的图，不该误伤；同哈希才是「同一张」，重新定稿出的新 id 同哈希也一起滤）。
`selection` 不是 issue（latest/sooc/raw）时不滤。

## ③ 导出预设：卡片右下角移除 + 真删除（#23，崔总）

- **UI**：卡片从单按钮改为「容器 + 两个动作」——点卡片任意处（按钮除外）仍 = 选中（旧行为保留）；
  右侧一列：上 = 运行勾 + 「未完成/总数」计数（**上移**），下 = `EasyDestroyButton`（默认确认、
  `Shift` 跳过）。`data-export-preset` 挂容器（拖放命中与冒烟不变）。
- **store**：`deletePreset(id)`——运行中（enabled）或队列里还有 pending/failed/running 条目时**拦下**
  （`setError` 说原因；只剩终态历史的可以删）；删的是选中的预设要清选中。落盘走 `writeChain`。
  - 实现中踩到的真 bug（单测抓住）：`selectedPreset()` 是**从 presets() 里查**的，`setPresets(next)`
    之后再判断「删的是不是选中的」永远为假 → 草稿与选中偏好留着尸体。改成**删之前**记
    `wasSelected = preferences.value().selectedPreset === id`。
- 单测 +2（删除/清选中/落盘；队列未清拦截→清了再删）；`check:export` 补两条路（拦截要说原因；
  清队后删除成功且选中态清掉）。i18n 六键（中英）。

## ④ editor 右栏不许自己跳顶（#19，崔总点名两次）

`panels.tsx` 里两处 `scrollHost.scrollTo({top:0})`：点裁切/旋转（工具块插在栏顶）、点定稿后
（切到定稿页签）。两处都删——**阅读位置是用户的**；页签切换保留。将来要定位具体条目用
`scrollIntoView({block:"nearest"})` 那种最小位移（注释里写明）。

## 顺带（文档/约定）

- `memory/PLAN.md` §4：发版后主线从三条加到**四条**——新增「编辑器 preset 的存储与载入」
  （崔总 2026-09-28；含待定问题：粒度/存哪/与 LUT 和基础曲线的关系/导入导出格式）。
- **`todos/` 目录约定**（崔总定，已写进 `AGENTS.md` §10）：复杂需求原文先落
  `todos/YYYY-MM-DD-<简述>.md` 防 todo 工具丢细节；开工时以它为源写 specs，做完删/移走。
  首份：`todos/2026-09-28-export-issue-ordinal.md`（导出文件名的 issue 序号体系，未开工）。

## 验证

- `pnpm typecheck` / `pnpm test`（1078+2 项）/ 三条 lint：通过。
- `pnpm check:export`（含预设删除两条路）：通过。定稿重构与对比过滤为编辑器交互，
  合成冒烟覆盖不了 GPU 渲染路径，归真机验收。

## 遗留（已登记未做）

- **#21 切定稿选中态先行 + 视口中央「切换定稿中」提示**（分析到一半：定稿面板选中态来自
  `getIssueLibrary` 回的 `selection`，点击后要等 60ms 防抖 + IPC + 库重算才切——需要
  「本地即时 override + 到位后清除」与视口 loading 态接线）。
- **#16 browse tiles 点击后跳回开头**（分析到 `PhotoGrid` 的 pinsKey/pendingPin 机制，
  browse 的 pinsKey 含 `JSON.stringify(store.filter())`——嫌疑是点击触发的某条路径把
  pinsKey 变化误当成换目录走了 pin 恢复；未复现未修）。
- **#15 pi 视觉 skill**（借 glm-4.6v 等视觉模型看图）：未开工。
- 定稿渲染同步耗时的后端优化（`ensure_snapshots`）：未动。
