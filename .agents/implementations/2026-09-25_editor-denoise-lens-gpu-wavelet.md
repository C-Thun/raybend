# 编辑器降噪、镜头校正与 GPU 小波收口
完成时间：2026-09-25 19:37:23 CST

## 范围与工作区边界

承接本次完整需求：降噪条带重复、镜头库检索及状态、手动镜头算法、BM3D 高质量档与后台缓存、编辑结果直方图、参考帧数据侧，以及追加的 wgpu 小波降噪。保留工作区已存在的 AVIF/editor 和其他任务改动；没有推送、打 tag、制作发布包或混合提交。

原 `plans/M3-W4.md` 的 #7/#8/#11/#12 功能已接通；#13 的本次编译/门禁/记录已完成。原 BM3D 性能目标仍未满足，明确保留在 FUTURE。参考帧这里只交付数据与纹理，不包含 W5 对分线交互。#14 对应改动已在当前工作区参与构建，不需要等提交才能联调。未改原 plan 的 progress。

## 根因与改动

### 降噪条带与算法边界

- `develop/denoise.rs` 的并行输出块推进到了后续行，输入却反复从图像开头读取；改成输入输出逐块配对，并保留全图行坐标。回归可注入 1/2/3/16 个线程，覆盖奇数尺寸、窄图和两支滑杆。
- 旧色度处理在任意非零强度就用低分辨率色度替换原图，造成零点跳变；改成按实际强度混合与保边，验证色度处理保留亮度。低分辨率分析的奇数尺寸分区也统一为面积平均边界。
- 非有限强度和畸形缓冲区安全退回；零强度保持原图逐位一致。
- 快速档现在优先走共享 wgpu 小波；旧 CPU 实现作为设备不可用/失效时的回退，仍保留以上修复与独立单元测试。低于 64×64 像素量的小图使用 CPU，避免 GPU 初始化和传输开销。

### wgpu 小波（本次新增，无 CUDA 依赖）

- `render/wavelet.rs` + `wavelet.wgsl`：四尺度 undecimated à trous B3-spline 分解，线性 Y / B-Y / R-Y 平面；亮度与色度独立 firm threshold，大系数不做固定软阈值偏移。
- 512×512 内部块，加完整 30 像素 halo（2×(1+2+4+8)）；打包 RGB16 存储缓冲区固定以像素宽度计算行距，读回只取块内部。
- GPU 工作缓冲约 25 MiB 上限，不随全图像素数增加；设备与计算管线在进程内复用，调用串行化以控制显存峰值。源图/结果仍为当前 CPU 显影管线的 RGB16，所以有上传和读回。
- 不依赖窗口 surface，编辑器显影线程与缩略图同走 `denoise_fast`。软件适配器不作为生产加速器；无适配器、设备丢失、计算/读回错误使用 CPU。失效设备本进程不循环重试，避免每帧报错。
- 界面快速档显示 GPU 优先与 CPU 回退说明；高质量档明确写 CPU BM3D。设计稿 `design/editor.pen` 和同名说明同步。
- 独立标量参考只在测试里存在；GPU 分块结果在合成边界案例中误差不超过 2/65535。没有把前端作为像素计算入口。

### BM3D、后台和缓存

- `develop/bm3d.rs` 移植 RapidRAW 两阶段实现：DCT 块变换、Hadamard 组变换、硬阈值基础估计、Wiener 最终估计与重叠聚合。
- 修复原参考实现匹配 SSD 单位、边界锚点、零权重窗口、小于 8 像素图像和坐标截断问题；采用粗搜加局部细搜、最多 16 个匹配块、受限线程与带 halo 的块处理。
- `develop/denoise_job.rs`：一个持久后台线程、最新请求优先、取消、单结果缓存。缓存键含解码源代号、尺寸、强度、完整镜头状态；曝光等后续参数变化可复用。重新读同路径也更新源代号。
- 拖动不提交新的 BM3D 工作；当前显示快速结果，松手后计算并换入高质量帧。旧源或旧参数结果不能覆盖当前帧。失败显示状态并保留快速结果。
- 缩略图按编辑栈中的方式调用同一核心算法。当前高质量仍为 CPU，不能把异步任务称为 GPU 加速。

### 镜头库、搜索与当前状态

