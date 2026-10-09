# raybend — 远期方向登记册（memory/FUTURE.md）

> 本文件登记**想做但要等、要先评估、或明确暂不做**的方向。**不做排期**。
> 排期在 `memory/PLAN.md`；纪律与关键真相在 `AGENTS.md`。
> 任何新增顶层框架或大型依赖，先登记到本文件再讨论。
>
> 建立时间：2026-09-15 00:18:03 CST

---

## 已进入排期的方向（2026-09-30 崔总明确要求摘出）

色彩管理、XMP 与相片整理上期的**顺序、范围、现状和工作波次**统一见 `memory/PLAN.md` §4；相片整理下期AI已于2026-10-04完成 TinyCLIP 定案、数据/任务/来源纠错/UI/XMP 与默认 CPU 模型资源接线，并通过开发及 Windows worker 冒烟；真实图库/GUI 与安装器仍待验收；见 `specs/photo-organization-phase2.md` 与 `specs/photo-organization-ai-w1.md`；本文件 E 节保留选型与后续蒙版方向。
本文件不再复制一份三条主线清单。崔总本轮要求色彩管理现在开工，已纳入该处排期。
本文件 C1 仅保留未纳入这轮的色彩方向及其触发条件，避免把已开工任务同时挂在 FUTURE。

---

## 🚀 Future Release：发版前必须完成（崔总 2026-10-01 设立）

> **定义**：本板块只记**下一个 release 发出去之前必须完成**的工作。这里不做排期细化，
> 每条只做指针与一句话状态，详情归各自 spec/登记项。四条主线的主体均已交付或在做，
> 这里收的是**收尾与验收**。
>
> **2026-10-04 崔总确认**：**XMP 与 editor presets 已完成，待验收**；当前继续完善的是**色彩管理与文件管理**。
> 等这两项完善并验收后，**最后做 CI 和发版**，由崔总在其它会话推进；本会话不执行 CI 改造或发版。

| # | 事项 | 一句话状态 | 落点 |
| --- | --- | --- | --- |
| 1 | **editor presets** | **已完成，待验收（崔总 2026-10-04 确认）**；审计发现的问题已于 2026-10-07 逐条修复并补回归（等高容器 / 选中收敛 / 名称上限 / 库串行 / 版本契约 / 错误区分 / rev 单次 / 应用前重读 LUT），真机验收待崔总 | `implementations/2026-09-30_editor-presets.md`；`implementations/2026-10-04_editor-presets-audit.md`；`implementations/2026-10-07_editor-presets-audit-fixes.md`；`design/editor.pen` |
| 2 | **文件管理与相片整理完善** | **正在完善（崔总 2026-10-04 口径）**；已交付的相片整理上期（含跨库多选导出）仍待 Windows 真机交互与大库性能验收 | `specs/photo-organization.md`；`memory/PLAN.md` §4；`memory/FINISHED.md` §11 |
| 3 | **XMP sidecar** | **已完成，待验收（崔总 2026-10-04 确认）**；本轮审查缺陷已修复并补回归，Windows 真实照片写出/读回与回收站链路待崔总验收；范围仍按冻结规格 | `specs/xmp-sidecar.md` §10；`implementations/2026-10-04_xmp-audit-fixes.md` |
| 4 | **色彩管理第一版验收** | 首版开发、核心攻关、共享显示状态及 Agent 冒烟已完成；整窗性能、真实色彩/多屏待统一实机验收；不自动升级旧照片 | `specs/color-management.md` / `implementations/2026-10-04_color-management-ready-for-acceptance.md` |
| 5 | **发版 CI 化** | tag → 自动构建 NSIS+MSI → 填 GitHub Release 下载资产；**官网下载地址用前端 JS（XHR/fetch）实时取 `/releases/latest`**（action 零 commit）；**色彩管理与文件管理完善、验收后最后推进；崔总在其它会话处理** | G24（查证与适配点在那里） |
| 6 | **国内官网镜像** | 腾讯 EdgeOne Pages + `rb.cthun.com`（大陆节点；**备案前提已满足**——崔总 2026-10-01 确认已实名+已备案）；与 #5 同一套取版逻辑；EdgeOne 建站/域名接入属人类动作 | G24 补充小节 |
| 7 | **官网占位内容切真实版本** | 下载/教程从占位切到真实版本信息（与 #5 的取版脚本一体完成） | `website/`（M5-W3 遗留） |
| 8 | **spike 诊断页二选一** | 发版阻塞：从发布包摇掉或保留，**不许悄悄进包** | C6 |
| 9 | **MSI 链路实际验收** | G22 脚本已实现；真机 MSI 生成/安装/升级/卸载待崔总验收 | G22、`specs/release-windows-msi.md` |
| 10 | **应用内回收站** | **必加（崔总 2026-10-09 定）**，形态待崔总规划后再写规格；⚠️ 与既有「删除 = 移到系统回收站」（`AGENTS.md` §2.3）是两回事：那条是删除动作的目的地，这条是应用内可见/可恢复的回收站形态 | 待规划（规划后转 `specs/`） |

---

## 🧊 发版后（after release）可立即做（崔总 2026-10-09 设立）

> **定义**：本板块记**发版之后马上可以动手**的项目 —— 不阻塞 release，但已经想清楚、成本可控。
> 与上面「发版前必须完成」的区别：那些不做不能发，这些是发完就有空做的。

* **高光改「luma 比值回贴」+ 高亮去饱和**（对照 RapidRAW `apply_highlights_adjustment`）：
  负向高光也从逐通道曲线改成「按亮度比值回贴」（严格保色相），并在 luma 进入 >1 区域时向白点去饱和。
  收益：压高光不出彩边、极端亮色更稳；成本：中等（要动显示链的颜色回贴方式）。
  现状：正向高光已有近似（`neutralize_positive_highlights`），负向仍是逐通道。
  落点：`crates/raybend/src/develop/pipeline.rs`。评估依据：本会话 2026-10-09 的 RR 对比报告。

---

## 库与磁盘的一致性：进目录「清点人头」（已排期 M4-W1）

**人类 2026-09-18 提出**：浏览侧进某个目录时，要拿 catalog 里记的文件数据与磁盘上实际的文件**对一遍**——
库里多了（文件没了）就标记缺失，库里少了（磁盘有、库里没有）就补上缺失文件的数据；
导入侧将来同理（有了缓存，进目录时也要清点）。

为什么值得单独做：这是「catalog 与真实文件库不漂移」的**唯一主动机制**——
否则用户在外面删/加文件后，界面会一直显示旧状态，而用户会以为是软件坏了。

已知约束：清点必须在**后台**做（大目录 IO 慢），且要能在库离线/盘被拔时安全降级；
`asset_files` 已有 `missing_since` 字段（迁移里预留），就是给这件事用的。

**2026-09-20 路线重排**：此项已从远期登记提升到 `memory/PLAN.md` 的 **M3-W1**，作为导出闭环前的
数据可信度门槛。实现口径仍遵守 catalog 不物理删缺失资产：磁盘少的补入，catalog 多的标记缺失。

## A. 运行时與框架迁移

### A1　Solid 2.0 迁移

* **现状**：Solid 2.0 仍是 RC（`2.0.0-rc.8`），配套路由 `@solidjs/router` 2.x 还在 `next`（`2.0.0-next.24`）；生态（组件库、虚拟列表）尚未跟进。
* **收益**：Solid 2 把 async 变成响应式图的一部分，对「大网格里缩略图异步流式进入」「筛选切换时的并发渲染」这类场景直接有益；同时核心吸收了原属于 SolidStart 的职责。
* **触发条件**：Solid 2.0 GA + Kobalte/Ark UI 与虚拟列表方案公布兼容版本。
* **迁移要点**：先升 `solid-js` 与 `@solidjs/router`；检查 Kobalte 兼容性；检查虚拟化方案；检查 `createResource`/`Suspense` 相关写法。
* **风险**：RC 期 API 仍可能变动；迁移期间生态文档稀少。

### A2　Tauri 3.0 迁移

* **现状**：`3.0.0-alpha.0` 已发布。已知变更：
  * 移除 `wry` / `cef` feature flag 与 `tauri::Wry`，改为显式依赖 `tauri-runtime-wry` / `tauri-runtime-cef`
  * `tauri-build` 不再复制 resources 到 target 目录，dev 从源路径解析
  * 顶层导出重整、MSRV 提升到 1.95、Linux 端迁 GTK4
