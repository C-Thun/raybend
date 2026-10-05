# CM-W3：显示 adapter、GPU 呈现与实时成本

> 2026-10-04 已接通矩阵/TRC、受验证的复杂 RGB CLUT 显示与 Windows 双路径；工程离屏与内存数据见 `cm-w3-hardening.md` / 核心攻关实施记录，整窗交互和视觉仍待崔总验收。契约见 `cm-w1.md`，证据见本轮实施记录。

屏幕变换属于当前窗口，不进入编辑栈、issue、预设、XMP 或缓存。系统互操作通过 ColorSystemAdapter；GPU 只接已准备的策略与小型资源。

## 实现

RenderImage/GpuContext 沿用同一渲染器、视口、对比与设备恢复；增加 RGBA16F 工作图，负值/>1 保留，有限 half 溢出拒绝，CPU/导出仍 f32。源和参考图由 Arc/帧身份复用；无空间处理时共享 f32 源，8MiB 行块转换/上传，不常驻整图 half，主图/参考图可共享同一 GPU 纹理。后台单 worker 准备主/参考源，完整准备后原子提交 tone/尺寸/状态，旧完整帧可在上传期间继续绘制；latest 与设备 generation 阻止迟到提交。

全局调整使用 192 KiB 表与 uniform；硬件线性采样、暗部密集地址、无色度时直接输出线性值，避免逐像素多次幂运算。参数更新不重新上传原图，镜头/降噪不变时复用空间结果。高质量降噪共享后台取消和最新请求判定；局部反差/锐化/LUT/几何仍为 CPU 空间回退。

显示矩阵/TRC 同源于 LCMS 参考；配置失效时准备/上传。暗部 sqrt 地址与边界/色域外测试；sRGB/P3 的近似恒等 TRC 经输出误差预算后省去重复采样，Adobe/其它真实 TRC 保留表。复杂 CLUT 优先保留受支持 mBA 的 B/矩阵/M/原生 CLUT/A 阶段，FP32 节点避免曲线放大量化；其余尝试有界 PCS 33/65/129³ 表。每份配置对照 LCMS 完整相对色度变换，预算不满足则明确回退并提示原因。mft2 Lab 使用 LCMS 旧编码的有效域，工作 RGB 不提前夹到 0..1。准备结果按原始 ICC 身份最多 8 项/48MiB 复用，失效仍重新读盘，generation 规则不变。

Windows adapter 查询窗口当前显示目标，监听窗口移动/DPI/焦点恢复及 WM_DISPLAYCHANGE/SETTINGCHANGE/DEVICECHANGE/POWERBROADCAST。有界 watcher 合并更新，generation 丢弃旧结果，销毁时取消。传统 SDR 只执行应用 ICC；ACM/Advanced Color 交付标准空间且不再次套显示 ICC。

Windows DirectComposition 的 scRGB 只在 OS 管理状态、有效 SDR white，以及 Rgba16Float + EXTENDED_SRGB_LINEAR 精确能力配对全部成立时启用；其它情况明确采用标准 sRGB。白点按 nits/80 缩放，SDR 亮度约束保留负通道/广色域；覆盖层和背景一致缩放。设备恢复重验能力，格式变化沿用同一资源重建，不另建 renderer。

## 性能闸门

测量分离源打包上传、配置准备、参数上传和绘制；color-draw-cost 复用目标，不包含离屏回读或每帧 ICC。WSL llvmpipe 为 CPU 软件 GPU，不能代表 Windows 硬件。此前 Windows DX12 Intel Arc 的 2MP 约 3.4ms p95 是旧一轮 tone/矩阵ICC/proof 的 draw+submit，不是整窗 E2E，也不是本轮原生 CLUT 数据。本轮官方原生 CLUT+tone 两轮 median 4.59–5.68ms / p95 8.39–10.31ms；加 proof 的尾延迟更高，旧路径同期也有明显尾抖动，不能据此宣称稳定整窗帧预算已验收。

60MP 独立进程的实际主存峰值与同步对照见核心攻关实施记录。同步上传在 Windows 仍曾阻塞约 2.69s，因此最终改为后台有界准备/完整帧提交；真实 DX12 两轮后台上传约 3.10–3.43s 期间旧完整帧继续绘制 63/75 帧，dispatch/poll 为微秒级、最后 commit 约 2.14–3.69ms。该并发探针包含回读，其 draw 耗时不等于正常呈现。初始化和设备恢复的资源重建仍同步；旧/新源交叠有必要资源成本。保留显式 V2 入口，不自动迁移旧照片；不能宣称“无性能代价”。

## 验收

- [x] float 范围、旧 sRGB、对比与设备恢复单元/离屏回归通过。
- [x] 常见 GPU 显示与 LCMS 在暗部/边界/色域外误差 ≤1/255；不支持的复杂配置显式回退。
- [x] adapter 状态失效、迟到结果、ICC/ACM 互斥、surface 协商与 scRGB SDR 数值有测试。
- [x] 复杂 CLUT 原生/PCS 路径、独立 LUT8/LUT16/mAB/mBA、实际官方 sRGB v4 displayclass、尖锐曲线拒绝及 Windows DX12 离屏通过；性能与 60MP 独立进程资源测量见核心攻关记录。
- [x] 设置/editor 共用实际呈现状态、本地化和诊断；系统事实与实际输出分开；中英组件/UI冒烟及 Windows 配套产物核对通过，见 `cm-w3-status.md` 和首版收尾实施记录。
- [ ] 整窗交互性能、多屏切换、实际显示色彩由崔总验收；离屏 draw+submit 不能替代这些结论。
- [x] 最终质量门/Windows 冒烟见实施记录；多屏/ACM/DPI/照片视觉与性能由人类统一验收。

Pencil 已连接并完成相应设计，手动保存由崔总完成。browse/fullscreen 使用标记 sRGB 代理交给既有 WebView 呈现；本轮不宣称它们已获得原生浮点 viewport 的能力。

本轮复杂 CLUT 是**显示**能力；复杂 CLUT 软打样仍不支持，不能据此勾选。RAW 新旧标度身份和 XMP/issue 不变契约见 `cm-w2.md`。