- 所提两种 Panasonic 12–60（Lumix G 与 Leica DG）、Olympus 12–45，以及 Canon/Nikon/Sony 24–70 已存在于随依赖集成的 Lensfun 数据。
- 根因之一是 lensfun 0.7 的 XML 加载流程没有调用 `guess_parameters()`，不少条目的焦段为 0；初始化时统一解析。手动搜索不再被当前相机卡口裁掉。
- `lens-options.ts`：品牌别名（含中文）、全半角、分隔符和 mm/毫米统一；品牌与数值 token 同时匹配，准确焦段优先，保留不同光圈/代次条目。不会把 2–6 当成 12–60。
- 自动按钮上方持续显示实际选中的配置型号及自动/手动/禁用/未匹配/加载状态。自动模式仍显示识别到的具体型号；无有效焦距时不冒充已应用。
- 没有把仅同品牌同焦段的另一代镜头悄悄当成精确自动匹配；仍可从搜索结果选择具体配置。Lensfun 官方在线数据库可下载更新，本次所提镜头无需另找数据。

### 镜头算法与其他调整

- 原全尺寸镜头坐标直接用在小预览上，导致光学中心和径向尺度错位，表现为斜角拉伸/中心亮度变化。`LensMap::for_image` 按实际尺寸适配 sensor/focal 归一化和光学中心偏移；预览和原尺寸共用同一几何模型。
- Lensfun PA 暗角系数是衰减，校正应取倒数；原实现乘衰减使角落更暗。已修正并加合成平场恢复测试及异常增益保护。
- 手动暗角分为「暗角补偿」和「暗角范围」，中心增益固定为 1，范围控制开始过渡的位置。暗角是径向变化，不局限于四个角像素；范围滑杆让影响区域可控。
- 色边校正拆为「红/青边」和「蓝/黄边」；畸变明确命名「桶形/枕形校正」。新增数值沿用统一参数契约、保存、重置机制。
- 原前端默认值根据滑杆填充原点推算，错误地把新暗角范围默认成 0；改为直接读取契约 default=50，并逐条断言。
- `catalog_0008_lens_channels.sql` 走既有迁移框架，把旧单支色边值 d 迁移成红 d / 蓝 -d，保留旧编辑效果；不另建迁移通道。
- 曲线恒等判定补齐端点横坐标；分段 Hermite 在极值处清零切线，防止超调（Rust 和用于画曲线的 TS 同步）。自然饱和度的输入饱和度限制在有效范围，避免正向调节反而降低异常高饱和区域。

### 直方图与参考帧

- 最终显影帧直接计算当前结果直方图，快速过渡帧和 BM3D 完成帧各自对应自己的 RGB8；复用已有直方图计数逻辑。
- `develop/reference.rs` 根据当前 SOOC/RAW 源生成不含影调/暗角/降噪/锐化的参考帧，沿用当前几何校正确保对齐；按源生命周期、尺寸、几何缓存。
- GPU 按参考帧代号更新参考纹理，换源与设备恢复处理同步；对比交互另属 W5。

## 文件与集成

主要涉及 `crates/raybend/src/develop/{denoise,bm3d,denoise_job,lens,curve,pipeline,params,reference}.rs`、`render/{gpu,wavelet}.rs`/`wavelet.wgsl`、`lens/mod.rs`、`thumbnail/render.rs`、既有 catalog 迁移框架、`src-tauri/src/{editor,lens}.rs`、前端 editor 参数/搜索/面板/状态、语言包及对应测试。新增可重复探针 `examples/{bm3d,wavelet}-probe.rs`；旧 `develop-probe` 补齐新的降噪方式字段。

- 图像缓存管线版本提升到 **10**，三种 `render_sig` 同步；catalog 迁移版本 **8**。
- 软件发布版本保持现有工作区版本；此次交付 Windows debug，不生成发布物。
- 命令/热键评估：既有降噪模式和滑杆属于面板连续输入，复用已有编辑保存/重置/撤销体系，不新增命令或默认热键；拆分后的新通道不创建第二套动作系统。
- `.pen` 通过 Pencil MCP 修改并检查组件截图，无布局溢出；未把设计图截图作为应用 GUI 验收。

## 验证（冒烟）

