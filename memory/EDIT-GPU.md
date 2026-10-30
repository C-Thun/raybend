# 编辑计算 GPU 化 —— 进程账本

> 崔总 2026-10-31 定：编辑里的像素计算应当走 **WGSL shader（wgpu）**，不是 CUDA。
> CUDA 绑死 NVIDIA，桌面产品不能这么绑；本仓的 GPU 后端本来就是 wgpu（DX12 / Vulkan / Metal 通吃）。
> 本文件只记**事实与进程**：哪一段已经在 GPU、哪一段还在 CPU、掉出 GPU 的判据是什么、下一步该搬谁。
> 规格类内容（怎么实现、为什么这样实现）写在 `implementations/` 与 `specs/` 里，本文件只留指针。
>
> 导出**可以留在 CPU**（崔总 2026-10-31 明确接受）；能 GPU 化更好，但不是阻塞项。
> 缩略图 / 预览缓存同理（后台批处理，不是交互手感问题）。

## 0. 一句话现状（2026-10-31）

编辑器的「全局影调 + 曲线 + 饱和度/自然饱和度 + 显示 ICC/软打样」在 GPU 上；
**镜头校正、锐化、动态反差、几何（裁切/旋转）、创意 LUT、降噪 High** 都在 CPU。
只要碰了其中任何一项，整个视口就从「GPU 每帧重画」掉回「CPU 全分辨率重渲一遍」。

## 1. 判据：什么时候掉出 GPU 快速路

`crates/raybend/src/render/working_preview.rs::WorkingPreview::render` 里那个 `fast` 布尔就是
**GPU 快速路的唯一开关**。它成立时：只上传一次源图 + 一张 192 KiB 影调表，滑杆拖动**不重算像素**；
它不成立时：走 `render_working_params` → CPU 全画幅重渲，再整张上传。

| 条件 | 现在（2026-10-31 之前） | 本次改动后 |
| --- | --- | --- |
| `nr_method == High` | ❌ 掉出（走 `high_preview` 那条独立路） | ❌ 仍掉出 |
| `sharpen` 非恒等 | ❌ 掉出 | ❌ 仍掉出 |
| `dynamicContrast > 0`（局部色调） | ❌ 掉出 | ❌ 仍掉出 |
| `geometry` 非恒等（裁切/旋转） | ❌ 掉出 | ❌ 仍掉出 |
| **启用了 LUT** | ❌ 掉出 | ✅ **进入快速路（本次改动）** |

镜头校正与降噪 Fast **不在判据里**：它们被折进一次性算好的「neutral 源」（`tier.neutral` 缓存），
所以调它们要重算一次源，但之后拖滑杆仍是 GPU。

## 2. 逐个阶段的现状

| 阶段 | 位置（权威实现） | 现在跑在哪 | 代码指针 | 备注 |
| --- | --- | --- | --- | --- |
| 白平衡 / 曝光 / 对比 / 高光 / 黑区 | `develop/pipeline.rs::chain_linear` + `Resolved` | **GPU**（折成影调表） | `spike.wgsl::tone_channel` + `render/tone.rs` | 表 8192×3 行，参数变化只重写 192 KiB |
| 曲线（基础曲线 + RGB + 单通道） | `develop/curve.rs`、`gpu_description()` | **GPU**（折进同一张表） | `develop/working.rs::gpu_description` | CPU 与 GPU 共用同一份 `curve_at` 定义 |
| 饱和度 / 自然饱和度（chroma） | `pipeline.rs::apply_chroma_unbounded` | **GPU** | `spike.wgsl::adjust_global` 尾部 | 在编码域做，和 CPU 同口径 |
| 显示 ICC / 软打样 / EDR | `color/display*.rs`、`render/color_transform.rs` | **GPU** | `spike.wgsl::screen_color` | 三套：矩阵/TRC 快路、采样 CLUT、原生 mBA CLUT（手动三线性） |
| 降噪 Fast（wavelet） | `develop/denoise.rs` + `render/wavelet.rs` | **GPU**（headless compute，失败回落 CPU） | `render/wavelet.wgsl` | 分块 512、halo 30，与整图等价 |
| 降噪 High（BM3D） | `develop/bm3d.rs` | **CPU** | — | 后台任务，不逐帧；优先队列已解决卡首帧 |
| 镜头校正（畸变/TCA/暗角） | `develop/lens.rs::warp_rgb` | **CPU** | — | 一次重采样，折进 neutral 源；**下一个该搬的候选** |
| 锐化 | `develop/sharpen.rs::sharpen_display` | **CPU** | — | 显示域 u16，掉出快速路；小半径卷积适合 compute |
| 动态反差（局部色调） | `develop/local_tone.rs` | **CPU** | — | 需要引导滤波/多尺度，掉出快速路 |
| 几何（裁切/旋转） | `develop/geometry.rs::apply_rgb` | **CPU** | — | 掉出快速路；视口旋转只影响工具覆盖层 |
| 创意 LUT | `develop/lut.rs` | **GPU**（2026-10-31 起；CPU 仍是导出路） | `render/creative_lut.rs` + `spike.wgsl::apply_creative_lut` | 编码域三线性 + 超域残差，见 `implementations/2026-10-31_lut-on-gpu.md` |
| 直方图 | `render/working_preview.rs::histogram_rgb8` | CPU（256px 采样） | — | 只算一次「松手后」，采样 256px，**不建议搬** |
| 导出 / 定稿渲染 | `display/output.rs` | CPU | — | 崔总明确可以接受；真要做 GPU 化是独立项目 |
| 缩略图 / 1920 预览缓存 | `thumbnail/`、`display/output.rs` | CPU | — | 后台批处理，不是手感问题 |
| 视口变换 / 覆盖层 / 对比分屏 | `render/viewport.rs`、`render/overlay.rs` | **GPU** | `spike.wgsl`、`overlay.wgsl` | 三条红线：状态在 Rust、矩阵唯一 |

