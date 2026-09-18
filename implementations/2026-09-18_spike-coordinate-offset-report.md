# 问题报告：spike 里「输入坐标」与「绘制画面」差一个常量

> 交给第二位（人或 Agent）独立看的调查报告。写作时间：2026-09-18 15:20 CST。
> 目的：把已经排除的可能、已经量到的数、以及仍然存疑的点一次讲清，避免从零复述。

---

## 0. 一句话

Windows 上「webview 挖洞 + wgpu 在洞里直绘」的 spike：**画面本身摆得是对的**
（图像居中于洞口、四角标记正确），但**鼠标读出的图像坐标与画面上那个点不一致**，
差一个常量（垂直方向最明显，约几十物理像素的量级）。
数学自洽、单位无误，问题出在**「CSS 坐标的原点」与「wgpu 表面的原点」不是同一个点**。

## 1. 这套东西是什么（30 秒背景）

- 产品 raybend（相片管理，Tauri + Solid），渲染架构（`AGENTS.md` §6.1 方案 B）：
  webview 覆盖整窗，**中间挖一个透明洞**，Rust 用 wgpu 直接往窗口表面画照片。
- 本 spike（`PLAN.md` A.2）就是为了验证这条路在 Windows 上成不成立：
  透明合成、DPI、1:1 锐利、坐标同步、多显示器。
- 窗口：`src-tauri/src/spike_viewport.rs::open_window()`（`WebviewWindowBuilder`，
  `transparent(true)`，**保留系统标题栏**）。渲染线程拿窗口的原生 HWND 建 wgpu surface：
  `raw_handles()`（同文件 661 行）→ `GpuContext::new()`（`crates/raybend/src/render/gpu.rs:118`）。

## 2. 观察到的现象（人类实测，Windows 11 + Intel Arc，dpr 1.25）

1. **缩小态**（`zoom 0.0974`）：图像**正正好好居中在洞口里**，红/绿/蓝/黄四角标记都在。
   用截图坐标反推，落点与 `洞口中心 + pan` 的预测一致 → **绘制侧正确**。
2. **1:1 态**：把鼠标**对着左栏读数**挪到 `3001.1, 2000.3`（即图像中心），
   那个位置**不在**画出来的十字上；反过来，把鼠标压在十字上，读数**不是** `3000, 2000`。
   差值在视觉上「还挺远」（1:1 下 1 图像像素 = 1 物理像素，所以差值≈物理像素数）。
3. 因此：**缩放锚点跟着光标走时**，若光标没真正落在图心，「十字会随缩放在径向移动」——
   那不是漂移，是锚点位置本来就不在图上那个点。

## 3. 已经排除的（都有数）

| 假设 | 证据 | 结论 |
| --- | --- | --- |
| 漏乘 dpr | 左栏 `洞口(CSS) = (300,37,524,707)`，`洞口(物理) = (375,46,655,884)`，恰是 ×1.25 | **排除** |
| 视口数学错（fit/居中/反变换） | `Viewport::image_to_physical`（viewport.rs:140）与 `physical_to_image`（:153）互为逆；`rect_center()`（:134）用**洞口**中心；`zoom_for_fit`（:188）也用洞口；有单测 `fit_modes_use_the_hole_not_the_window`、`zoom_at_keeps_the_anchor_pixel_under_the_cursor`（dpr 1.0/1.25/1.5 各一遍） | **排除**（且绘制落点与预测吻合） |
| 命中测试用了另一套公式 | `hit_view()`（spike_viewport.rs:1067）就是 `pointer_to_image()`（viewport.rs:179 = `css × dpr` → `physical_to_image`），与自检 `coord_roundtrip_error()` 同一对函数 | **排除** |
| 前端送错字段 | 前端送的是 `event.clientX/clientY`（`src/dev/SpikeViewport.tsx::onPointerMove`），不是 `offsetX/Y` | **排除** |
| 洞口矩形过期 | 前端用 `ResizeObserver` + `getBoundingClientRect()` 上报，读数与屏幕上的虚线框一致 | **排除** |

> 附注：左栏那句「程序化往返最大偏差 0.000000 物理像素」**不能**证明坐标对齐 ——
> 它只证明 `physical_to_image` 与 `image_to_physical` 自洽。错位发生在
> 「CSS 坐标的原点是谁」这一层，不在这两个函数里。

## 4. 剩下的头号嫌疑

**webview 的 CSS 原点**（`clientX/clientY` 的零点）**与 wgpu surface 的原点不是同一个点。**

