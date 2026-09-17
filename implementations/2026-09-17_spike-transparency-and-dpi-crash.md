# 修：spike 窗口看不到透明区 + 拖到外接屏（DPI 变化）崩溃

完成时间：2026-09-17 19:03:28 CST

范围：`crates/raybend/src/render/gpu.rs`、`src-tauri/src/spike_viewport.rs`、`src/dev/SpikeViewport.tsx`
状态：两处根因均已修并重建产物；透明是否成立仍需人类目视（`AGENTS.md` §2.8）

---

## 一、崩溃：拖到外接屏 → 卡几秒 → 进程直接死

### 证据从哪来

`scripts/spike-win.mjs` 把子进程的 stdout/stderr 落到了 `/tmp/raybend-desktop.log`
（这一版刚加的）。人类的复现直接在这份日志里留下了**完整 panic backtrace** —— 不用再猜：

```text
thread 'spike-render' panicked at wgpu-30.0.1/src/backend/wgpu_core.rs:3981:18:
wgpu error: Validation Error
Caused by:
  In Surface::configure
    The `SurfaceOutput` returned by `get_current_texture` must be dropped before
    re-configuring via `configure` or retrieving a new texture via `get_current_texture`.

thread 'spike-render' panicked at …/wgpu_core.rs:4065:22:
wgpu error: Validation Error
Caused by:
  In Surface::release_texture
    Surface is not configured for presentation
```

### 真根因

`GpuContext::render()` 里那个 `Suboptimal` 分支：

```rust
wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
    self.surface.configure(&self.device, &self.config);   // ← frame 还活着就 configure ✗
    frame
}
```

wgpu 的硬规矩是**持帧期间不能 `configure`**。而 `Suboptimal` 恰恰是「尺寸/DPI 刚变」时
surface 的返回值 —— 所以把窗口拖到另一块屏（触发 DPI 变化）必炸 ✓ 与人类的现象完全对应。

**为什么是「直接崩掉」而不是「窗口卡住」**：`cargo` 的 dev 档是 `panic = unwind`，
第一次 panic 在解开过程中要释放那个还活着的帧，而 surface 此时已不再处于可呈现状态 →
**第二次 panic** → 双重 panic 直接 abort，进程当场死。

### 修法

`GpuContext` 加一个 `reconfigure_pending` 标记：

- `Suboptimal` 分支只**挂标记**（那一帧仍然能画，先画完呈现出去）；
- 真正 `configure` 挪到**下一帧开头**——此时手上没有帧（`get_current_texture()` 之前）。

代码里把这段事故经过写进了注释，免得后来者「顺手」把 `configure` 挪回去。

### 顺带修的另一个真 bug：洞口上报后没重新适配

`HoleRect`（界面把洞口矩形报给 Rust）只设了 `clip_rect`，**没有重新适配**，
于是 `Fit` 算的还是「整窗」那一套：实测 zoom 停在 `0.41`
（按整窗 2560×1640 适配），而按洞口应该是 `0.05` 上下 —— 洞里看到的是一块放大的局部。
现在按当前档位重适配（`Free` 档不动，否则会把用户自己的缩放抹掉）。

## 二、看不到透明区：中间被整块底色盖住了

`src/dev/SpikeViewport.tsx` 中列原本是：

```tsx
{/* 洞口之外的四周保持不透明（真实产品里这里就是界面面板） */}
<div class="pointer-events-none absolute inset-0 -z-10 bg-surface-main" />
<div class="absolute inset-0 -z-20 bg-surface-main" />
```

注释写的是「洞口**之外的四周**」，但 `inset-0` 是**整块盖满** —— 负 z-index 只决定页内堆叠，
**并不会让页面像素变透明**，所以洞口也被刷成了不透明 ✗ 于是「透明挖洞」根本没出现。

改成**只画环**：上/下/左/右四条底色带（用同一个 `HOLE_MARGIN_PX = 190` 常量，
与 Rust 的 `HOLE_MARGIN_CSS` 对齐），中间那块保持无背景。

> 一条容易读反的界面约定：工具条上那个按钮的文案是**动作**而不是状态 ——
> 写着「洞口：关（整窗出图）」表示**点它会关掉**，也就是当前洞是**开着的**。
> 我自己先读反过一次，在此记下。

## 三、验证

```text
npx tsc --noEmit                     0 error
pnpm test                            487 passed
pnpm lint:colors / lint:arch / lint:i18n   通过
cargo test -p raybend                759 passed / 1 ignored
cargo test -p raybend-desktop        35 passed
cargo clippy --workspace --all-targets     0 warning
pnpm spike:win                       构建 + 核对 + 启动，退出码 0
```

启动后应用自身日志（**无 panic**，且窗口确实开了）：

```text
[raybend] 数据底座就绪：C:\Users\andar\AppData\Local\com.cthun.raybend\app.db（位置：local）
[raybend] spike 调试窗口已打开
[raybend] 主窗口已就绪，等闪屏露满 3 秒后显示
```

**未经人类验证**：洞口那块的像素是否真的透出桌面/下层窗口、wgpu 画的图是否落在洞里、
1:1 白块是否像素级锐利、以及跨显示器拖动是否不再卡死 —— 全部要人类目视 + 实操确认。

## 四、遗留

- **panic 落盘**：现在 panic 只在「由脚本启动」时能落进 `/tmp/raybend-desktop.log`；
  双击启动的崩死不留痕。要做成常驻习惯，应加一个 panic hook 写进
  `%LOCALAPPDATA%\com.cthun.raybend\logs\`（`AGENTS.md` §6.4 已预留该目录）。留给 W2。
- 渲染线程 panic 会**带走整个进程**（release 档 `panic = "abort"`）。真实产品里
  视口线程崩溃不该带走主窗口 —— 这条属于 `AGENTS.md` §8 第 8 条（GPU 兼容与回退），
  等 M2 真做视口时统一处理。
