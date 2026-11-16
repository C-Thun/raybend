本轮实施记录完成时间：2026-10-04 07:25:35 CST（开发/冒烟交付；完整色彩管理仍有下列闸门及真机验收）

# 色彩管理：浮点/GPU、显示 adapter、配置复用与 RGB 打样

## 结果与完成边界

W1 原契约/设置入口保留；本轮将 W2 输入→显影→代理/导出→XMP 与旧版兼容、W4 资产/默认/单张及批量/预设、W5 RGB 打样接到真实应用路径。W3 Windows 显示 adapter、GPU 输出与 scRGB 已实现，但复杂 CLUT 显示不支持，真机性能/高分辨率内存闸门和真实 RAW/显示色彩仍未验收。不能把“代码接通”记成五波全部完成。V2 通过应用输入显式启用；旧照片/issue 不自动升级。未提交、未推送、未发版；同期 XMP、AI 标签、预设改动未回滚。

Pencil main/editor/export 三份内存稿先行修改、自行审视后实现，用户已授权无需中途设计确认；文件保存仍须用户手动完成。

## 核心决策与复用

1. 固定内置 sRGB/P3/Adobe ICC 字节，SHA-256 身份不受生成时间影响。16 MiB 上限、RGB v2/v4/设备类别等由静态 LCMS 验证。常见矩阵/TRC 走经过 LCMS 网格对照的快路径；独立手工构造 v2/v4 LUT8 CLUT 的输入保留 LCMS，并验证不被伪装成 GPU 矩阵。
2. 工作图平铺 f32 Rec.2020 D65；RGB8/16、灰度、RGB32F/RAW worker v6 不先降 8-bit，负值/>1 保留，非有限数拒绝。旧算子经无截断线性 sRGB 桥复用，尚不宣称全部原生 Rec.2020。RAW 上游 Rescale 的提前截断仍是限制。完整含 alpha 编辑明确拒绝。
3. 高质量降噪把原有单一队列一般化为 u16/f32，共用 BM3D、取消、最新请求、镜头/源/参数键；V2 不再禁用 High。准备中提供 Fast 结果，ready 后复用 high Arc；曝光/曲线更新不重跑降噪或重上传源。
4. 同一 RenderImage/GpuContext 增加 RGBA16F，不建立第二套 renderer。半精度溢出拒绝，CPU/导出 f32。实时全局调整仅生成/上传 192 KiB 小表与 uniform；硬件插值、暗部地址、无色度时直接线性输出、有色度时 decode 表、恒等显示 TRC 省略冗余采样。空间效果继续共用 CPU 回退，过期任务不能落地。
5. Windows OS 互操作均通过 adapter：当前显示目标/ICC、Advanced Color 与 SDR white、设置页跳转、隐藏顶层消息窗监听和可释放订阅。事件只触发重读；有界 watcher 合并通知/generation 丢弃迟到结果。传统 SDR 用应用 ICC；ACM 不重复管理。scRGB 仅精确 Rgba16Float/EXTENDED_SRGB_LINEAR 配对、Windows Composition、有效 OS managed/white 才启用，白点 nits/80，覆盖层/背景同步；设备恢复重新协商。
6. catalog 迁移 16 保存可选固化颜色，app 迁移 10 保存配置资产；复用现有单写者/事务/撤销/哈希/issue。旧空栈继续 schema 1/hash，新色彩用 schema 2，同一 XMP canonical 读写与指纹；显示、打样、设备默认不进便携载荷。撤销到 color=None 确实恢复旧处理；参数发送增加作用域/就绪闸门，初始空载荷和跨照片/库尾样本被丢弃。
7. 资产保留原始字节，批量导入去重/隐藏/恢复；原件移动不破坏引用。ColorDefaults 用 color.defaults.v1 保存，真实内嵌/标记优先，改变默认不追溯改旧照片。ProfileSelect 复用既有 Form.Select，不另建样式体系。
8. 照片选择先后台预检实际 RAW/SOOC 源、身份/相容性/文件签名，再提交共用历史。批量最大 256，固定选择/目标/预检 token；提交前核对所有文件、锁定、编辑栈、当前库，整批一个事务/撤销与 XMP 队列，避免半应用。命名 issue 不重写；资源缺失阻止精确重放/导出。
9. 编辑预设可选色彩组默认不选，保存自动/指定策略及版本而非复制另一照片内嵌配置；整份应用先完整预检。输出目标归导出预设 v4（v1–3 显式迁移），新草稿异步默认不能覆盖手动选择或既有预设。
10. RGB 打样目标经 LCMS 目标编码→工作域双参考后用 112 字节矩阵/开关模拟，相对色度；目标色域警告独立于显示色域。transient 状态、关闭/失败/设备恢复不污染照片；视口“RGB 打样中”标签在面板收起后仍可见。未提供 CMYK/纸白/BPC/感知的假开关。

