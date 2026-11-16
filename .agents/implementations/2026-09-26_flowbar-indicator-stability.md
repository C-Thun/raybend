# FlowBar 选中背景消失修复
完成时间：2026-09-26 23:31:26 CST

## 范围与根因

修复工作流选择器启动时或状态更新后丢失当前 flow 背景的问题。

- `FlowBar` 原先在响应式 `options` getter 中执行 `WORKFLOWS.map(...)`，每次生成全新选项对象。
- 队列状态读取完成/刷新时，`exportStore.processing()` 的依赖变动，即使返回值仍是 `false`，也会重新计算这组对象；语言切换同样如此。
- `FlowSwitcher` 的 Solid `For` 按对象身份复用，因此四个 item 全部被替换。Ark/Zag 的选中背景尺寸观察器仍绑定旧 item，且选中 value 没变时不会重新绑定。
- 本地启动采集到明确证据：第一次测量 item 为 86×32 且 `isConnected=true`；下一次从 `ResizeObserver` 回调量到的 item 已经 `isConnected=false`，宽高都是 0。调用栈落在 Zag `radio-group.machine.mjs::syncIndicatorRect` 的闭包；随后 Indicator 的四个几何变量均变为 `0px` 并被设置 `hidden`。
- item 的 checked 状态和文字仍正常更新，所以出现「选中项文字深色，主色底消失」。这是节点被替换后观察器仍引用旧节点的问题。

## 改动与复用

- `src/shell/FlowBar.tsx`：四个选项只创建一次；`label` / `processing` 改为响应式 getter。复用原有 FlowSwitcher、Solid For、Ark Indicator，不新建尺寸观察器或计时修补。
- 保持现有单个背景块滑动的设计；未给 item 叠加第二块背景。没有视觉规格变化，无需修改 Pencil 设计稿。
- `scripts/check-flowbar.mjs`：复用 `scripts/lib/cdp.mjs`，挂载真实 FlowBar / Solid / Ark，使用小型合成状态验证背景几何、节点身份和焦点。
- `package.json`：仅增加 `check:flowbar` 命令，保留工作区已有其他改动。
- 命令/快捷键：这次只修复现有呈现与节点生命周期，没有新增可触发功能；现有 flow 命令及默认键位不变。

## 已验证（冒烟）

- 先运行未修复版本：37 项检查中 31 项失败，复现背景隐藏、节点替换与焦点丢失；控制台无异常，说明仅检查启动无报错无法发现此问题。
- 修复后 `pnpm check:flowbar`：37 项全部通过，无异常和控制台错误。覆盖应用启动、初始 import、重复 false 队列刷新、处理开始/结束、flow 与队列同时更新、四个 flow、中英两种语言、紧凑/宽松两档密度、连续快速切换；同时检查导出 shimmer 仍随处理状态出现/消失。
- `pnpm test`：1043 项通过，约 1.43 秒。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：全部通过。
- `pnpm build`：通过，保留已有大 chunk 提示；本地 dist 已更新。

## 未经真机验证与环境说明

- 没有构建 Windows exe 或验证 WebView2 中实际启动/切换的视觉效果；Windows 版本需要重建后由崔总确认。
- 本轮是组件 DOM/状态冒烟，不声称完成 GUI、真实照片库或色彩 E2E。
- 当前工具沙箱因 WSLg `/mnt/wslg/distro` 挂载冲突无法启动（apply_patch 同样受影响）；通过获准的沙箱外定向读写/测试继续完成。未更改挂载或沙箱配置。
- 未提交、推送或发布；工作区原有大量并行改动保持原状。