* **我们从 M0 起就一直遵守的纪律**（见 `AGENTS.md` §6.2）：业务逻辑不进 `src-tauri`；不引用 `tauri::Wry`；不依赖 feature flag；资源路径走 Tauri path API。
* **触发条件**：Tauri 3 进入 beta 或 rc 且社区插件（dialog / fs / process / os / single-instance / updater）同步发布。
* **迁移顺序**：`tauri` + `tauri-build` → 运行时 crate 依赖 → 插件 API 变动 → 打包配置。
* **参考**：[tauri-apps/tauri#15068](https://github.com/tauri-apps/tauri/pull/15068)（运行时 crate 化）、[tauri-apps/tauri#14011](https://github.com/tauri-apps/tauri/issues/14011)（顶层导出整理）

### A3　Tauri CEF 运行时评估（备选与退路）

* **实测证据（2026-09-18，M2-W1 spike）**：透明窗口 + 无边框 + 跨显示器拖动（两侧 DPI 不同）→
  窗口 **「未响应」**（不是崩溃）：日志显示**全程没有 `Resized` / `ScaleFactorChanged`**，
  事件只有 `Focused` / `Moved`；进程 CPU 累计仅 5 秒（等待型，非空转）；
  我们的渲染代码未被调用、锁纪律经查干净 → 卡点在 **wry/WebView2 的窗事件处理层**。
  待做的 1 分钟判定：拿**不透明的主窗口**跨屏拖，区分「透明相关」还是「跨屏 DPI 本身」。
* **动机**：① 若 Windows 上「透明 webview + 原生 GPU 内容」合成出现不可解问题，CEF 提供可控的 Chromium 版本；② 消除 WebView2 随系统升级带来的行为漂移；③ 未来跨平台一致性。
* **代价**：包体积显著增大（CEF 远大于 WebView2 的系统依赖模式）、内存占用上升、构建链更复杂。
* **触发条件**：M0-W1 的渲染 spike 出现阻断性问题，或后期需要 WebView 行为完全可控。
* **参考**：Tauri 3 的 `tauri-runtime-cef`。

---

## B. RAW 解码后端（可插拔候选）

> 设计前提：`raybend-raw` 内做**后端可插拔**接口，其它模块不直接依赖任何具体解码库的类型。
> 解码一律在**独立 worker 进程**中执行（上游明确不保证恶意/损坏文件的安全性）。

| 编号 | 候选 | 许可 | 优点 | 缺点 / 风险 | 状态 |
| --- | --- | --- | --- | --- | --- |
| B1 | **rawler**（上游 [dnglab](https://github.com/dnglab/dnglab)） | LGPL-2.1-only | 纯 Rust、覆盖最广、同时给像素与元数据、自带 DNG 写出能力、可静态链接进 AGPL-3.0 | API 不稳定不遵循 SemVer、无 GPU、官方明说 Windows 未支持、不为恶意文件做防护 | **当前首选** |
| B2 | rawler（RapidRAW fork：`CyberTimon/RapidRAW-DngLab`） | LGPL-2.1 派生 | 含其解码修复（可从 diff 学习/吸收） | 需确认 fork 新增代码的许可标注；跟随 fork 有维护耦合 | 待评估（可能直接采用或只吸收补丁） |
| B3 | [libraw](https://www.libraw.org/)（Rust 绑定 [rsraw](https://github.com/mdegans/rsraw)、[libraw-rs](https://github.com/paolobarbolini/libraw-rs)） | LGPL-2.1 / CDDL-1.0 | 成熟、相机覆盖广、可作质量对照基准 | C++ 依赖、线程模型一般、性能不突出 | 备选/对照基准 |
| B4 | [rawspeed](https://github.com/darktable-org/rawspeed) | LGPL-2.1 | 解包速度极快（Canon/Nikon 尤其） | 只解包不做颜色；C++；集成成本高 | 性能备选 |
| B5 | rawloader（[pedrocr/rawloader](https://github.com/pedrocr/rawloader)） | 宽松许可（需复核） | 纯 Rust、干净 | 覆盖少、维护弱 | 低优先 |
| B6 | [openraw](https://lib.rs/crates/libopenraw) | 需复核（Rust 原生重构版） | Rust 原生、专注文件结构解析 | 生态小、成熟度未知 | 观察 |
| B7 | [zenraw](https://github.com/imazen/zenraw)（imazen） | **AGPL-3.0-only 或商业授权** | 三种可换后端、与 zencodec 生态整合；**AGPL-3.0-only 与本项目许可一致，License 障碍已消失** | 太新、生态小 | 观察（可重新评估） |
| B8 | 平台原生：macOS ImageIO / Windows WIC + Raw Image Extension | 系统组件 | 免费、与系统一致、mac 上质量好 | Windows 侧质量一般且依赖商店包；不可控 | 远期（mac 阶段） |
| B9 | Adobe DNG SDK | 专有 | 最省事 | 与 AGPL-3.0 不兼容 | **排除** |
| B10 | 自研解析器 | 自有 | 完全可控、可针对性能优化 | 工作量巨大、相机覆盖是长期苦役 | 长期（仅在 B1/B2 出现阻断问题时考虑） |

**补充方向**：DNG 归档功能（把老 RAW 批量转 DNG 并嵌入原图）——rawler 本身是 DNG 写出器，这是少见的能力，可作为「档案化」卖点，但属于编辑/归档里程碑。

### B-现状：已知未支持的解码 / 去马赛克（2026-10-05 登记，**不着急**）

> 来源：`implementations/2026-10-05_orf-internal-preview.md` 的 11 品牌样张矩阵（样张在 `/mnt/c/src/tmp/raw-samples/`，
> 全部 CC0 来自 raw.pixls.us，id 清单在该记录里）。
> **预览不受影响** —— 网格/胶片带能出图（走 `media::tiff` 的有界内嵌预览提取器，不依赖 rawler 的机型库）；
> 下表只影响**完整解码**（编辑视口 / 1:1 / issue 渲染）。
>
> **处理时机**：不单独排期；等 B2（RapidRAW fork）/B3（libraw 对照）真正提上来时一并做，或上游补了就直接受益。
> 新机型永远会领先于上游机型库，这是常态而不是 bug（rawler 0.8.0 是当前最新版，`orf.rs` 等与上游 main 逐字节一致）。

| # | 现象（实测原文） | 样本（raw.pixls.us id） | 影响面 | 出路 / 判据 |
| --- | --- | --- | --- | --- |
| B-1 | **Sigma X3F / Foveon 不能解码**：`X3fDecoder` 有实现但机型库为空，且 `raw_metadata` 是 `todo!()`（被调用会 panic，靠 worker 隔离兜住） | `sd Quattro` #6756 | 仅 X3/Foveon（sd Quattro、dp 系列）。**Sigma fp / fp L 是 Bayer + DNG，正常** | 口径已定（`memory/FUNCTION-IMAGING.md` §1.2）：**可以不支持**。真要支持不能只补机型库 —— 那个 `todo!()` 得先变成真实元数据 |
| B-2 | **Nikon HE / HE★ 压缩不支持**：`NEF compression Some(HighEfficency) is not supported` | `Z 8` #6616（文件名标 Lossless，但实测走 HE 分支 —— 以解码器报错为准） | 新尼康的 HE 档（省空间模式；默认无损压缩档正常） | 上游补；或从 B2 fork 里吸收补丁 |
| B-3 | **Sony arw6（新式压缩）机型库缺 mode 条目**：`Unknown camera, model 'ILCE-7M5', make: 'SONY', mode: 'arw6'` | `ILCE-7M5`（A7 V）#8846 | A7 V 一代起的新压缩档（A1 II / A9 III 等同理） | 上游机型表；我们这边只能等/换后端 |
| B-4 | **Hasselblad CFV 100C 不在机型库**：机型串带快门模式 `CFV 100C/Electronic Shutter` | `CFV 100C` #7782 | CFV 100C / X2D 之后的新哈苏 | 同上；**预览已经能出**（strip 提取修好了），只缺完整解码 |
| B-5 | **DNG 元数据尺寸取到缩略图**：Ricoh GR IIIx DNG 报 `160×120`（真尺寸 6000×4000 在 SubIFD）；CR3/RAF 报 `0×0` | `GR IIIx` #5818 等 | tile 比例、看图档位判定（不阻塞出图） | 归 `media::meta`：DNG 优先取 `NewSubFileType=0` 那个 SubIFD 的尺寸（或 `DefaultCropSize`），CR3 走 ISO-BMFF 的 `ispe`。独立小活，未排期 |
| B-6 | **两个容器变体没接进扩展名表（优先级低）**：`media::kind` 的 RAW 列表里没有 `.ori`（奥林巴斯高分辨率）与 `.fff`（哈苏），这两类文件会被当作 `Other` 跳过；上游 rawler 的格式表里两者都在 | 暂无样本（需要一个 `.ori` / `.fff`） | 只有这两个扩展名；`.orf` / `.3fr` 正常 | 先拿到真样本再动：`.ori` 是 ORF 结构（预检的 `IIRO` 能过），`.fff` 的魔数待实测确认（若也是标准 TIFF 才能直加）。崔总 2026-10-05 定：**优先级低，不急**。见 `implementations/2026-10-05_readme-and-future.md` |

（B-5 严格说属「RAW 支持矩阵」的缺口而非解码本身，一并放这里免得下次又翻一遍。）

---

## C. 渲染与色彩

### C1　色彩管理余项（基础 SDR 闭环已移 `memory/PLAN.md` §4）

* **本轮排期之外**：完整 HDR 照片编辑/输出、打印布局/驱动与 CMYK 成片、显示器硬件校准引擎、macOS/Linux 色彩系统实装。它们不随“支持 ICC”暗中加入本轮验收；条件分别是 SDR 呈现稳定、打印需求成形、具备校准硬件与跨平台工作启动。
* 高级相机 DCP/ICC 风格全面兼容、打印机打样能力也先留扩展位；基础 RAW 矩阵与照片 RGB ICC 属当前排期。遇到有代表性的真实样本再为这些高级项另立单元。
* 首版只提供 RGB 矩阵/TRC 相对色度打样；CMYK、纸白/黑墨、感知表与 BPC 的验证属于后续打印需求。完整 alpha 编辑、原生浮点 fullscreen/browse 与每机型默认覆盖须另立实施单元，当前入口不能暗称已支持。
* **当前排期的验收不挪到远期**：Astra 本轮已实现受验证复杂 CLUT GPU 显示、精确输入加速、60MP 有界后台上传与 RAW 新旧标度身份；真实 DX12/实际 ICC/真实 RW2 工程数值见 `implementations/2026-10-04_color-management-core-hardening.md`。整窗实时性、真实显示/RAW 观感、多屏仍归 CM-W3 统一验收。复杂 CLUT 软打样并未因显示路径完成而获得支持。
* 已排期部分的唯一主题规格为 [`specs/color-management.md`](../specs/color-management.md)。系统互操作按 `memory/ARCHITECTURE.md` §2.2 的 adapter 约束实施。当前已接入的 ICC 引擎为 [lcms2](https://github.com/kornelski/rust-lcms2)，Windows 静态构建已通过；不要求用户安装系统 DLL。

### C2　HDR / 10bit 输出

* Windows 需要 Advanced Color 与对应 DXGI 色彩空间；wgpu 侧支持有限
* **触发条件**：SDR 流程稳定后再评估
* **2026-09-30 方向澄清**：此处远期指 HDR 照片编辑/输出；HDR 桌面上的 SDR 正确显示，以及服务 SDR 广色域的 scRGB/fp16 呈现属于 C1 的兼容与能力评估，不能全部延后。

### C3　前端渲染演进：**canvas tile（登记为备选，不采用）**

* **为什么现在用虚拟列表而不是 canvas tile**：
  1. **实际瓶颈不是 DOM**。网格在「只渲染可见行 + 固定单元格尺寸」的前提下，DOM 节点数恒定，10 万条与 1 千条的差别只在数据层，不在渲染层。RapidRAW 的网格就是 `react-window` 虚拟列表（不是 canvas），运行在 Tauri 上；这直接说明这条路在同类产品中是成立的。
  2. **快速滚动时的问题不是「没画出来」，而是「解码跟不上」**。无论 DOM 还是 canvas，画布上画的都是已经解码好的位图；解码速度由缩略图尺寸、编码格式与解码线程决定。canvas 不能加速解码，只能减少节点创建与布局开销。
  3. **占位 tile 方案本身没问题**，但它解决的是「已解码位图还没到位」的视觉过渡，不解决「解码队列积压」。真正的解法是：解码离开主线程、用已解码位图 LRU、取消不可见的解码请求、缩略图尺寸与显示尺寸匹配（避免浏览器二次缩放）。
  4. **DOM 带来的能力是免费的**：选中态、键盘导航、框选、拖拽、无障碍、DevTools 调试、CSS 主题。换成 canvas 这些都要自己实现。
  5. **canvas 的收益场景很窄**：单屏需要同时显示数万个单元、或需要像素级自定义绘制（波形图/密度图叠加）。以相片管理器的展示密度，达不到这个量级。
* **因此**：canvas 只在「虚拟化列表实测达不到目标帧率」时才作为优化手段引入；当前已有一个中间方案——网格用 DOM 虚拟化，**覆盖层（蒙版、裁剪、密度图等）用 canvas**（RapidRAW 用 `konva` 就是这个分工）。
* **触发条件**：M2 的 10 万条目实测帧率不达标，且排除了解码与布局因素。
* **参考**：`react-window`（RapidRAW 用法）、`@tanstack/solid-virtual`（Solid 版）

### C5　网格改 tile 尺寸时的重建成本（**已部分优化，剩下的登记在案**）

* **已做**：滑块拖动过程中**不写设置**（原先每动一格一次 IPC + 一次数据库写），改为拖拽结束落盘一次。
* **剩余**：改尺寸会让 tile 流重新分组 → 可见行的行对象整体重建 → `<For>` 按对象身份换 DOM →
  缩略图 `<img>` 被重建（图片本身命中缓存、不重新解码，但布局与绘制是真的）。手感仍不够跟手。
* **可能的做法**（未做，需先测量）：把行高从行对象里挪出去（由 store 提供），让行对象在只有尺寸变化时
  保持身份；或用按键位复用的渲染方式 —— **但不能简单替换**：行种类是混排的（标题行 + tile 行），
  按键位复用会把标题行的 DOM 复用成 tile 行。
* **触发条件**：有人继续抱怨，或 M2 视口完成后一并处理。

### C4　多视口与多窗口

* 双显示器「网格 + 查看器」分离、多文档标签页撕出为独立窗口
* **触发条件**：核心闭环稳定后

---

### C6　wgpu 版本锁定与升级路径（**M2-W1 落定 30.0.1**）

* **现状**：锁定 `wgpu 30.0.1`（上游最新），配 `pollster 0.4` 在渲染线程里跑 async 初始化。
* **记一笔前例**：`AGENTS.md` §3 记着 RapidRAW 把 wgpu **降到 29.0** 以规避 Apple 设备上的 P3 色偏。
  那是 macOS/色彩管理的问题，本项目 Windows 优先、且第一阶段不做色彩管理，所以**从最新版起步**；
  真在 macOS 上踩到再降，且到那时我们本来也要自己接色彩管理（C1）。
* **升级纪律**：分辨率与呈现路径（surface 配置、alpha 模式、device lost 恢复）是 spike 的核心观测对象，
  升级 wgpu 时这些行为要**重跑一遍 spike**（`pnpm spike:win`），不要只看编译过不过。
* **发布前要处理的一件事**：spike 诊断页（`src/dev/SpikeViewport.tsx`，`?spike=1`）是**静态 import**、
  会进产物 —— 这是刻意的（人类要在 Windows 打包版里验透明挖洞与 DPI，而那个产物跑的是 `dist/`）。
  发布里程碑时二选一：① 用一个构建期常量把它摇掉；② 保留（它只有几 KB，且不打开就零开销）。
  **不许**在没做选择的情况下让它悄悄进发布包。
* **已知的 API 变动教训**（30.0.1 实测，写下来免得下次又猜）：
  `Instance::new` 收**值**不是引用；`InstanceDescriptor` 没有 `default()`（用 `new_without_display_handle_from_env()` 或对显式配置调用 `.with_env()` 才会读 `WGPU_BACKEND`）；
  `Surface::get_current_texture` 返回 `CurrentSurfaceTexture` 枚举（不再是 `Result`）；呈现走 `queue.present(texture)`；
  `Device` 上取不到 `queue`；`PipelineLayoutDescriptor` 用 `immediate_size`（不再有 `push_constant_ranges`）且布局要裹 `Some`；
  管线与 render pass 的 `multiview` 改名 `multiview_mask`；`SurfaceConfiguration` 新增 `color_space`。

---

### C9　Windows 编辑视口的显式合成接入（2026-09-25 已确认并固化）

* **真机反馈**：Opaque 修复后同屏内移动几乎不闪；窗体边缘超出屏幕后移动仍稳定复现闪烁。
* **查实的实现冲突**：Tauri 2.11.4 对透明窗口建立 softbuffer，`RedrawRequested` 时向顶层 HWND
  填背景色（默认 0）；softbuffer 0.4.8 最终执行 GDI `BitBlt`。旧产品的 wgpu surface 也直接挂
  同一 HWND，存在两个绘制路径竞争底层内容。不能继续把补帧频率当成合成隔离。
* **已采用方案**：复用 wgpu 30 的 DX12 `DxgiFromVisual`，把 GPU surface 放在 DirectComposition
  visual 上，位于父窗直接绘制层之上、WebView 子窗之下。保留 Rust/wgpu、透明 DOM、现有矩阵与编辑管线。
  不引入新依赖，不改 wry，不需要改用 CEF。
* **已完成命令冒烟**：现有 exe 通过 `WGPU_BACKEND=dx12` 与
  `WGPU_DX12_PRESENTATION_SYSTEM=visual` 启动，实际 Intel Arc / Dx12 / Opaque，出帧与空闲休眠通过。
  崔总随后确认「成功了」。现已将 DX12 + DxgiFromVisual + Opaque 固化为 Windows 产品默认；
  显式环境变量仍可用于诊断覆盖。非 Windows、透明 spike 和离屏工具保持各自原来的默认策略。
* **平台接缝**：`PresentationAdapter` 只选择后端/呈现配置，与 alpha 策略分开；
  macOS/Linux 暂用 `PlatformDefault`，真正移植时再扩展，见 `memory/ARCHITECTURE.md` §2.1。
* **结论**：保留 Rust/wgpu + WebView 分工，隔离平台合成层；不继续用提高补帧频率掩盖同 HWND 竞争。
  此次确认针对本机复现场景，不能外推为所有 GPU/驱动均已通过。
* **参考**：[DirectComposition 层次](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd)、
  [合成器的保留式结构](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components)。
  详细源码证据与验证见 `implementations/2026-09-25_editor-offscreen-composition-investigation.md`。

### C8　编码格式范围：**缓存/快照恒为 AVIF；JXL 只做未来的导入/导出**

> **格式范围清单的唯一事实来源是 `memory/FUNCTION-IMAGING.md` §1**（位图导入 6 种 / 导出 5 种、RAW 只进不出、
> JXL 的定位、issue 快照恒 AVIF、以及「哪些要另接解码器」）。本节只留**决策与代价**，不再抄清单。

* **现状（M3-W3 定，人类 2026-09-24）**：全系统缓存图（缩略图 + 库内大图）统一 **AVIF，
  质量 90、次级采样 4:4:4**（`thumbnail/render.rs` 的 `AVIF_QUALITY` / `AVIF_SPEED` /
  `encode_avif`）。人类实测「AVIF 压缩率确实可以」——比 JPEG q82 小 2.3–3 倍。
* **LUT 封面例外**（2026-09-25/26）：LUT 库演示图按 `design/editor.md` §3.1.1 使用 Q80 WebP。
  2026-09-26 以 `webp` / 静态 libwebp 替换严重丢色的 webp-rust；不改变照片缩略图和 issue 的 AVIF 口径。
* **已知代价**（实测，见 `examples/avif-probe.rs`）：编码慢 6–12 倍
  （网格 384px：62ms vs JPEG 5ms；1920px：539ms vs 100ms）。所以缩略图那条队列
  **必须留在后台**，而且 `image` 的 `rayon` feature（ravif 多线程）是必须的 ——
  不开线程慢 8 倍以上。
* **JXL 的定位（人类 2026-09-24 明确）**：未来支持**导入 / 导出全流程**（等生态成熟：
  纯 Rust 编解码器速度、系统/浏览器支持）；**不用它替换缓存格式** ——
  ~~原设想「JXL 成熟后替换 AVIF 缓存」已收掉：issue 大图快照保持 AVIF 问题不大。
  若真要动缓存格式，触发条件仍是先量一次编码耗时（照 `avif-probe` 的路子），
  别只看压缩率就换（AVIF 那次的教训）。
* **导出侧**：M4 的导出首批就是上述 5 种（`memory/FINISHED.md` §7（M4-W2）），JXL 成熟后作为第 6 种加入。

---

### C7　RAW 出图口径：从「预览/完整解码两条路」收敛到「Rust 按规范渲染 + 缓存」

* **现状（2026-09-17 定：保持现状）**：缩略图与看图共用 `DecodeRequest::thumb`，
  内嵌预览够大（≥ 所需尺寸的 75%）就用预览，否则退回**完整解码**。两条路的颜色与亮度不同
  —— 预览是相机处理的、完整解码是我们的最简处理（无机内风格、无色彩管理）。
  实测观感：双击点开 RAW 时「模糊→清晰」的同时颜色/亮度也变一下。
* **为什么先不收敛**：看图档位要的是清晰度（用户拍板）。把阀值提到 100% 会让看图永远吃预览（糊），
  降下来会让小图也走完整解码（慢）。判据与位置的说明写在 `rawler_backend.rs` 的 `pick_embedded`。
* **真正的收敛点**：**Rust 按规范渲染出位图 → 前端只负责拖拽/缩放 → 渲染结果按 `render_signature` 缓存**。
  这属于 M3 编辑里程碑的原生视口（spike 已验收）；届时「缩略图 / 看图 / 导出 / 入库」共用同一条渲染管线，
  两条路并成一条，缓存键里已经预留了 `render_signature`（含 `pipeline_ver`）。
* **触发条件**：M3 编辑视口产品化（或在此之前有人被这个观感差异咬到两次）。

---

## D. 编辑与显影模块（已提前为第一阶段 M3，2026-09-21 人类定案）

> 原口径「编辑不是第一阶段重点」已被 2026-09-21 再重排取代：**M3 = 编辑（GPU 显影工作台）**。
> 本节保留为技术方向与前提条件登记；波次划分以 `memory/FINISHED.md` §6（M3） 为准，开工前另写详细计划。

### D1　场景参考（scene-referred）管线

* 从 RAW 线性数据开始处理，保证色阶连续（**不是**转成 JPEG 再编辑）
* 关键认知：RAW 编辑必须是「解码 → 线性化 → 场景参考处理 → 显示变换」的完整链路；任何在 8bit 显示参考空间里做的编辑都会损失色阶
* 参考：[darktable 的色彩管线文档](https://docs.darktable.org/usermangement/development/en/special-topics/color-pipeline/)（注意 darktable 是 GPL-3.0+，依 AGPL-3.0 第 13 条可与本项目组合，算法可移植，需保留署名与来源）

### D1.5　管线分期与缓存：拖动时只算看得见的那块（人类 2026-09-24 设想，**未开工**）

* **来源**：人类 2026-09-24 提的优化设想，原话与参考阶段表见 `specs/HANDOFF-2026-09-24-w3-bugs.md` §3。
* **三件事**：
  1. **拖动限流**：拖动拉杆时每 0.2 秒才算一次（现在是每帧一条 IPC + 每帧重跑整张图）；
  2. **只算视口里那块**：拖动期间只对「屏幕上实际看得见的那块像素」跑管线，
     松手后才走全图（全图 → 更新 latest → 预览/缩略图刷新这条链）；
  3. **develop 分期 + 节点缓存**：前期（白平衡 / 曝光 / 高光恢复）→ 中期（去马赛克 / CCM / 色彩空间）→
     后期（反差曲线 / 饱和度）——**靠后的操作不该重算前面的**。
* **现状对应**：decode 已经只做一次（线性源常驻显影线程，**只在换照片时重解**）；
  白平衡 / 曝光确实在 develop 早期（线性域），曲线与色度在后段。
  **2026-09-24 已补上两件**（人类当天追加）：
  * **拖动中只算预览档** —— `render::tier::tier_for_params`：手指还按着时一律 `Preview`
    （1:1 也不做全尺寸），松手那一下按缩放重新算 ⇒ 「释放鼠标才对全图做处理」；
  * **限流两层**：前端按帧合并（只发最后一个值）+ 后端显影线程 `merge_jobs` 把队列里
    被顶替的任务合并掉（不重复算）。
  仍缺：**视口裁切计算**（1:1 时只算可见区域，现在是整张算完再采样）与**分期边界与缓存**
  （现在每帧从头跑整条链）。
* **前提**：这事儿要改的是「渲染线程 ↔ 管线」的接口，**开工会牵动 M3-W5/W6 的排期** ——
  先另立工作单元出方案，别在修 bug 时顺手做。
* **直方图口径（人类 2026-09-24 补）**：直方图必须随调整**实时刷新**，
  **不许**「载入时用快的算法、实时另起一套」——那样快的这点意义也没了。
  唯一计数实现是 `display::histogram::histogram_of_rgb8`（只看 RGB8，不管像素从哪来）；
  文件那条路（初次载入）走像素口，实时那条路的像素由显影线程当前帧直接给出
  （已在内存，不落盘、不解码、不编码）—— 两条只是取像素的来源不同。

---

### D2　去马赛克算法候选

* 候选：RCD（darktable）、Markesteijn（X-Trans，darktable/RawTherapee）、AMaZE / DCB（RawTherapee）
* **许可前提已满足**：darktable（GPL-3.0+）与 RawTherapee（GPL-3.0）都可与 AGPL-3.0 组合（AGPL-3.0 §13 允许与 GPL-3.0 作品组成单一作品），算法可移植，需保留署名与来源说明
* **触发条件**：M3 编辑开工时；在「直接用简单算法（快）」与「移植高质量算法（慢）」之间做取舍

### D3　降噪 / 锐化 / 镜头校正

* 镜头校正数据库：[lensfun](https://lensfun.github.io/)（LGPL-3.0，可经 GPL-3.0 路径与本项目 AGPL-3.0 组合）
* **2026-09-25 已落地**：快速档已接 wgpu 四尺度 à trous 小波（设备不可用回退 CPU 多尺度保边）；高质量档移植 RapidRAW 的两阶段 BM3D（纯 Rust CPU），独立后台、取消与单结果缓存。亮度/色度独立控制；完整相机/ISO 噪声档案与 AI 去噪仍属后续。
* 镜头使用内置 Lensfun 库；已补焦段解析、全库品牌/焦段搜索、预览尺寸坐标适配、PA 逆衰减补偿。手动暗角量/范围与红/蓝通道分开。在线增量更新可用 Lensfun 官方数据库更新源，当前内置库已包含两种松下 12–60、奥巴 12–45 及多家 24–70。
* **当前 GPU 小波**：复用 wgpu compute，亮度/色度独立 firm threshold，B3-spline 四尺度、512 像素分块 + 30 像素完整 halo；编辑器与缩略图共用。无 CUDA/OpenCV 新依赖。噪声阈值仍为手动启发式，后续可引入相机/ISO 噪声标定与按尺度控制。
* **BM3D GPU 移植（2026-09-25 明确后续方向）**：目标是跨厂商 **wgpu/WGSL**，需移植块匹配、协同 3D 变换、Wiener 第二阶段和重叠聚合；复用当前 compute 服务的分块/读回边界，单独验画质与性能。CPU BM3D 保留为参考与后备；合成图实测 1920×1080 2.931 s、6000×4000 36.241 s，仍未达原定 ≤2 s / 全尺寸数秒目标。
* **CUDA 依赖方案只作参考**：DawyD/bm3d-gpu（https://github.com/DawyD/bm3d-gpu ，BSD-2-Clause）已有 CUDA 实现，但直接集成限 NVIDIA 且需 C++/CUDA 工具链。登记在此，当前不集成 CUDA；研究其中 GPU 分组与聚合策略后移植到 wgpu。GPU NLM（OpenCV CUDA）同样不作为当前依赖。小波参考资料：darktable 官方 OpenCL 源码 https://github.com/darktable-org/darktable/blob/master/data/kernels/denoiseprofile.cl （本次小波 WGSL 为自行实现，未复制此文件）。

### D4　局部调整与蒙版

* 线性/径向渐变、画笔蒙版、色彩/亮度范围蒙版
* **点击生成蒙版（2026-10-04 崔总明确）**：EfficientSAM 是优先验证候选，先点选/加减点，再用于局部调整；CPU 可运行，CUDA 不作产品依赖。源码结论与限制统一见 E2，不在这里维护第二套方案。
* 主体/天空自动语义选择与精细抠图仍为后续，与点击分割分别验证。
* 开工时按当前 develop stack / issue / latest 契约设计统一蒙版资产；已采用的像素结果与手动修正属于不可随缓存清除的编辑成果，不能只存 AI 重跑指令。同步对接届时 XMP 资源往返；不提前以旧 M1 schema 假设另造存储。

### D5　色调映射

* AgX / filmic（darktable 的 AgX 是当代画质基准之一）
* 显示变换（display transform）与 C1 色彩管理一起做

### D6　DNG 归档

* 把老机型 RAW 批量转 DNG（可选嵌入原图）——rawler 自带此能力

### D7　联机拍摄（Tethering）

* Windows 上可用 [gphoto2](https://github.com/gphoto/libgphoto2)（RapidRAW 在 Unix 上用它）或厂商 SDK
* **触发条件**：核心闭环完成后，看用户需求

### D8　3D LUT 与胶片模拟（第一版不做，人类 2026-09-25 定）

* **目标**：一次套用「整包观感」（亮度曲线 + 色彩），而不是只改光比 —— 即「模仿某款胶片 / 厂商风格」这类需求。
* **形态**：**3D LUT**。M3-W6c 的 `.cube` 引擎（三线性插值）就是它；**HaldCLUT** 是同一条查表路径的另一种编码
  （把 identity 立方体嵌进一张二维图，常见 512×512 / 4096×4096）。两者**共用一份插值实现**，不写第二套（§2.12）。
* **数据来源候选**（按许可从宽到严）：
  * **自己生成** —— 从「我们的中性渲染 ↔ 相机内嵌 JPEG」成对采样回归出 LUT。**唯一无许可问题的「学官方观感」路径**，
    而且数据就是用户自己的照片。
  * **公开胶片模拟** —— Pat David / pIXELsHAM 的 FreeHaldClut 系列（RawTherapee Film Simulation 用的那套，
    见 [RawPedia 的 Film Simulation 页](https://rawpedia.rawtherapee.com/Film_Simulation)）。许可 **CC-BY-SA 4.0**：
    内容是内容许可、与软件 AGPL 分开管；
    署名与「相同方式共享」进 `THIRD-PARTY-NOTICES.md`，**不得**当软件依赖混编。
  * **商业 `.cube`（用户自带）** —— 走 M3-W6c 已有的导入通道，不需要额外机制。
* **参考实现**：RawTherapee `rtengine/clutstore.cc`（`HaldCLUT::getRGB`：三线性插值 + `strength` 与原值混合）、`rtgui/filmsimulation.cc`。
* **为什么第一版不做**：LUT 是**乘在光比之上**的。基础渲染曲线（`memory/FINISHED.md` §6（M3-W6a））没做对之前，
  LUT 只是把错的光比再染一层色，观感只会更乱。**顺序必须是「先光比、后色彩」。**
* **触发条件**：M3-W6a 的基础曲线落地并被人验收之后，按需求拉回。

### D9　DCP 相机配置文件（第一版不做，人类 2026-09-25 定）

* **是什么**：Adobe 的相机配置文件格式，内含 `ColorMatrix` / `ToneCurve` / `HueSatMap` / `LookTable` /
  `BaselineExposureOffset` 与双光源白平衡插值 —— 是**公开可得里最完整的「官方观感」近似**。
  RawTherapee 自带 130 个（`rtdata/dcpprofiles/`），覆盖佳能 / 尼康 / 索尼 / 富士 / 松下 / 奥巴 / 宾得等。
* ❗**许可红线（不可越）**：DCP 文件**不能再分发**。Adobe 的 Color Profile License 与 DNG SDK 都不允许；
  第三方项目 [camicc](https://github.com/rafaelcgs10/camicc) 的 README 也明确写着「从本地 DNG Converter 提取、
  不要 commit、不要再分发」。因此这条路**只能支持用户自带路径**，**绝不允许打包进安装包**
  （RawTherapee 打了包，那是它自担的风险，我们不跟）。这条红线要同时写进 `THIRD-PARTY-NOTICES.md`。
* **实现成本**：RawTherapee `rtengine/dcp.cc` 是 2000+ 行量级；难点在 `HueSatMap` 的色相-饱和度双线性插值
  （`makeHueSatMap`，源码注释 "Ported from Adobes reference implementation"）与双光源插值。
* **单拎出来说**：`BaselineExposureOffset` **本身就有价值** —— 它就是「RAW 本来偏暗」的那个补偿量（各家能差 ±1 EV）。
  **但它不必靠 DCP 拿到**：同一个量能从内嵌 JPEG 反推，更省事且无许可问题（见 `memory/FINISHED.md` §6（M3-W6b） 的 RAW/直出拟合方案）。
* **触发条件**：① 用 [dcamprof](https://github.com/Beep6581/dcamprof)（GPL-3.0，需 ColorChecker 实拍 + Argyll CMS 测量）
  自己生成 profile 的流程跑通之后；或 ② 用户群体明确要求读写 Adobe 的 DCP。两者都不是第一版的事。

### D10　RapidRAW 参考：暗部局部细节/噪声保护 + HDR 高光压缩（2026-10-09 登记）

* **来源**：RapidRAW `src-tauri/src/shaders/shader.wgsl` 的 `apply_tonal_adjustments`（阴影/黑区）与
  `apply_highlights_adjustment`（高光）。2026-10-09 的高光/黑区算法对比评估认定值得记下、暂不实现的两条。
* **① 暗部抬升的局部细节保护 + 噪声保护**（RapidRaw 暗部处理的王牌）：
  * 用**模糊 luma** 算细节比 `t_pixel / t_blurred`，夹在 `[0.8, 1.25]`，按抬升量放大后乘回；
  * 噪声保护：细节放大量再乘 `smoothstep(0.0, 0.1, t_blurred)` —— 深阴影里的噪点不跟着放大。
  * 解决的问题：**抬黑之后画面发灰、噪点炸**（纯全局曲线做不到）。
  * 我们的落地前提：需要带邻域的算子与模糊图（`develop/local_tone` 有现成设施，但它现在服务
    `dynamicContrast`，不是这一级）；还要决定它落在 LUT 快路径还是融合路径 —— 属独立工作单元。
  * 备注：同一条函数里的**对比补偿**（pivot 0.2 / stretch 1+1.3·lift / 85% 混合）已于 2026-10-09
    落地，见 `crates/raybend/src/develop/pipeline.rs` 的 `BLACKS_COMP_*`；这里只留尚未做的部分。
* **② HDR 高光压缩 + AgX tone mapper**：
  * RR 对 `luma > 1` 走有理压缩 `excess / (1 + excess·k)`，再交给 AgX 显示变换 —— 曝光推爆的高光**还能压回来**；
  * 我们目前 `highlights_curve` 对 `value ≥ 1` 直接早退、后面裁到白点 ⇒ 推爆的区域救不回。
  * 前提：把显示链从 display-referred 改成 scene-referred（与 D1 / D5 同一件事），属架构级。

---

## E. AI 能力

> **当前状态（2026-10-04）**：上期代码与Agent冒烟已完成，Windows真机验收仍待崔总。下期 TinyCLIP 逐类折中阈值、标签来源/文字禁止/任务/默认 CPU worker/获批 UI/XMP/模型资源已接线并通过开发与 Windows worker 冒烟，设置中主动安装后可识别。**真实图库、GUI 与干净 Windows 安装尚待验收；人物漏标与海类误标/漏标仍明确存在。** 当前报告见 `docs/ai/tinyclip-v1/README.md`，原门槛失败实验保留在 `docs/ai/2026-10-04/README.md`；实施状态与验收仍归 `memory/PLAN.md`、`specs/photo-organization-phase2.md`。崔总已指定本期TinyCLIP收尾、SigLIP 2留远期考查；点击蒙版继续独立留未来，不纳入本次。

### E0　相片整理：本地 AI 标签识别（已转实施规划）

* 范围：人、鸟、车、山、海、森林、夜景等有限多标签；manual/ai 分来源、按照片与文字屏蔽/恢复；复用现有标签导航、筛选、导出和累积自动桶，不传播目录标签。
* 使用稳定的**原始 SOOC/RAW 内嵌预览**，不能误用随 latest 编辑变化的网格小图；长边 384 是初测代理规格，最终由模型预处理与效果实验确定。
* 先主动识别明确范围，CPU 完整可用，不自动扫描历史全库；模型缺失不影响手动整理。
* 实施正文和验收只维护在上述 specs，不在 FUTURE 复制另一份阶段清单。人脸/语义搜索/编辑蒙版不随本期扩围。

### E1　推理后端与模型分发

* **2026-10-04 崔总定案：本期只用 TinyCLIP** ViT-40M/32 Text-19M LAION400M，FP32 图像编码器151.53 MiB。逐类阈值折中、偏向减少误标，不再将原拟定 P≥90%/R≥60% 当整期交付硬闸门；实测不足与漏标仍如实披露，不保证真实图库准确率。当前实施正文见 `specs/photo-organization-phase2.md`。
  * https://huggingface.co/wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M
* **SigLIP 2 留远期考查**：`google/siglip2-base-patch16-224`，Apache-2.0，revision `75de2d55ec2d0b4efc50b3e9ad70dba96a7b2fa2`。本次 FP32 图像编码器354.46 MiB，是TinyCLIP约2.34倍；WSL热编码P50/P95约185/193ms，未测Windows。公开样本中鸟类召回更高、人物接近原门槛，但不能据此承诺真实图库或场景收益。本期不安装、不并跑、不继续选型；将来重新考查须比较独立产品定义样本、量化后的效果/体积、Windows CPU资源成本，再决定是否替换默认模型。已有实验留在 `docs/ai/2026-10-04/`。
  * https://huggingface.co/google/siglip2-base-patch16-224
* 运行时方向：Rust `ort` + ONNX Runtime CPU；`load-dynamic` **不代表不用分发 DLL**，应用随包提供固定 CPU 运行库并按绝对路径加载。实验精确版本已固定ort 2.0.0-rc.13 / ORT 1.28.0 CPU / opset17；许可证与NOTICE已归档。默认桌面构建已启用 CPU 推理，固定 DLL/许可与 TinyCLIP 源包进入私有应用资源；Windows debug 核对通过，正式安装器与干净机运行尚待验收。
  * https://github.com/pykeio/ort
  * https://onnxruntime.ai/docs/get-started/with-cpp.html
* 标签词表的文字向量预计算，首版不分发 tokenizer/文本编码器，不增加 `tokenizers` 运行依赖；与未来语义搜索分开。
* 首版模型随应用提供，在设置中明确安装，保留可信离线目录导入；校验/原子安装/损坏修复已接线。在线下载作为后续分发优化，须先发布固定模型制品，首版不依赖下载站点；保留版本与许可；普通安装不要求 Python/PyTorch。设备级 models 目录独立于可删图片缓存，卸载模型不删标签/纠错。
* **排除 MobileCLIP research-only 权重用于产品**：代码 MIT 与 LICENSE_MODELS 是两层，不能混淆。https://github.com/apple-aiml-research/ml-mobileclip/blob/main/LICENSE_MODELS
* **CUDA 不作为照片软件运行依赖**（崔总 2026-10-04 明确）。GPU/NPU 后续可选，CPU 路径始终保留。DirectML 已进入 sustained engineering；WinML 可作后续候选，但必须单独核对 Win10/11 与 Rust 集成。https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html

### E2　点击生成编辑蒙版：EfficientSAM 候选（2026-10-04 调研）

**崔总明确想要的交互**：在图片上点击物体得到蒙版，可进一步加点/减点修正，再用于局部调整。与标签识别分开实现，不要求共用模型；先登记未来，与 D4 整合，当前不实施。

**源码核对结论**：
* 官方实现**使用 PyTorch**：`efficient_sam/efficient_sam.py` 继承 `torch.nn.Module`；`setup.py` 的空 `install_requires` 不能证明无需 torch。README 没列依赖不等于没有。
* **CUDA 不是推理必需项**：官方加载权重用 `map_location="cpu"`，导出脚本提供 ONNX（opset 17）及编码器/解码器分离；产品可用 Rust + ORT **显式 CPUExecutionProvider**。这是源码支持的部署路线，尚未在 RayBend/Windows 上实际跑通或测速度。
* `export_to_onnx.py` 使用 PyTorch 做一次性转换；产品分发转换好的模型，不分发 Python/PyTorch/CUDA。已有共享推理/安装基础可复用，模型与缓存身份独立。
* 先验证 ViT-T，ViT-S 作质量对照。图像编码一次，在同一输入版本上缓存 embedding，后续点选只跑解码器；官方示例 embedding 为 `1×256×64×64`，float32 约 4 MiB（仅特征，不含模型与中间激活）。
* 当前源码 `decoder_max_num_input_points = 6`，超过会截断；界面必须明确点数限制或设计提交一笔后继续新蒙版笔划的策略，**不能无限收点后静默忽略**。
* 预处理实际是缩放到 1024×1024 并归一化（注释的 pad 描述与实现不一致）；必须按代码验证坐标变换，不能套另一种 SAM 的等比补边算法。分离导出中的动态 shape/点数分支、负点及输出张量按实际导出物检验。
* 原生/ONNX 路径输出低分辨率 logits 后插值，不是全尺寸发丝级抠图；默认最多三个候选按预测质量选择，预测 IoU 是估计分数。真实边缘、透明物体与 CPU 交互延迟待实测。

**未来实现边界**：
1. 进入智能蒙版工具才编码，选图变化取消旧结果；缓存绑定 source base（raw/sooc）、图像版本、预处理与模型摘要，绝不复用标签模型的 embedding。
2. 使用当前编辑源的稳定基底和足够清晰的预览，原图坐标存点/蒙版；裁剪、旋转、镜头几何映射沿用 Rust 变换契约，不能由前端猜换算。DPR 与屏幕坐标不进蒙版真相。
3. 点选、负点、候选预览、叠加/减去、确认后局部调整；单次点选可撤销。详细 UI 与热键在开工时由 Pencil 定案。
4. **已采用蒙版是编辑成果**：存确定结果、坐标依据、来源/模型信息和手工修正，不只存“下次重跑 AI”的指令。模型升级/卸载与清缓存不得改变旧定稿；生成中间特征可删。
5. 与 D4 的画笔/渐变/范围蒙版组合走同一蒙版数据与编辑管线；接入时同步评估 XMP `rb:` 编辑资产及资源引用往返，复用届时 sidecar 体系。
6. 无 GPU 的 Windows 真机先测首张编码与后续点击延迟、内存、长宽比/旋转/边缘误差；未满足交互预算之前不承诺“实时”。GPU/NPU 只能是可选加速。

许可：仓库根 LICENSE 为 Apache-2.0；实际采用的权重文件与派生 ONNX 仍需保留来源/许可/修改记录，发行前按具体制品复核。**本轮只核源码，没有安装、转换或跑模型。**

官方证据（2026-10-04 查阅）：
- 项目及演示：https://github.com/yformer/EfficientSAM
- 核心模型/CPU 加载/点数/预处理：https://github.com/yformer/EfficientSAM/blob/main/efficient_sam/efficient_sam.py
- ONNX 转换：https://github.com/yformer/EfficientSAM/blob/main/export_to_onnx.py
- 分离图实现：https://github.com/yformer/EfficientSAM/blob/main/onnx_models.py
- ONNX 推理示例：https://github.com/yformer/EfficientSAM/blob/main/EfficientSAM_onnx_example.py
- 许可：https://github.com/yformer/EfficientSAM/blob/main/LICENSE

主体/天空/背景自动语义选择、精细抠图和一键换背景继续保留为更后续方向；点击分割不自动等同于认识物体名称。

### E3　语义搜索

* CLIP 类模型 + 按需文本编码：自然语言找照片（“海边日落”）。与本期固定词表标签分开，不提前引入向量库或 tokenizer。
* 与 FTS5 的关键词搜索互补，不能替代确定性搜索。

### E4　人脸聚类

* 检测 + 嵌入 + 聚类；未来开工时按当前分层 app.db/catalog.db 存储设计，不沿用过时的 index.db 假设。

### E5　原则

* 本地优先：不上传照片；明确范围、可停止、可恢复，模型资源与图片缓存分开。
* CPU 完整可用，CUDA 不作运行前提；可选加速不能破坏跨显卡基础功能。
* 能力接口、任务、安装机制可共享；标签与蒙版不强求同模型，不默认共享特征。


### E6　命令注册表 → MCP / AI 工具层

M2-W3 已有统一命令注册表：命令 id、标题、适用上下文、可用条件、当前键位与执行入口由同一份数据提供，
commands panel、快捷键设置和标题栏菜单都是它的不同呈现。未来接入 AI 时，可把这份注册表作为能力参考表，
再增加一层**显式、类型化、可授权**的 MCP 工具描述，而不是让 AI 模拟点击界面或另写第二套动作。

注意边界：不是把所有界面命令直接无条件暴露。需要逐条补参数 schema、只读/写入/破坏性级别、当前上下文、
确认策略与可审计结果；批量删除、导出覆盖等动作必须保留人类确认。先从只读查询与可撤销标记动作开始。

* **触发条件**：开始规划 AI 助手 / 外部 MCP 接入时；
* **复用纪律**：MCP、commands panel、菜单、快捷键最终调用同一业务动作，差异只在入口与权限适配层。

---

## F. 平台扩展

### F1　macOS

* **2026-09-27 拍板：暂缓**，等 Windows 版成熟后再评估（memory/REVIEW.md R3-01）。直接动因：签名 $99/年 无免费替代、无 Developer ID 签名的直下在 macOS 报「已损坏」不可用（`docs/release.md` §8）。
* 解码：ImageIO / CoreImage（免费且质量好）
* 渲染：Metal（wgpu 已支持）
* 打包：签名与公证（notarization）、App Store 政策与 AGPL-3.0 的冲突（**注意：AGPL-3.0 与 App Store 条款不兼容**，需走独立分发）
* 路径：NFD 规范化问题（`AGENTS.md` §7.3 已预留）

### F2　Linux

* **2026-09-27 拍板：空闲时先行打包**（人类意向，非承诺；memory/REVIEW.md R3-01）。开发与打包可在现有 WSL 环境完成（`pnpm tauri dev` 本就是 Linux/webkit2gtk 构建），真机/虚拟机仅用于验收；前置项：G11（`tauri.linux.conf.json` 的系统标题栏收尾）。
* Tauri 3 起 GTK4；Flatpak 分发；RAW 解码与色彩管理差异
* **优先级最低**

---

### Windows MSIX / Microsoft Store 分发（2026-09-27 登记；同日拍板为直下链走通后的主攻方向）

崔总定案：本轮 M5 先收口 NSIS/MSI 本地打包 → GitHub Release 安装包下载 → 官网自动更新；MSIX 后续另开工作单元，不要求现在申请商店或提供 Identity 来挡住直下发行。

**2026-09-27 下午拍板（memory/REVIEW.md R3-01）**：官网直下走通后即启动 MSIX，作为「零现金成本解决 Windows 安装体验」的主攻方向——商店重签与开发者账号均已免费（`docs/release.md` §6），代价是 MSIX 包装与双通道工程；macOS 暂缓（$99/年无免费替代）、Linux 空闲时先行（WSL 可构建）。

后续开工输入：真实 Package Identity Name / Publisher、商店账号与认证要求；对接现有 `RAYBEND_DISTRIBUTION=store` 前后端身份，禁用直下 updater。另做 Windows SDK 包装与商店更新流水线，复用现有发布来源/许可/worker 校验，不把 NSIS 改后缀当 MSIX。

验收需覆盖直下版数据迁移、MSIX 数据目录/权限、卸载重装保留库、自重启 RAW worker、WebView2、导入导出目录和外部应用交接。申请/商店身份/实际签名与上架由崔总操作；本轮没有创建 MSIX 包或虚构商店下载地址。相关操作边界见 `docs/release.md` §6。

## G. 其它能力

| 编号 | 方向 | 说明 | 触发条件 |
| --- | --- | --- | --- |
| G1 | 插件/脚本系统 | 用户可用 Lua 或 WASM 扩展（导出规则、自定义元数据面板等） | 核心 API 稳定后 |
| G2 | Windows Shell 集成 | 注册 `IThumbnailProvider` 让资源管理器显示 raybend 渲染的缩略图。**风险**：进程内 shell 扩展，崩溃会影响 explorer.exe，签名与测试成本高 | 发布后 |
| G3 | 云端同步 | 多机同步 catalog 与编辑。**注意**：本项目已是 AGPL-3.0，一旦对外提供网络服务，就必须按第 13 条向使用者提供对应源码 | 不考虑近期 |
| G4 | 视频与 Live Photo | 视频缩略图与时码；大幅增加复杂度 | 相片流程稳定后 |
| G5 | 打印与色彩校准 | 软打样、打印布局 | 色彩管理（C1）完成后 |
| G6 | 多语言 | 从一开始预留 i18n（建议 i18next 或等价方案），首个版本只做中文 + 英文 | M1 UI 骨架时留位 |
| G7 | CLI 与无头模式 | 复用 `crates/raybend` 做命令行导入/导出/校验（这也是分层架构的回报） | 核心库稳定后 |
| G8 | 分发与捐赠 | 发布渠道、捐赠入口、第三方许可声明页 | M5（2026-09-21 起） |
| G9 | **窗口状态持久化** | 记住上次的窗口大小/位置/是否最大化。现在每次启动都是固定 1600×1000（`tauri.conf.json`）；要跨会话记住需写到本地配置（可放 `app.db` 的设置表或 localStorage） | 有人抱怨之后，或 M1-7 收尾时顺手做 |
| G10 | **Windows 11 贴靠布局（Snap Layouts）** | 鼠标悬停系统最大化键会弹出分屏菜单 —— 自绘三键后这一入口没了（Windows 不提供 API 让我们调出它）。补偿方案候选：① 自绘一个近似的分屏菜单；② 用 Win32 侧介入 `HTMENU`/`WM_NCHITTEST` 让系统认得我们的按钮（需要不安全的窗口子类化，与 Tauri 3 迁移纪律有冲突）。**登记原因**：M1-4 去掉系统标题栏时已明确知道丢了这个能力，不是遗漏 | 有实际用户反馈后 |
| G11 | **Linux / macOS 的沉浸式外壳** | `src-tauri/tauri.linux.conf.json` 目前**故意**保留系统标题栏，只为 WSLg 开发期能拖边缩放（用户 2026-09-15 明确允许）。**发布 Linux 版之前必须处理**：删掉该文件或改回 `decorations: false`，并补上 Linux 侧的自绘缩放（Tauri 有 `startResizeDragging`，但需要自己做命中区）。macOS 还要重新评估红黄绿灯与 `titleBarStyle` | 做 Linux/macOS 版之前 |
| G16 | **查看照片：DOM 版已产品化；原生视口随 M3 编辑里程碑产品化** | M2-W2 已完成 DOM 看图、胶片带、2–4 图对比、缩放/位移同步、评级打标与三态 chrome；M2 不再把它替换成 wgpu。原生 wgpu 透明挖洞已通过 spike（M0-2），M3 编辑开工时产品化；届时保持现有状态/交互契约，只替换需要原生像素管线的渲染层。 | DOM 查看器已完成；原生视口随 M3 启动（2026-09-21 重排） |
| G17 | **IPC 类型桥改自动生成** | M1-5 决定**手写 TS 镜像**（`src/api/types.ts`）+ 共享键名契约（`src/api/dto-contract.json`，两侧各有测试对着它断言），不引入 `tauri-specta`。**复评条件**：命令总数明显超出预期（>25 条），或手写镜像出现第二次真实漂移（第一次是 `CacheStats` 漏了 `rename_all`，被契约测试当场抓到） | 命令数 >25 或再次漂移时 |
| G13 | **库文件加密** | 用户 2026-09-15 明确「留到 future」。**预想目的**：① 库放在移动硬盘/笔记本上丢失时，防止他人直接打开 `catalog.db` 看到照片清单、路径、评分与关键词（**元数据泄露**比照片本身更敏感）；② 便于把库放进不信任的存储（NAS、共享盘）；③ 将来若做云同步，加密是前置条件。**注意**：若只加密 catalog 不加密 `photos/`，照片本身仍是明文 —— 要真正防泄露需连带考虑照片容器加密或全盘加密配合。实现候选：SQLCipher（重依赖，需评估 Tauri/Windows 交叉编译）或应用层加密敏感列 | 有实际隐私需求时，或云同步（G3）启动前 |
| G14 | **`photos/` 目录名可配置** | 库根下的落地目录名目前固定为 `photos/`（`memory/FUNCTION-REPOSITORY.md` §1）。有人习惯 `originals/`、`Photos/` 等。做成库级设置需同时处理「改名前后的历史文件仍要能找到」 | M1 之后，有人提出时 |
| G15 | **与 Luclin `Gid` 的互操作** | 用户现有体系用 `Gid`（时间戳 + shard + 随机数 → 定长 base62，上游默认 18 位）生成唯一 ID。raybend 的库 ID 取的是该方案的**裁剪版**（`memory/FUNCTION-REPOSITORY.md` §2.5：去掉 shard、固定 16 位，已实现于 `store::ids`）。将来若要把库登记到那套服务端，需要双向转换：**补回 shard 段** + 移植 `Gid` 的 `toUpper`（base36 大写、定长 20）形式；注意 base62 在大小写不敏感文件系统上的风险 | 决定对接 Luclin 体系时 |
| G12 | **吸附（snap）的实际行为** | 设计稿把吸附开关列为「以后会加」的占位；M1-4 只做了它的视觉与状态（按下就是按下）。真要做：拖动照片时吸附到网格/边缘，需与视口变换（M2）一起设计 | M2 视口完成后 |
| G18 | **后端错误文案的 i18n** | 界面文案已经全部进语言包（中/英，有 `pnpm lint:i18n` 守门），但**后端（Rust）抛给前端的错误句仍是写死的中文**（`crates/raybend/src/error.rs` 的 `thiserror` 文案、`src-tauri` 的命令错误、扫描 problems 等）。要做的话得改成「错误码 + 参数」的结构化错误走 IPC，前端加一层 `error.*` 映射与回退（拿不到 code 就显示原话）。**登记原因**：范围不小且现在没阻塞（工作语言就是中文），但一旦真要分发多语言版，它是最大的一块 | 做多语言分发之前，或 Web/CLI 也复用核心库时 |
| G19 | **统一 UI 状态池（命令可用性 → 总状态）** | 崔总 2026-09-23 提的方向：把快捷键、命令可用性与 UI 组件状态接进**一个状态池**，由统一状态驱动。本轮（2026-09-24）只做了**最小收敛**：`lib/commands.ts::availabilityOf` 把「能不能用」收成一条判定，分发器 / 命令面板 / 标题栏菜单读同一份（之前三套口径，正是「F11 搜不到」的温床）。真正的状态池是更大的重构：状态源从「各工作区注册的动作槽」改为共享 store，收益是跨界面一致性（菜单/面板/按钮/热键不会各说各话），风险是动到全部工作区的状态接线 | 命令/界面数量再长一波，或再出现一次「同一个能力在不同入口说法不一致」的 bug |
| G20 | **Gallery / 外部编辑器** | 外部编辑产物入库统一管理（新 flow + 与 browse 关联跳转）；**取回链路跑不顺**，现在只做「纯单向输出」。详见文末「Gallery / 外部编辑器」 | 单向输出 M4-W6 已开发，软件兼容待验收；Gallery 本体未排期 |
| G21 | **导出的「来源标记」：元数据 → 水印 → 内容凭证** | **定位：它是「来源标记」，不是「防盗手段」**（当防拷手段宣传是错的期待）。三层按成本递进：① **元数据**（EXIF/IPTC 的作者·联系方式·版权）—— **M4 基座已有**（导出元数据矩阵已写作者/版权/说明与关键词）且**已在写入**，**再完善即可**，不单独开波次；缺点是一删就没。② **传统频域水印**（DCT/DWT 嵌短串，如用户 ID + 时间戳）—— 能抗压缩与轻度裁剪，适合「发到平台想证明是我拍的」这类**善意场景**；❗**不要宣传「不可去除」**，对抗性编辑下不堪一击。③ **内容凭证 C2PA** https://c2pa.org/ —— 记录来源与编辑历史、可密码学验证（徕卡已机内做签名）；它解决的是**信任链**而不是「把信息藏起来」，比传统隐形水印更值得投入，但**比较远** | ① 随时可做（不单独开波次）；② 有人提出「要能证明是我拍的」时；③ 更远，等生态与需求成形 |
| G22 | **Windows 本地源码镜像 / MSI 恢复** | **2026-09-29 已实现脚本，待实际 MSI 验收**。旧失败原因：32 位 WiX light.exe 无法从 WSL 映射盘/UNC 源可靠创建 cabinet；历史实测见 `implementations/2026-09-27_release-msi-wix-light-path.md`。现方案从 WSL 一条 `pnpm release … --win-msi` 自动镜像并调用 Windows Cargo/Tauri；`--win-nsis` 可独立/组合。按崔总追加要求，自动探测 Windows LocalApplicationData、本地盘与 WSL 实际挂载，通过 `wslpath` 双向转换，支持 `--win-dir`，不写死 C 盘或 `/mnt/c`。复用现有版本/签名/dav1d/核查/收口，不复制依赖、target 或 Git。规格 `specs/release-windows-msi.md`，操作 `docs/release.md`，实施 `implementations/2026-09-29_release-windows-msi.md`。首次切换目录冷编译，后续复用固定镜像和缓存；仍须 WSLENV 转发环境 | 脚本与路径冒烟已落地；实际 MSI 生成、安装/升级/卸载及 NSIS → MSI 切换由崔总验收 |
| G24 | **发版 CI 化：tag → 自动构建 → 填 Release 下载址 → 官网实时取版**（崔总 2026-10-01 登记：**发版验收通过后马上做；现在不动**，先验本地发版脚本） | 目标：push `v*` tag 到 GitHub → 一个 workflow 内自动：① windows runner 构建（NSIS + MSI）；② 创建 GitHub Release 并把安装包填进下载资产。**③ 官网版本号与下载地址由 website 前端 JS 运行时实时获取**（GitHub `/releases/latest`：天然只含非 draft/prerelease，恰合「最新有效版本」语义）——**action 不产生任何 commit**（崔总 2026-10-01 定，否掉「release workflow commit 版本文件」与「website 固定分支」两个 dirty 方案）。**查证结论（2026-10-01）：单 workflow 闭环可行**——`tauri-action` 官方支持 tag 触发→构建→建 Release→上传产物（含 updater `latest.json` 可选）；dav1d 静态库可在 windows runner 上 `meson + ninja + nasm` 一次构建并缓存（libavf CI 先例，本地等价脚本 `scripts/build-dav1d-win.cmd`）。已知适配点：① `release.mjs` 加 CI 模式（tag 即版本、无人工交互、与 tauri-action 的产物路径对齐，不再需要 WSL→Windows 镜像）；② dav1d 构建结果用 actions cache 复用；③ 建议 Release 先建 **draft**、人类点 publish（保留人类发布把关，`AGENTS.md` §2.1 精神）；④ updater/latest.json 是否上传随 M5 更新通道实配再定；⑤ `check:win` 的产物校验要在 CI 里跑同一套逻辑；⑥ 官网取版脚本带**静态兑底**（内置最低已知版本；`api.github.com` 国内可达性与匿名限流 60 次/时/IP 是现实风险，失败/超时回退静态值）；备选更稳形态：release workflow 发 `repository_dispatch` 触发 website 重构建、构建期注入（静态输出、访客不碰 GitHub API）——开工时二选一。参考：https://v2.tauri.app/distribute/pipelines/github/ 、https://github.com/tauri-apps/tauri-action | 崔总发版验收通过后立即排期 |

---

### G23. NAS 来源与远程库的存储边界（2026-09-28 摸排建议，待决）

崔总要求为可卸载来源/库的完善评估未来 NAS 接口。先复用本地恢复工作统一定位、结构化可用性、连接代次与库会话；来源侧扩展既有 `Scanner` / `FileOps`，不提前引入远程 SDK。**NAS 作导入源与 NAS 承载库是不同能力**；现行 catalog 不放网络盘/云同步位置的约束不变，远程库需另定 catalog 访问边界、身份与写入一致性。

候选接口、现有缺口及取舍统一见 `implementations/2026-09-28_removable-storage-audit.md` §2.E / §4；原话在 `specs/storage-recovery-w1.md` 附录，决策索引为 `memory/REVIEW.md` R5-03。尚未选定协议、框架或排期。

同日规划跟进：本地存储最小接缝随 `specs/storage-recovery-w1.md` 实施范围编排，专项路线见 `memory/PLAN.md` §1.1；这不代表 NAS 协议或远程 catalog 已进入实施范围。

2026-09-29 W1–W4 工程落地：已有位置分类/端点提示、ConnectionStatus、共享 ProbeBudget 与
CatalogSessions；来源扩展继续复用 Scanner/FileOps，长任务恢复通过 StorageRecovery 适配
设备可用性并固定实体身份。没有引入远程 SDK 或空壳 provider 框架。后续远程实现应接入这些
边界，另行确定远程对象身份与暂存/提交语义；本地 FileId 不直接当 NAS 对象身份。
实际协议、认证、远程 catalog 与多机写入仍待决，证据见 W3/W4 实施记录。

### G24 补充：国内分发与下载加速（2026-10-01 查证，崔总提出 rb.cthun.com 国内专供）

- **Gitee Pages：已死，排除**。2024-05 无公告下线（至今未恢复；蓝点网报道 https://www.landian.news/archives/103754.html 、Gitee 官方 issue https://gitee.com/oschina/git-osc/issues/I9RGKI ，Vant 等项目已被迫迁移）。Gitee 仓库镜像本身还能用（代码可见性/国内 clone 快），但页面托管没这个选项了。
- **Deno Deploy：无大陆节点，不解决国内问题**。官方区域表 https://docs.deno.org.cn/deploy/manual/regions/ 无中国大陆；社区实测国内流量走香港 GCP、移动绕德国。CI 倒是方便（GitHub 集成自动部署），但换了也不比 GitHub Pages 快。
- **首选候选：腾讯 EdgeOne Pages（现名 EdgeOne Makers）** https://pages.edgeone.ai/ —— 免费、静态托管、**GitHub 仓库集成自动构建部署**（CI 方便，不用镜像仓库）、腾讯 CDN **含大陆节点**、自带媒体存储（可放下载文件，额度待查）。✅ **前提已满足（2026-10-01 崔总确认）：`cthun.com` 托管于腾讯云个人账号，已实名认证且已 ICP 备案**——域名、托管、EdgeOne 同在腾讯体系，无接入商变更障碍；`rb.cthun.com` 子域共享主域备案，CNAME 到 EdgeOne 分配地址即可。
- **零成本补充**：官网取版 JS 可给 GitHub 资产 URL 拼 ghproxy 类加速前缀做「加速下载」入口——不需镜像仓库/新站，但公共代理稳定性自担，只作兑底选项。
- **MSIX 进商店**：崔总 2026-10-01 重申「觉得有用」——维持 R3-01 拍板（直下链走通后即启动，零现金成本签名主攻）；与 G24 同属发版验收后的分发收口。

---

## H. 导入与浏览的界面演进

> 这两项来自 2026-09-15 的界面设计评审，**当时明确不做**，但已确定是方向。

### H1　按时间分组模式的**纵向导航条**

* **内容**：在按时间分组（`memory/DESIGN.md` §12.7）的照片区**右侧**加一条**纵向导航条**，
  用于快速跳转：日组 / 时间片在导轨上以刻度或色块表示，拖动或点击即可跳到对应位置。
* **为什么需要**：按时间分组后内容会变得很长（一天可能几十个时间片，一次导入可能几十天），
  光靠滚动滚过不去。这与音乐/视频软件的「章节导轨」是同一类交互。
* **UI 形态建议**：极窄的一条（不超过 `12px`），悬浮/拖动时才展开显示时间标签；
  上面用「片密度」表达拍摄密集度（密集处色块高、稀疏处接近空白）。
* **与现有设计的关系**：应与右侧「库」列共存而不与之冲突 —— 导航条紧贴照片区右缘、
  而非窗口右缘。
* **依赖**：`VirtualGrid` 与时间分组先落地（M0-6 与 M2）。
* **状态**：📝 未排期（原 M4-W5 后移，见 I 节）

### H2　导入前的**筛选剔除强化**

* **内容**：把现在「选目录 → 按时间分组 → 批量排除 → 导入」这套，做得**更强**：
  在导入前就能完成一轮接近完整浏览体验的筛选（而不只是排除少数几张）。
* **可能包含**：
  * 按拍摄参数筛选（机身/镜头/焦段/ISO/快门）后再导入
  * 只导入达标照片、自动 Skiplist（连拍重复、极相似帧）
  * 导入前的快速全屏预览与标星
  * 排除规则可保存为预设，下次自动套用
* **为什么先不做**：导入模块第一阶段的目标是「把照片可靠地弄进库并看到缩略图」。
  导入前筛选是**锦上添花**，且它会把缩略图/解码/预览的难度拉高到接近完整浏览模块。
* **与「浏览」模块的边界**：若两者能力越来越接近，应考虑**合并设计**，
  而不是维护两套相似的筛选界面 —— 这一点在开工前需重新评估。
* **状态**：📝 已登记，未排期

### 导入模版的编辑体验（M1-9 的明确取舍）

M1 里「导入模版」**纯手输**（`LibrarySettingsDialog`：一个输入框 + 实时预览 + 校验）。
不做下拉变量、不做可视化拼装 —— 先把功能跑通。以后要优化的方向（登记在此，别再重复发明）：

* 变量**面板/下拉**：点一下把 `:CYEAR` 之类插到光标处（现在只有提示文案）；
* **可视化拼装**：目录段 / 文件名段分开编辑，每段看到示例结果；
* **常用模版**：几个预设（按日期分层、按机型分层、平铺）一键套用；
* 模版**改动的影响提示**：改完告诉用户「以后的导入会落到新路径，已有照片不动」。

### H3　每库多个导入模版

* **内容**：一个库可以保存**多个**导入模版，导入时选一个（现在是每库一个）。
* **出处**：用户 2026-09-15 在浏览模式规格里提到，并明确「**优先级不高，先简约**」。
* **依赖**：`memory/FUNCTION-REPOSITORY.md` §3 的模版系统先落地（M1-6）。库设置弹窗里要从「一个输入框」变成「模版列表 + 选择」。
* **状态**：📝 已登记，低优先级

### H4　标签库管理界面

* **内容**：标签的**改名、合并、删除、批量整理**（按使用次数排序、找孤儿标签等）。
* **为什么先不做**：浏览模式只需要「用标签」（输入即搜 + 回车新建），管理是低频操作。
* **依赖**：标签体系落地（`app.db` 的全局标签词典 + 每库关联）。
* **状态**：📝 未排期（原 M4-W2 后移，见 I 节）

### H5　`titlebar` 右侧的消息中心

* **内容**：右上角 toast（非遮挡提示）的**历史列表**入口 —— 点开能看到最近的通知。
* **出处**：用户 2026-09-15 提出 toast 放右上角时顺带提到「以后可以在 titlebar 右边放消息中心」。
* **依赖**：toast 组件先落地（浏览模式的一部分）。
* **状态**：📝 已登记，未排期

### H6　图片筒（picture bucket）：跨目录 / 跨库的临时工作集

* **内容**：一个**跨目录、跨库**的「桶」，把不同位置的照片放进同一个工作集里做批量操作
  （评级、打标签、导出、删除…），像 Lightroom 的 Quick Collection / digiKam 的 Light Table。
* **与现有机制的关系**：浏览模式的**旗标（flag）**目前承担了「临时工作集」的角色 ——
  但它**只在内存、不持久化**。picture bucket 是它的**持久化、可命名、可多桶**版本。
* **同时解决**：浏览模式**暂不支持跨目录选择**（`memory/FUNCTION-BROWSE.md` §5.2）—— 有了桶就能跨目录批量操作。
* **状态**：已并入相片整理上期并完成代码与 Agent 冒烟；跨目录／跨库的持久相片桶及批量操作已落地，Windows 真机验收待崔总。见 `memory/FINISHED.md` §11；不另造一套桶。

### H7　每库导入时自动写 `author`

* **内容**：按库配置一个默认作者，导入时自动写进照片的作者字段。
* **出处**：用户 2026-09-15（浏览模式信息栏的「作者（可编辑）」字段附注）。
* **状态**：📝 已登记，未排期

### H8　搜索扩展到照片信息

* **内容**：左列搜索条从「只搜库名 / 目录名」扩展到**照片信息** —— 标签、注释（Description）、
  文件名、拍摄日期、EXIF 品牌/机型等，并支持组合条件。
* **依赖**：FTS5 索引（`AGENTS.md` §7.2 已定 `trigram`，注意**两字查询搜不到**，短词要 `LIKE` 兜底）
  * 标签体系落地。
* **人类 2026-09-21 注**：落地时把 **`Ctrl+K` 同时应用于照片搜索** —— 命令面板的入口习惯复用为
  照片搜索入口（面板内切模式或直达搜索），不要另起一套搜索快捷键。
* **状态**：📝 未排期（原 M4-W1 后移，见 I 节）

### H9　对比模式的进一步能力

* **内容**：多图网格对比（>4 张）、同步方式可切换（同步缩放 / 独立缩放）、差值/叠加等看图工具。
* **现状**：本期只做「并排 + 比例同步 + 位移百分比同步 + 以第一幅画幅为准」，
  且**最多对比最近选中的 4 张**（人类 2026-09-19 定：不支持超过 4 张）。
* **状态**：📝 已登记，未排期

### H10　高级筛选面板（分面计数 + 文本 / 标签 / 日期 / 机型 / 镜头 / ISO / 焦段）

* **内容**：一个「高级筛选」面板：左侧分面树（按机型 / 镜头 / 年份 / 标签 / 星级带计数），
  右侧条件区；配合**文本搜索**（文件名 / 注释 / EXIF）与**日期区间**、镜头、ISO、焦段区间。
* **现状**：本期（M2-W2）的筛选范围是**口述范围** —— toolsbar 上的筛选开关 +
  条件 chips + 排序 + 「任一 / 全部」；**高级筛选面板明确不做**（人类 2026-09-18 定），
  引擎其实已经支持这些字段（`BrowseFilter` 的 `text` / `takenFrom` / `cameras` / `lenses` /
  `isoFrom` / `focalFrom` 等），缺的只是界面与分面计数（`query::facets` 目前只统计标记四组）。
* **状态**：📝 未排期（原 M4-W1 后移，见 I 节）

---

## I. 组织、检索与互操作（原 M4 整体后移，未排期；2026-09-21 人类定案）

> 原计划为独立 milestone（5 波）；重排后不排期，待核心功能（编辑/导出）与发版后按需求拉回。
> 底座（平面标签、FTS、查询/分面、时间线）已在 M1/M2 落地，未来拉回时不重铺。
>
> **2026-09-26 更新**：本节里的 **XMP** 与 **相片集（集合/智能集合）** 已被人类定为
> **M5 之后三条主线**中的两条 —— 见 `memory/PLAN.md`「M5 之后：三条主线」，优先级以那里为准。
> 本节其余条目（搜索 UI、标签管理、地图/发现视图等）**仍未排期**。

* **全局搜索与高级筛选**：照片级搜索 UI（文件名/标题/说明/标签/器材/地点/日期）、搜索历史与保存查询、
  可序列化的查询 DSL；**`Ctrl+K` 同时应用于照片搜索**（人类 2026-09-21 注，详见 H8）；
  高级筛选面板（分面计数 + 文本/标签/日期/机型/镜头/ISO/焦段，H10）
* **标签体系收尾**：层级、改名/合并/删除、同义词、孤儿清理、标签管理面板（H4）
* **集合与智能集合**：手动集合 + 保存查询的智能集合，承接跨目录工作集（H6 的图片筒由它实现）
* **XMP 互操作**（**已实施完成，范围已冻结**）：规格 `specs/xmp-sidecar.md`（原名 `xmp-w1.md`，2026-10-01 随冻结更名；真机验收待崔总）。
  范围：写出 `rb:`（全部 issue + latest）+ 标准层（raw-based latest 的 `crs:` 调整 + 基础元数据：评级/色标/关键词/作者/说明/地点）；
  自家导入在新登记资产时**自动发现**；跨家方言读入（Lightroom/darktable）**不做**（崔总 2026-10-01 定：不适配其他家的导入，非延后）；
  「DB 为真相源、XMP 为通道」冲突规则不变；**XMP 不是第一公民**——整库备份/迁移靠**直接拷 repos 目录**（见 J 节）。
  issue 的 XMP 表示契约在 `specs/issue-xmp-contract.md`（本波同步 `rb:ordinal` 与 `auto_adjust` 说明）。
* **发现视图**：时间线聚合导航、地图（底图依赖需先评估，G 系列另有登记）、感知哈希重复/相似、
  burst grouping（H1/H9 相关）

## J. 多仓、缓存治理与数据安全（原 M5 整体后移，未排期；**含数据备份支持**）

> 原计划为独立 milestone（3 波）；重排后不排期。**明确包含数据的备份支持**（人类 2026-09-21 注）——
> 迁移前快照与 7 份轮转已在 M1 落地，未来拉回时补齐定期备份策略、启动检查、恢复 UI 与用户指引。
> **2026-09-29 崔总再次确认：备份功能要做**——仍留本节未排期，拉回开工前先出方案。
> **2026-09-30 崔总口径（备份路线收窄）**：catalog 结构**可能连导出都不需要做**——**直接把 repos 目录拷过去就行**；
> 顶多在「创建库」流程加一个**「引用已有目录」**的入口。定期策略/恢复 UI 等条目按此重新审视、拉回时再细化；
> XMP 不承担备份职责（见 I 节与 `specs/xmp-sidecar.md` §0）。

* **多仓与离线卷**：多仓同时打开、跨仓搜索、拔插盘降级与自动重定位
* **缓存治理与任务中心**：容量/保留期/路径设置、缓存面板与一键清理、统一任务中心
* **备份、恢复、完整性与诊断**：**数据备份支持**（定期策略 + 恢复 UI + 指引）、`integrity_check` 调度、
  日志与诊断包；RAW worker 失败接进统一诊断

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-10-09 | 崔总定：**应用内回收站发版前必加**，形态待规划；登记为 Future Release #10（与「删除=系统回收站」区分）。 |
| 2026-10-07 | editor presets 审计问题逐条修复并补回归（等高容器、选中收敛、名称上限、库串行、payload 版本契约、新建错误区分、rev 单次、应用前重读 LUT）；记录 `implementations/2026-10-07_editor-presets-audit-fixes.md`。状态仍为**已完成、待真机验收**。 |
| 2026-10-04 | 崔总确认 XMP 与 editor presets **已完成，待验收**；本轮 XMP 审查缺陷修复见 `implementations/2026-10-04_xmp-audit-fixes.md`。当前完善色彩管理与文件管理，二者完成并验收后最后推进 CI 和发版，由其它会话处理。 |
| 2026-10-01 | **新增 G24（发版 CI 化，崔总指示登记）**：发版验收通过后马上做——tag 触发 GitHub Actions 自动构建 NSIS+MSI、填 Release 下载资产、同步官网（单 workflow 闭环已查证可行）；当前不动，崔总先验本地发版脚本 |
| 2026-10-01 | **G24 官网同步方案修正（崔总同日定）**：否掉「action 内 commit 版本文件」与「website 固定分支」（都 dirty）；改为 **website 前端 JS 运行时实时获取** GitHub `/releases/latest`（action 零 commit）；登记静态兑底与 `repository_dispatch` 备选，开工时二选一 |
| 2026-10-01 | **G24 补：国内分发查证**：Gitee Pages 已于 2024-05 无公告下线（排除）；Deno Deploy 无大陆节点（不解决）；首选 **腾讯 EdgeOne Pages**（GitHub 集成自动部署 + 大陆节点，前提 `rb.cthun.com` ICP 备案——崔总提出，备案状态待确认）；MSIX 进商店崔总重申有用（维持 R3-01） |
| 2026-10-01 | **G24 补的硬前提已确认**：崔总告知 `cthun.com` 托管在腾讯云个人账号、已实名认证且已备案——EdgeOne 大陆节点方案无障碍，`rb.cthun.com` 子域共享备案、CNAME 接入即可 |
| 2026-10-01 | **XMP 范围冻结 + 规格更名**：崔总定「XMP 到此为止，不适配其他家导入（不做）」；规格 `specs/xmp-w1.md` 更名 `specs/xmp-sidecar.md`，移除全部 W1/分波次表述（无 W2）；活文档引用同步，历史存档（todos/implementations）按惯例不改 |
| 2026-10-01 | **设立「Future Release」板块**（崔总指示）：发版前必做项统一归口——四条主线收尾（presets 基本完成、相片整理真机验收、XMP W1 真机验收、色彩管理第一版收尾）+ 发版 CI 化（含官网 XHR 实时取版）+ 国内镜像（EdgeOne/rb.cthun.com）+ spike 页二选一 + MSI 实际验收；锁定块注记收尾归此板块 |
| 2026-09-30 | **XMP 细则定案 + 定位修正**：写出 `rb:`（全部 issue + latest）+ 标准层（raw-based latest 的调整 + 元数据）；自家导入自动发现；**XMP 不是第一公民**——备份 = 直接拷 repos 目录（J 节按此收窄口径）；规格 `specs/xmp-sidecar.md`（待审）。I 节 XMP 条目随之转入当前工作 |
| 2026-09-30 | **XMP 口径修正（崔总定）**：**导出优先、导入不急**——先建立自有格式表达与 sidecar，目的是表态开放、用户数据资产归用户；导入兼容降级为远期待需求拉回；「能否被别的软件兼容属边界之外」。原话 `docs/user-requirements.md` 2026-09-30 节；`memory/REVIEW.md` R6-01、`memory/PLAN.md` §4 #2 同步 |
| 2026-09-30 | **「四条主线」收缩为三条**：崔总指示「编辑器 preset 现在就做，future 里有就删掉」——#4 从锁定清单移除，转为当前工作（设计稿先行，`design/editor.pen` + `todos/2026-09-30-editor-presets.md`）；`memory/PLAN.md` §4 同步 |
| 2026-09-29 | **文件开头新增「发版后最优先四条主线」锁定块**（崔总指示写入）：四条主线清单与顺序未经崔总明确要求不可改动；详情以 `memory/PLAN.md` §4 为准 |
| 2026-09-29 | **H6 图片筒并入「强化相片集」主线**（崔总定：放进这次相片整理功能一起做掉，先记录待其整理方案）；**J 节备份功能确认要做**（崔总定，仍留 J 未排期）。详见 `memory/PLAN.md` §4 #3 与 `todos/2026-09-29-photo-collections.md` |
| 2026-09-15 | 初版：登记 A–G 七类远期方向；确立 rawler 为首选解码后端与候选清单；记录 canvas tile 的取舍理由 |
| 2026-09-15 | 新增 G13（库文件加密，含预想目的）/ G14（`photos/` 目录名可配置）/ G15（与 Luclin `Gid` 的互操作），来自用户对 M1-2 计划的评审意见 |
| 2026-09-19 | 新增 **H10**（高级筛选面板：分面计数 + 文本/标签/日期/机型/镜头/ISO/焦段 —— M2-W2 明确不做，引擎已支持字段）；H9 补记「最多对比最近选中的 4 张」 |
| 2026-09-20 | 跟随 `memory/PLAN.md` 路线重排：库与磁盘一致性提升到 M3-W1；H1/H4/H6/H8/H10 分别指向 M4 对应波次；G16 改为「DOM 查看器已产品化，原生视口留给显影」；C7 删除过时的 M2-W3 显影视口表述 |
| 2026-09-21 | 跟随 `memory/PLAN.md` 再重排（人类定案）：**编辑提前为 M3**（D 节口径同步、「issue 与 SOOC」标已排期、C4/C7/D2/D5/G16 触发点改为 M3）；一致性改指 M4-W1；原 M4（组织检索）与原 M5（多仓/缓存/数据安全，**含数据备份支持**）整体后移为本文件 **I/J 节**；H1/H4/H6/H8/H10 改未排期；H8 补注 **Ctrl+K 同时应用于照片搜索** |
| 2026-09-15 | 新增 **H3–H9**：来自浏览模式规格（`memory/FUNCTION-BROWSE.md`）——每库多模版 / 标签库管理 / 消息中心 / **图片筒 picture bucket**（含跨目录选择）/ 每库自动 author / 搜索扩展到照片信息 / 对比模式增强 |
| 2026-09-24 | 新增 **G20 + 文末「Gallery / 外部编辑器」**（人类记一笔）：Gallery 方向纳入远期；**取回链路跑不顺故现在不做**，只登记「纯单向输出」最小形态；附主流编辑器命令行调研矩阵 |
| 2026-09-24 | 「Gallery / 外部编辑器」补**编辑器发现**小节（人类定：走系统已安装软件探测，不让用户找启动文件；**限定范围** —— 支持范围 = 主流候选表，不做全系统枚举）：探测做成**可插拔 adapter**，现在只有 Windows 系统级接口实现（App Paths 注册表 + 路径 glob），mac/Linux 远期各加；同日调研核实 Photoshop 注册 App Paths、Affinity 官方提供 MSIX 与 MSI/EXE 双包 |
| 2026-09-24 | 单向输出**排期 M4-W4**（人类定案）；候选表首批 9 个：Photoshop / Affinity Photo（1.x+2.x 合并）/ Affinity（Canva 整合版）/ GIMP / Krita / PaintShop Pro / Topaz Photo AI + **RawTherapee、RapidRAW**（人类定：我们 RAW 开发不做极致，专业 RAW 开发送外部）；弹窗交互规格（默认上次应用 / + 应用发现 / 按路径判重 / 失效报错并自动删登记）与发现方案进 `memory/FINISHED.md` §7（M4-W4） |
| 2026-09-27 | 新增 **G21**（导出的「来源标记」）：人类定**定位为「来源标记」而非防盗手段**；三层 = 元数据（**已在 M4 基座**，只需完善）→ 传统频域水印（善意场景，**不得宣传「不可去除」**）→ 内容凭证 C2PA（**比较远**，解决信任链） |
| 2026-09-15 | 新增 H1/H2（导入与浏览的界面演进）；新增 G9（窗口状态持久化）/ G10（Win11 贴靠布局的丢失与补偿方案）/ G11（Linux·macOS 沉浸式外壳与 `tauri.linux.conf.json` 的收尾）/ G12（吸附的实际行为），均来自 M1-4 去系统标题栏时确认的边界 |

---

## issue 与 SOOC（人类 2026-09-19 记一笔；**已排期 M3**，2026-09-21）

这是本应用的一个**创新点**：每张图可以有**自然的多定稿版本**，每个版本叫一个 **issue**。

- 对图片做各种**无损**编辑 → 定稿出**一个 issue**；
- **导出界面**像画廊一样，自然地选择「导出哪个 issue」；
- **编辑界面**里可以对 issue 做对比；
- **浏览界面**可以选择「这张图现在显示哪个 issue」；
- **SOOC**（straight out of camera）是一个**固定的、写死的 issue**：
  **只要带原 JPG 的，那张 JPG 就是 SOOC**，且**不允许**把别的 issue 命名为 `sooc`。

未来很多功能会围绕 issue 与 SOOC 展开（导出、对比、浏览显示、编辑栈）。
它也与 RAW 的三形态相关：位图 + RAW 配对时，位图就是 SOOC，编辑落在 RAW 上。

**issue 打编辑基准标签（M3-W6c 已实现）**：

* 编辑器里可以切**编辑基准**（拿哪个当底）：`SOOC`（直出位图）/ `RAW`，默认 RAW ——
  切换已实现（`store::develop::EditBase` + `develop_edit_target(base)`，规格见 `memory/FUNCTION-IMAGING.md` §4.3）；
* 每个生成的 issue 已带「**基于 SOOC 编辑** / **基于 RAW 编辑**」的标签；
* **点不同标签的 issue 时同步切换总览图下的按钮**，恢复完整 profile 和编辑源；
* 前提：同一份编辑栈落在位图与 RAW 两种像素上观感有差，所以基准必须跟着 issue 存，
  不能只是一份全局偏好。命名 issue 不可变，`latest` 是随编辑或切 issue 更新的工作副本；撤销/重做覆盖切换。

---

## Gallery / 外部编辑器（人类 2026-09-24 记一笔；**单向输出已开发 M4-W6，真机兼容待验收**）

**愿景**：新增一个 **Gallery** 工作流，把「外部编辑器的产物」收进库里统一管理。

- Gallery 里的内容**库内不可编辑**，只能再送去外部编辑器；作用是**帮助管理 + 导出**；
- 与 `browse` 建立关联，两边可以方便地跳转（源照片 ↔ 它的外部产物）；
- 定位是**旁路**，不在 `import → browse → edit → export` 这条流水线上 ——
  若真做成第 5 个 flow，`AGENTS.md` §11.4 的「flow = 有序流水线」定义与 `memory/DESIGN.md` 要同步改；
- 存储倾向：`assets` 表加 `kind` 字段（`photo` / `gallery`），
  **复用**评级 / 标签 / 集合 / 搜索 / 缩略图 / 导出全部现有能力（§2.12），差异留在适配层；
  文件放**库根 `gallery/`**（与 `photos/` 平级，不污染导入模版与序号；**绝不进 `cache/`** ——
  那是用户劳动成果，不能被 GC 掉）；
- 开工前按 §5.1 先出 `design/gallery.pen` + `gallery.md`。

### 为什么现在不做：**取回链路跑不顺**（人类 2026-09-24 定）

外部编辑器**不会配合我们** —— 它不会通知我们「我保存了」。这条链路没法在「无需人工操作」的前提下跑通：

- **写文件不是原子的**：Photoshop 会写临时文件再改名，并长时间持有文件锁；
- **行业先例也在坏**：Lightroom 自己的 *Edit In Photoshop* 自动回收长期故障
  （Adobe 社区 2025 年帖：问题持续 9 个月以上，需手动重新导入）；
- **生态连调用都不配合**：Affinity Photo 2 起是 MSIX 打包应用，**不能直接调 exe**，
  社区通行的解法是拿一个 `.bat` 包一层 `start /b affinityphoto2.exe %1`。

结论：取回必须靠**显式的人工动作**，其价值撑不起现在做。等真有需求时再启动。

### 现在可做的最小形态：**纯单向输出**（**已排期 M4-W6**，人类 2026-09-24 定案）

**2026-09-26 最新入口定案**：仅 browse toolsbar right 提供外部编辑；拍摄信息上方用下拉选择当前显示定稿，选择只存组件内存且不影响 latest，切目录/重启后默认 latest。TIFF 只由此功能输出；普通导出改为 WebP/AVIF/JPG/PNG。2026-09-27 已实施；显示选择为组件内存，输出/应用发现与打开通过 Platform adapter。


不管取回，只做「送出去」：

1. 触发命令 → 弹窗确定**保存位置**（每次都弹，但**记忆上次目录**，不改就直接确定）；
2. 渲染当前 issue 并编码成 **16-bit TIFF**（无损；AVIF/JPG 不能作为编辑基础）；
3. Platform adapter 唤起选定外部编辑器打开该文件；
4. **结束** —— 产物留在用户选的位置，库里不留任何记录。

要点：

- **送出的必须是渲染结果，不是原始 RAW** —— 直接送 RAW 会绕过编辑栈，且两套解码结果不一致；
- **耗时是秒级**（渲染 + 16-bit TIFF 编码，6000×4000 未压缩约 144MB），
  不是「打开文件」那种瞬时操作，**需要进度反馈**；
- **应用登记**：`{ 名称, path }`，由 adapter 将 TIFF 路径作为一个文件参数传入，不暴露参数模板；
  存 `app.db.settings`（**设备级** —— exe 路径是机器相关的，不该跟着库走）；
  **不让用户自己找启动文件 —— 走系统已安装软件的发现**（人类 2026-09-24 定，探测方案见下节）；
  手动指定 exe 永远保留为兜底（便携版/绿色版发现不到）；
- **调用手段（2026-09-27 实施）**：独立 `Platform` adapter 的 `discover / available / launch`；Windows 复用已有 windows-sys 的 ShellExecuteW，单独引用 TIFF 参数，原生 POSIX 后备直接传 argv。无需新增 opener 依赖。macOS/Linux 发现机制未来按系统各补 adapter；MSIX 执行别名存在才发现，实际打开由真机核对。
- **命令体系**（§2.15）：登记为命令 + 快捷键设置可见；默认热键建议**留空**并写明理由
  （会弹窗、秒级耗时、非高频，且不与导出抢键位）；
- **已排期 M4-W6**（`memory/PLAN.md`，2026-09-24 定案）；M4-W4 导出格式清单已含 TIFF（16-bit 无损，外部编辑前置）。

### 编辑器发现：候选表 × Windows 系统级接口（人类 2026-09-24 定；同日调研核实）

不让用户去翻 `C:\Program Files`，也**不做全系统应用发现** ——
**支持范围 = 候选表**：挑定一批主流编辑器（PS / Affinity / GIMP / Krita 等），
每个条目带 `{ 显示名, exe 名, 参数模板, 兼容性备注 }`，只探测表内这批。

**架构：探测做成可插拔 adapter**（与 `AGENTS.md` §2.12「一份实现 + 一层数据适配」同构）：

- 现在：**Windows 系统级接口**一个实现 ——
  ① `HKCU / HKLM / WOW6432Node` 三处 `...\CurrentVersion\App Paths\<exe名>`，
  `(default)` 值即完整路径（Microsoft 官方推荐的应用注册机制，
  [Application Registration](https://learn.microsoft.com/en-us/windows/win32/shell/app-registration)，
  官方明说「不鼓励 PATH、应在 App Paths 注册」；**Photoshop 确定注册**（`App Paths\Photoshop.exe`），
  主流桌面软件大多照此办理）；
  ② 个别兜底走常见路径 glob（如 `%ProgramFiles%\Adobe\Adobe Photoshop *\Photoshop.exe`）；
- 远期（F1/F2 平台扩展时）：mac（`/Applications` + LaunchServices）、Linux（`.desktop`）各加一个实现；
- **明确不做**：全系统枚举（开始菜单 `.lnk` 全扫、`SHAssocEnumHandlers` 全量发现）——
  限定范围后候选表 × App Paths 已够，全量发现反而混入无关程序，还得多一层过滤。

工程要点：

- 探测到路径**顺手 `fs::metadata` 验证**：卸载残留（键在文件没了）直接过滤；
- **多版本并存**（如 PS 2022/2024）都列出让用户选，不静默替人定；
- **发现 ≠ 启用**：结果经用户确认才生效；探测只在设置页打开 / 首次使用时跑一次，不进启动路径；
- 全在 Rust：`windows` crate 已在依赖里（注册表查询就是现成 API），**不新增大依赖**；
  前端只拿发现结果列表。

**Affinity Photo 2 特例**（官方核实）：官方**同时提供 MSIX 与 MSI/EXE 两种安装包**
（[官方说明](https://support.serif.com/hc/en-us/articles/10373465222671-What-s-the-difference-between-MSIX-and-MSI-EXE)）——
MSI/EXE 版是传统形态，可发现、可直接调 exe；**MSIX 版**装进隐藏的 WindowsApps 不能直调 exe：
优先探测 **App Execution Alias**（`%LOCALAPPDATA%\Microsoft\WindowsApps\` 下的别名，可带参数），
退路 `explorer.exe shell:AppsFolder\<AUMID>`（**带不了参数**，只能开编辑器让用户自己开文件，体验降级要标注）。

### 调研事实：主流编辑器的命令行（2026-09-24，避免将来重查）

| 编辑器 | 命令行 | 备注 |
| --- | --- | --- |
| Photoshop | `Photoshop.exe "<绝对路径>"` | **相对路径会失败**（Adobe 社区实测）；绝对路径可行 |
| Affinity Photo（1.x + 2.x 合并识别，人类定） | `Photo.exe "<file>"` | 1.x 与 2.x MSI 版传统安装可直调；2.x MSIX 版走别名/包装（见上 Affinity 特例） |
| Affinity（Canva 整合版） | 确切 exe 名待核实 | 2025.10 发布，EXE 与 MSIX 双形态（Canva 官方帮助确认）；作为整体进候选表，**独立 Designer / Publisher 不进** |
| GIMP | `gimp-2.10.exe [FILE...]`（3.x 为 `gimp-3.0.exe`） | 官方文档明确支持 |
| Krita | `krita.exe "<file>"` | 标准命令行打开（落地实测） |
| Corel PaintShop Pro | `Corel PaintShop Pro.exe "<file>"` | exe 名带空格；官方 CLI 主要是 Scriptlet 脚本，直开待落地实测 |
| Topaz Photo AI | `Topaz Photo AI.exe "<file>"` | 社区证实相对路径失败；另有 `tpai.exe` 批处理 CLI（不是 GUI） |
| RawTherapee | `rawtherapee.exe "<file>"`（落地核实） | 首批成员：**我们 RAW 开发不做极致，专业 RAW 开发送外部**（人类定） |
| RapidRAW | exe 名落地核实 | 首批成员，同上理由；AGPL-3.0，本项目参考实现 |
| **darktable** | ❌ | CLI 参数表里**没有「打开指定文件」**（只有调试类参数）；排除 |
| Lightroom / Capture One | ⚠️ 不建议 | catalog 型，**双库冲突** |
| 系统默认程序 | `open_path(path, None)` | 兜底，任何机器可用 |

**要明确标注给用户**：darktable、LR、C1 这类「catalog 型」当外部编辑器 =
两套软件同时管同一张照片的元数据，那不是集成，是制造数据损坏。
RawTherapee / RapidRAW 是**首批成员**（我们 RAW 开发不做极致，专业 RAW 开发送外部，人类定）；
它们收到的是我们的渲染 TIFF，其 RAW 专属模块不可用，属预期（`memory/FINISHED.md` §7（M4-W6））。

### 未定项

- Gallery 的中文名（**建议别叫「画廊」** —— 摄影语境里「画廊」通常指 Web/打印这类输出模块，容易混）；
- Gallery 的入口是否包含「手动导入已有的外部文件」（会把它从「我们的衍生产物」扩成「外部文件管理器」）；
- Gallery 本体的排期（单向输出已定 **M4-W6**）。

### 2026-09-26：M4-W1 文件监听选型落地

采用 `notify` 8.2.0（官方说明：https://docs.rs/notify/8.2.0/notify/），关闭默认的 macOS feature；Windows/Linux 使用平台原生监听。
最多保留 32 个访问过的范围，监听本目录、实际 `_RAW` 与父目录，全部非递归。
照片根的父目录用于发现 `photos` 整体替换；事件在入队前排除 DB/cache/隐藏/临时文件，避免自触发。
200ms 去抖、连续事件最多等待 1s，积压超限改为重扫活跃范围。原生事件不能替代进入、展开与回焦点读盘。