## AVIF 低成本标记与输出

之前 with_colorspace(Srgb) 改变编码内部模型，2MP 基准约慢 13%/大 17%，未采用。本轮 media/isobmff 容器 adapter 共用现有 XMP 箱体解析/iloc 调整：增加 colr/prof 与主图 ipma 关联，更新绝对偏移，保留现有 AV1 码流及 CICP，不重编码。独立 AVIF 解码器确认 ICC 和解码像素；附加 XMP 后仍一致。非法箱体/截断/重复 ICC/偏移溢出明确拒绝，缓存版本统一 v15。

JPEG、PNG16、TIFF、WebP 可输出 sRGB/P3/Adobe/自定义 RGB ICC，实际转换与嵌入身份一致，非 sRGB EXIF Uncalibrated。AVIF 非 sRGB 组合仍禁用。缩略图、1920 代理、issue/导出与外部编辑 TIFF 使用同一固化输入/处理算子；缺失输入不偷偷假定 sRGB。外部编辑 TIFF 单元逐像素对照共享 f32→sRGB16 并回读 ICC。

## 界面、命令与 XMP

main 全局设置草稿/保存/错误/资产列表；工作空间说明明确写“编辑器色彩页应用输入后启用”，已去除过时的未接入文案；editor 第三组曲线·预设·色彩管理；export 格式后的输出配置，窄窗口沿用右栏内部滚动。Flowbar 无边框 Tabler 齿轮与全屏 maximize 均 16px，跟随 flow 图标；指向低浓辅色，设置打开主色常亮，关闭消失。Pencil 有暗/浅色设置与批量确认稿，局部截图已复检。

统一命令已接打开色彩页、恢复自动、批量恢复自动、打样和色域警告；专业低频动作 defaultKey 明确留空，避免占照片标记键；设置沿用 Mod+,。选择器/保存沿用面板上下文，不再注册重复动作。离屏/Windows 诊断脚本属开发工具，不是产品命令。

XMP 同波接好：固化输入、版本、资源身份随现有 rb 编辑载荷读写；schema 1/2 与旧 hash 兼容。批量操作排入原有 sidecar 写队列；预设/显示/打样/全局默认边界清楚。未复制 ICC 文件进入 XMP，资源按哈希引用，缺失可见。

## 性能证据与闸门

- 早期 2MP 常见 ICC 输入约 19ms、输出约 50ms；LCMS 参考约 388/430ms（平台/负载不同不可混比）。CPU 浮点全局调整约 26–31ms，旧版约 12–13ms，因此不把 CPU 全局参考用于每帧实时处理。
- color-draw-cost 复用同一 1920×1080 目标，5 帧热身/40 次，逐次提交完成计时；排除源上传、ICC 准备、回读和每帧目标分配。它测着色器+提交，不能称产品窗口完整帧耗时。WSL adapter 实际为 Vulkan llvmpipe CPU 软件 GPU；隔离负载后旧版约 3.8ms，V2 复杂色度/ICC/警告约 26.7ms，不能宣称无成本。后续恒等 TRC 优化需以后面的最终结果为准。
- Windows DX12 / Intel Arc 集成 GPU（驱动 32.0.101.8243），debug optimized 构建、1920×1080、同一渲染器。两轮复杂组合 p95 2.9–3.5ms；ICC 本身相对全局调整增加有限，旧 sRGB8 加 P3/Adobe ICC 在基准波动内。准备与绘制分开计时，首次源打包/上传 35–48ms、全局表准备约 4.7–5.4ms/上传 0.8–1.0ms，配置失效时准备/上传约 1.4–2.0ms。这些工作不属于每帧重复 ICC。基准顺序受频率/系统负载影响，以下完整保留第二轮，不用个别负耗时差宣称优化倍数。