## 3. 已经踩过的坑（别再踩）

1. **vec3 在 uniform 里要 16 字节对齐**：写成 `f32 + vec3` 会变成 96 字节，wgpu 绘制时才报
   「bound with size 80 where the shader expects 96」。统一用 `vec4`，Rust 侧手算偏移。
2. **绑定组与设备绑定**：设备丢失重建时，layout / buffer / texture 必须整套重建
   （`DeviceResources` + `replace_resources`）。手写重建漏一个就是**打死渲染线程**（图冻住）。
3. **参照帧（对比左侧）不能套创意 LUT/影调**：它是「没有编辑的同一张源」。
   所以主图与参照帧要绑**不同的绑定组**，不能靠全局状态。
4. **CPU 已烘焙的帧不能再在 GPU 套一次 LUT**：`gpu_lut=None` 表达「这张图的 LUT 已经在像素里了」。
5. **f16 纹理是传输格式，不是算错的原因**：GPU 里所有计算是 f32，只有存储是 half。
6. **`max_bind_groups` 默认就是 4**（0..3）。新阶段另开一个 `group(4)` → `create_shader_module`
   直接报「group index 4 that exceeds the max_bind_groups limit of 4」。抬上限等于赌驱动，
   本地桌面软件不赌 —— 把新阶段挂进已有的组里。创意 LUT 就是这么做的：挂进 **group 0**
   （图像绑定组，binding 4..7），顺带让「参照帧不套 LUT」变成结构事实而不是运行时开关。

## 4. 下一步建议顺序（改之前先量）

1. **锐化**（显示域、局部卷积、单趟 compute）：掉出快速路的开关里最没有结构阻力的一项。
2. **动态反差**（局部色调）：需要先把引导滤波搬到 compute，工作量中等，交互收益大。
3. **几何（裁切/旋转）**：其实更适合放进**顶点/UV**而不是 compute —— 视口已经有一份矩阵，
   可能只需要把「编辑后的输出尺寸」喂给 `Viewport`，不必逐像素重算。开工前先设计。
4. **镜头校正**：重采样 + 两次插值，适合 compute；但它只影响 neutral 源（一次性），
   收益是「换镜头参数不用整图重算」，优先级低于上面三项。
5. **导出 GPU 化**：等 1–4 稳定后再谈，别先动。

## 5. 怎么量

- 交互帧：`RenderState.develop_ms` / `decode_ms`（`src-tauri/src/editor.rs`），以及
  `[editor] 真帧：… 解码 …ms 管线 …ms（整条 …ms）` 日志。
- 纯 GPU 绘制成本：`cargo run -p raybend --example color-draw-cost`（`OffscreenRenderer::measure_draws`）。
- 管线各阶段：`cargo run -p raybend --example develop-probe`（ms/MP 口径）。
- 像素正确性：`cargo run -p raybend --example lut-offscreen`（CPU vs GPU 逐点对照）。

## 6. 变更日志

- **2026-10-31**：创意 LUT 上 GPU（`render/creative_lut.rs` + `spike.wgsl` + `gpu.rs`）；
  快速路判据去掉 `lut.is_none()`。此前只要启用 LUT，拖任何滑杆都是 CPU 全画幅重渲。
  离屏对照：CPU vs GPU 最大差 1/255（带 shaper 的样例 3/255），`examples/lut-offscreen.rs`。
  详见 `implementations/2026-10-31_lut-on-gpu.md`。