- `pnpm test`：**925 通过**。
- `pnpm typecheck`、`lint:colors`、`lint:arch`、`lint:i18n` 全通过；`pnpm build` 通过（保留已有大 chunk 警告）。
- `cargo test -p raybend --lib`：**1042 通过、2 ignored**，8.63 秒；GPU 专用 ignored 测试另行显式执行。
- `cargo test -p raybend-desktop --lib`：**80 通过、1 ignored**；真实 RAW 手动探针仍为 ignored。
- `cargo check --workspace --all-targets` 通过。首次发现旧 `develop-probe` 漏 `nr_method`，补齐后重跑通过。
- GPU 显式冒烟：WSL llvmpipe/Vulkan、Windows Intel Arc/Vulkan、Windows Intel Arc/DX12，均通过标量参考、块缝、1 像素/窄图/奇数尺寸、纯色、色度保亮度、降噪量、零点连续性、设备销毁错误回退测试。
- Windows 实际快速档入口的 `develop::denoise` 及 worker 回归：**16 通过**；确认确实选择 Intel Arc GPU。
- `pnpm debug:win` 全流程通过，包含 `check:win` 的 dist 内容/时间与 RAW worker 协议/时间核对。
- Windows 主程序：`C:\rb-target\raybend\debug\raybend-desktop.exe`，构建时间 2026-09-25 19:34:33 CST；worker 19:33:34，含 `raybend-worker-proto-v3`。
- `git diff --check` 通过。全仓 `cargo fmt --check` 存在工作区已有文件格式差异（如 `examples/backfill-metadata.rs`）；未批量改写其他任务文件。新增 GPU 文件单独 rustfmt。

## 可重复性能数据与边界

Windows Intel Arc / Vulkan，优化 dev 构建，合成输入，调用 `wavelet-probe`；以下降噪耗时**包括打包、上传、计算、读回**，不含照片解码/镜头/影调/呈现，不代表 GUI 帧率。

| 输入 | 第一次处理 | 后续两次 |
| --- | --- | --- |
| 1920×1080 | 0.142 s | 0.116 / 0.121 s |
| 6000×4000 | 1.076 s | 0.733 / 0.857 s |

每次新进程初始化计算设备/管线另需 0.638–0.736 s；应用内复用设备。DX12 完成正确性冒烟，表中性能仅测 Vulkan。

CPU BM3D 的本机合成数据耗时：1920×1080 2.931 s，6000×4000 36.241 s。两者是不同降噪算法，不能据此声称等质量加速比。BM3D 全尺寸仍需后台等待。

尚未验证：真实照片噪声/纹理/色彩观感、手动镜头视觉效果、不同镜头/焦距/光圈的数据覆盖、多显示器/DPI 与 GUI 拖动响应。需要在 Windows 实际照片上确认。小波和 BM3D 阈值为手动启发式，尚非相机/ISO 标定的物理噪声模型。

## 来源、后续与环境绕法

- BM3D 实现来源：RapidRAW `src-tauri/src/denoising.rs`，提交 `f00145c11fd57043476574a384be7409a0a18e76`（以 THIRD-PARTY-NOTICES 中核实的完整提交为准），AGPL-3.0；改动与许可已登记。未引入仅供学术用途的 BM3D 软件代码。
- Lensfun 官方数据 https://lensfun.github.io/lenslist/ ，更新入口 https://lensfun.github.io/manual/latest/lensfun-update-data.html 。
- Lensfun 衰减定义核对上游 `libs/lensfun/mod-color.cpp`：https://github.com/lensfun/lensfun/blob/master/libs/lensfun/mod-color.cpp 。
- CUDA BM3D https://github.com/DawyD/bm3d-gpu （BSD-2-Clause）仅登记研究参考；**后续 BM3D 实现方向明确为 wgpu/WGSL 移植**，不是引入 CUDA 或 OpenCV。见 FUTURE.md D3。
- 小波参考资料 https://github.com/darktable-org/darktable/blob/master/data/kernels/denoiseprofile.cl ；本次 Rust/WGSL 是自行实现。
- 本会话默认 sandbox 的 bubblewrap 因 `/mnt/wslg/distro` host mount 在执行前失败；在已授权仓库内通过受审批的 shell 完成读取/编辑/构建，没有修改主机挂载。Firecrawl 搜索未返回有效结果时使用 web 工具与已知上游源码交叉核对。