已知的窗口事实（都是 Tauri/tao 的 API）：
- `window.inner_size()` 给的是**客户区**尺寸（读数 `物理 1598×1022` 与 `CSS 1278×817` = ÷1.25 吻合）；
- `window.inner_position()` 给的是**客户区**左上角在屏幕上的物理坐标；
- `window.outer_position()` 给的是**窗口（含边框）**左上角；
- 前端 `window.screenX/screenY` 给的是 **webview** 视口左上角的屏幕坐标（CSS 像素）；
- 而 wgpu 的 surface 是从 **HWND** 建的（DXGI swapchain）。

三个「原点」在这一层上**未必相同**：窗口有标题栏与边框，webview 容器在窗口里的
位置也未尝只能是客户区原点。差一个标题栏（125% 下约 40 物理像素）正好与观察量级相符。

## 5. 这一轮已经把测量做进去了（等一次运行就能定性）

左栏「表面 / 窗口」区块现在多出四行（物理屏幕像素）：

```text
窗口原点       ← window.outer_position()
客户区原点     ← window.inner_position()
webview 原点   ← 前端 window.screenX/screenY × devicePixelRatio（新命令 WebviewOrigin）
输入偏移       ← webview 原点 − 客户区原点      ← 本次要的那个数
```

- `输入偏移 = (0,0)` 且显示「✓ 对齐」→ 输入与表面同坐标系，问题另有他因（下一步查
  `GpuContext::resize()`（gpu.rs:307）之后 `viewport.dpr`/`viewport_size` 的更新时机，
  洞口是 CSS 量、物理值是 ×dpr 得来的）；
- `输入偏移 ≠ (0,0)` → **就是它**，把 `pointer_to_image()` 的入参平移掉这个量即可。

## 6. 想请第二位独立判断的几个点

1. **Tauri 2 / tao 在 Windows 上**：`WebviewWindow` 的 webview 容器坐标原点，是与
   `inner_position()`（客户区）一致，还是与 `outer_position()`（窗口）一致？
   有没有已知的「webview 相对窗口内缩」？
2. **wgpu（DXGI swapchain on HWND）**：surface 覆盖的是**客户区**还是**整个窗口**
   （含标题栏与边框）？`Configure` 传的尺寸用的是 `inner_size()`（见 `GpuContext::resize`），
   如果 surface 实际覆盖整窗，那么「表面原点 = 窗口原点」而「客户区原点 ≠ 窗口原点」，
   输入侧就会差一个 frame 的偏移 —— 与现象一致。
3. 若 1、2 都不是，还有没有别的常见原因会让「输入与绘制差一个常量」？
   （例如透明/分层窗口 `WS_EX_LAYERED` 下的呈现偏移、DPI 感知级别
   `PerMonitorV2` vs `System` 的差异？）

## 7. 关键位置速查

| 作用 | 位置 |
| --- | --- |
| 建窗口（标题栏、透明、尺寸） | `src-tauri/src/spike_viewport.rs::open_window()`（360 行起） |
| 取原生句柄 | 同文件 `raw_handles()`（661 行） |
| 窗口事实（尺寸/DPR/两个原点） | 同文件 `read_window_facts()`（1204 行），**只允许主线程调** |
| 洞口上报 → 物理矩形 | 同文件 `apply_command()` 里 `SpikeCommand::HoleRect` 分支（959 行） |
| webview 原点上报 | 同文件 `SpikeCommand::WebviewOrigin`（62 行定义 / 945 行处理） |
| 命中测试 | 同文件 `hit_view()`（1067 行） |
| 变换数学（图像↔物理↔CSS） | `crates/raybend/src/render/viewport.rs` 121–230 行 |
| 逐帧绘制（scissor = 洞口） | `crates/raybend/src/render/gpu.rs::render()`（327 行，398 行设 scissor） |
| 窗口事实推送渲染线程 | `src-tauri/src/spike_viewport.rs` 的 `WindowFacts` / `pending_facts` 一带 |

## 8. 复现方式

```bash
cmd.exe /c taskkill /F /IM raybend-desktop.exe    # 先关掉旧实例（exe 被占用会构建失败）
pnpm spike:win                                     # 重建 + 启动（日志汇到 /tmp/raybend-desktop.log）
```

然后：① 抄左栏「窗口原点 / 客户区原点 / webview 原点 / 输入偏移」四行；
② 光标（现在是十字准星）压在图像中心的十字上，抄「命中测试」里的图像坐标。
这两组数一到手，「是不是原点问题」就定了。
