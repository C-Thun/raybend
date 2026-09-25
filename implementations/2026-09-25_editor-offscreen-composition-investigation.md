# editor 窗口越出屏幕闪烁：架构核查与 DirectComposition 对照
完成时间：2026-09-25 17:48:17 CST

## 本次范围

- 根据真机反馈复核整体实现，区分小修小补与平台呈现层的局部架构修正。
- 复用当前 `GpuContext`、环境变量支持和 `scripts/lib/cdp.mjs`；未改渲染代码、前端或默认后端，未新增依赖。
- 文档：`FUTURE.md` C9、`docs/native-viewport-coordinate-guide.md` §8.1，并将新反馈记回上一轮实施记录。
- 无新增用户功能，不涉及命令注册表/默认热键。

## 已查实的代码事实

依赖源码均来自 `/home/andares/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`。

1. `tauri-runtime-wry-2.11.4/src/lib.rs:4700` 为透明窗口建立 softbuffer surface；
   `:4189` 的 `Event::RedrawRequested` 调用 `window.draw_surface`。
2. `tauri-runtime-wry-2.11.4/src/window/windows.rs:46` 的 `draw_surface` 按客户区尺寸
   调整软件缓冲、`buffer.fill(color)`；默认 color=0；随后 present。
3. `softbuffer-0.4.8/src/backends/win32.rs:176` 的 present 最终对窗口 DC 执行
   `Gdi::BitBlt(..., SRCCOPY)`，并 `ValidateRect`。
4. 本仓 `render_window.rs::raw_handles` 取得主窗口 HWND，`GpuContext::new` 用 RawHandle
   创建 surface；上一轮真机冒烟确认默认实际为 Vulkan / Opaque。即 GPU 与软件背景绘制目标相同。
5. `wry-0.55.1/src/webview2/mod.rs:205` 创建单独的 `WRY_WEBVIEW` 子 HWND，
   样式 `WS_CHILD | WS_CLIPCHILDREN`；使用普通 `CreateCoreWebView2Controller`，不是由我们管理的统一合成树。
6. `wgpu-types-30.0.1/src/backend.rs:675` 已提供 `Dx12SwapchainKind::DxgiFromVisual`。
   `WGPU_DX12_PRESENTATION_SYSTEM=visual` 可启用；同时 `WGPU_BACKEND=dx12` 选择 DX12。
   单独切 DX12 仍默认 `DxgiFromHwnd`，不能把后端名称变化等同于已隔离绘制层。
7. `wgpu-hal-30.0.1/src/dx12/dcomp.rs:98` 用 `CreateTargetForHwnd(hwnd, false)` 建
   DirectComposition target；`dx12/mod.rs` 经 `CreateSwapChainForComposition`、SetContent、Commit
   把 GPU swapchain 挂到 visual。GPU 内容位于父窗直接绘制层上方、子窗口下方。

## 判断与未证明部分

- **总体分工可以保留**：DOM 做界面与交互，Rust/wgpu 做图像及变换。问题集中在 Windows 的平台合成接入。
- 现有接入存在具体的同 HWND 绘制竞争；Opaque 改善了 alpha，但没有隔离 Tauri 的软件背景更新。
  越出屏幕后重新露出区域需要重绘，与反馈吻合；没有捕获该次闪烁的 WM_PAINT/Present 时序，
  所以这是代码证据支持的主要解释，不宣称已经逐帧证明因果。
- Vulkan swapchain 的 `.clipped(true)` 允许裁剪不可见区域，但这是合法且官方建议的常见配置；
  不能仅凭它就断言 wgpu/Vulkan 有 bug，也不应直接 fork wgpu 改它。
- 当前 Suboptimal→下一帧 configure 有重配放大风险，但没有计数证据证明此次发生了重配风暴。
- 优先验证 DirectComposition 独立合成层。无需推翻编辑器、把图像运算迁到 JS、引入 CEF，
  也不以常驻 60Hz 重绘掩盖同层覆盖。如果子窗口裁剪仍阻断，可进一步评估显式合成树接入。

## 官方依据

- https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd
  明确列出父窗直接绘制、下层 visual、子窗口、上层 visual 四层，及 topmost 控制。
- https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components
  DirectComposition 的 visual tree 是保留式结构，由 DWM 合成；改动通过 Commit 提交。
- https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model
  直接 HWND 的 flip model 不应与其它绘制 API 混用；不能把简单切 DX12 HWND 作为最终隔离方案。
- https://docs.vulkan.org/refpages/latest/refpages/source/VkSwapchainCreateInfoKHR.html
  clipped 允许忽略不可见区域；不保证一定发生裁剪。
- 使用 firecrawl-search 查询后，对决定性结论直接核对官方文档；搜索中出现的论坛/AI 回答没有作为事实依据。

## 验证与当前试运行

- 最初尝试只读原实例状态，9334 调试口未开；随后 `tasklist.exe` 确认没有 raybend-desktop.exe。
  没有关闭或杀掉已有实例。
- 利用**同一份已构建 exe**，仅本次进程设置 `WGPU_BACKEND=dx12`、
  `WGPU_DX12_PRESENTATION_SYSTEM=visual`，通过 WSLENV 透传；全局环境与配置未变。
- 在新实例执行命令级冒烟：设临时小视口 → bind → 查询 ready/history → 检查空闲
  → 发 reset 意图 → 再次检查空闲 → unbind；未拖动/缩放窗口，未选择照片，未做视觉 E2E。
- 结果：`Intel(R) Arc(TM) Graphics · Dx12`；history `Dx12 / Bgra8UnormSrgb / Opaque`；
  首帧 1，空闲仍 1，命令唤醒为 2，空闲仍 2；无错误、无重启。
  通过现有环境解析与 DX12 surface 源码确认 Visual 路径的选择；现有 history 本身只报告 backend/format/alpha。
- 冒烟会话已回收，**主窗口留着进行同场景目视对照**。重启后不会自动保持这些开关。
- 脚本 `/tmp/raybend-editor-dcomp-smoke.mjs`，结果 `/tmp/raybend-editor-dcomp-smoke.json`。
- 本轮仅文档与进程参数变化，不重跑编译/单元测试；`git diff --check` 通过。

## 待目视确认

在本次已启动的 DirectComposition 实例中，进入 editor 选同一张照片，确认照片可见、
工具覆盖层可见，再复现窗体边缘越出屏幕/拖回及跨屏移动。命令成功呈现不等于最终合成可见，
仍不能宣称闪烁已消除。对照通过后，再固化 Windows 默认呈现路径并补选择策略单元测试。

## 对照结果（2026-09-25 18:00:08 CST）

崔总复测后确认「成功了」。将该结论按原场景记回本记录；不能外推到未测试的其它 GPU/驱动。
现在进入默认策略固化，见 `implementations/2026-09-25_editor-directcomposition-default.md`。