| 2MP Windows DX12 路径 | median ms | p95 ms |
| --- | ---: | ---: |
| 旧 sRGB8 | 1.873 | 2.012 |
| 旧 sRGB8 + P3 ICC | 1.655 | 1.891 |
| 旧 sRGB8 + Adobe RGB TRC | 1.921 | 2.123 |
| V2 仅源图 | 1.783 | 1.910 |
| V2 全局调整 | 2.630 | 2.764 |
| V2 全局 + P3 ICC | 2.381 | 2.694 |
| V2 全局 + Adobe TRC | 2.786 | 3.059 |
| V2 全局 + ICC + 打样警告 | 2.500 | 2.675 |
| V2 色度/全局 + ICC + 打样警告 | 3.266 | 3.385 |
| V2 色度/全局 + scRGB FP16 | 3.318 | 3.463 |

该量化结果未显示 2MP 硬件绘制的明显卡顿级成本，但不是产品窗口完整帧耗时/体感验收。跨屏、DPI、ACM/白点与高分辨率峰值内存仍待人类，不能据此自动升级旧照片。

- 独立 LUT8 CLUT 2MP 单次 CPU 输入最终重测 392.64ms（并行构建时 741.02ms），矩阵/TRC 快路径约 19ms；只在后台源载入发生，不进入每帧呈现，但大图载入成本不能忽略。复杂 CLUT 后续优化须保留参考误差/签名验证。
- AVIF 容器 ICC 最终重测 4.69ms，2,035,960 → 2,036,561 B（+601 B，约 0.03%），AV1 数据不变。对应旧编码 4.72s，容器附加约 0.1% 的单次编码时间；不是实时计算成本。不同试验中的重编码耗时受负载波动，不复用它来宣称绝对速度收益。
- 按对象足迹，单张 f32 工作图约 12 B/像素，半精度打包 8 B/像素，GPU 源 8 B/像素，参考/空间缓存另加；60MP 单组约 1.56 GiB，叠加解码/参考/空间结果可能数 GiB。已减少无用副本和旧图重叠，但尚无真实 60MP 峰值记录，不设拍脑袋过期缓存。高分辨率 full tier 按原策略惰性请求。
- 默认不自动迁移旧照片/旧 issue。复杂组合的真机性能和内存没过门槛前，V2 保留显式入口，不用提前 clamp/降成 8-bit/错标掩盖。

## 已验证（Agent 单元/冒烟）

- cargo test --workspace：核心 1425 passed / 10 ignored；RAW worker 7；桌面库 108 / 1 ignored；桌面入口 2，doc 2；零失败。core 合成测试 29.73s 是本轮与构建并行的全量运行，新增重负载性能检查均 ignore。
- cargo check --workspace：通过。
- pnpm typecheck、pnpm test（1203 passed）、lint:colors/arch/i18n、pnpm build：通过。构建仍有现有 chunk 大小提示，不是本轮错误。
- ICC 独立 v2/v4 LUT8、矩阵/TRC 数值、固定指纹；降噪 f32 范围/取消/缓存；批量事务/撤销/旧栈/锁定拒绝；命令/DTO 双侧一致/初始空载荷和跨照片迟到样本回归：通过。
- GPU 离屏在 WSL Vulkan llvmpipe 与 Windows DX12 Intel Arc 均通过：sRGB/P3/Adobe 暗部/边界/色域外、全局调整上下限/组合及 RGB 打样与参考最大误差 1/255，scRGB FP16/SDR white/广色域范围及设备恢复通过。
- 最终独立端口/CDP UI 冒烟（1431/9431）：通过、problems=[]，齿轮 16px/无边框/开启常亮/关闭恢复与密度回归通过。Windows 主程序/四个前端资源/v6 worker 核对通过，原生进程数据底座和主窗口 ready；日志有既有库离线的明确反馈，未冒称真实照片渲染通过。测试启动的进程按 PID+路径+创建时间确认后已关闭。AVIF/CLUT ignored 基准通过。中途因共享 Vite/CDP 端口碰撞发生白屏/target navigated，不当作成功；改用专用 1431/9431 重跑。Windows 一次构建后因后续测试源码 mtime 导致 worker 新鲜度失败，按纪律重建，不能只改检查器。

- 新增 lcms2 6.2.0/lcms2-sys 4.0.7（vendor Little CMS 2.19.0）与 half 已登记 THIRD-PARTY-NOTICES；许可证生成器收录绑定/LICENSE 与 vendor/LICENSE，598 项分发清单生成通过，无新增原文缺失；已有三项 SPDX 补足问题保持原记录。最终 Windows 产物已重建并核对，包含最新许可清单及修正后的启用说明。

## 遗留与 Astro 可接手的具体问题

1. 传统 SDR 中需要应用执行的复杂 LUT8/LUT16/CLUT 显示配置现阶段明确不支持 GPU；输入/离线仍走正确 LCMS，软件不假称屏幕已管理。下一步需要独立真实 ICC、3D 插值/域/暗部与越界定义、LCMS 误差预算及硬件 A/B；不能仅按标签抽出矩阵。对应 CM-W3 未验收，不挪 FUTURE 偷改范围。
2. Windows 2MP GPU 绘制已取得通过数值预算和 <3.5ms p95 的量化证据；WSL 软件 GPU 复杂组合成本、CPU 空间回退和高分辨率峰值仍需要代表性照片基准。保留可重跑 color-draw-cost、color-offscreen、check-color-win.mjs 与 ignored 单次 ICC/AVIF 基准；无法通过的项据具体数据交 Astro，未自行给其它会话发消息。
3. rawler Rescale 提前裁剪需真实 RAW 对照，不能凭合成数据宣布高光恢复正确。缺少真实 RAW/ICC 与显示器验证条件时保持限制并交统一验收。
4. 本轮外：完整 alpha 编辑、DCP/自定义 RAW 风格、CMYK/纸白/BPC/感知打样、HDR 照片输出、硬件校准、macOS/Linux 实装、原生浮点 browse/fullscreen、每机型默认覆盖。FUTURE C1 记录触发条件；当前 browse/fullscreen 是明确标记的 sRGB 代理交 WebView 呈现，不宣称原生浮点。
5. 人类仍需手动保存 Pencil；真实照片、广色域显示、跨屏/DPI/ACM、实际流畅度与高分辨率内存统一验收。没有 GUI E2E/颜色体感的 Agent 自证。

## 涉及文件

核心 color/*（含固定 assets/color/*.icc）、develop/{sample,working,denoise_job,bm3d,lens,denoise,local_tone,sharpen,curve,lut,pipeline,reference}.rs、render/{image,gpu,overlay,color_transform,tone,proof,working_preview,output_space,spike.wgsl}、media/isobmff.rs、thumbnail/render.rs、display/output.rs、export/*、external_editor/mod.rs、store/{develop,issues,color_batch,color_profiles,migration,migrations/*} 与 XMP canonical 接缝。

外壳 editor/editor_working、color_profiles/color_batch/color_system/color_system_events、thumbs/issues/export/external_editor/develop/contract/lib；前端 api/color、共用 ProfileSelect、GlobalSettingsDialog、FlowBar、editor 面板/批量窗/store/actions/命令/预设、editor/export workspaces；design 三份、cm-w2～5/主题规格、PLAN/FUTURE 和本记录。scripts/check-color-win.mjs 与 Rust examples 是重复执行的诊断入口。

Windows 行为依据：[Advanced Color ICC profiles](https://learn.microsoft.com/en-gb/windows/win32/wcs/advanced-color-icc-profiles)、[DirectX HDR / scRGB](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range)、[WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)。

最终证据日志（本机会话临时文件，关键数据已写入正文）：/tmp/raybend-cm-workspace-tests-final3.log、workspace-check-final3.log、ui-tests-delivery.log、ui-smoke-own-final.log、windows-delivery.log、windows-gpu-cost-quiet.log、clut-bench-quiet.log、avif-container-bench-quiet.log、licenses-final.log（以上除首项外均同 raybend-cm- 前缀）。最终 Windows debug 为 /mnt/c/rb-target/raybend/debug/raybend-desktop.exe；worker 同目录。无 commit/push/release。
