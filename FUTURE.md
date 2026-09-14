# raybend — 远期方向登记册（FUTURE.md）

> 本文件登记**想做但要等、要先评估、或明确暂不做**的方向。**不做排期**。
> 排期在 `PLAN.md`；纪律与关键真相在 `AGENTS.md`。
> 任何新增顶层框架或大型依赖，先登记到本文件再讨论。
>
> 建立时间：2026-09-15 00:18:03 CST

---

## A. 运行时與框架迁移

### A1　Solid 2.0 迁移

- **现状**：Solid 2.0 仍是 RC（`2.0.0-rc.8`），配套路由 `@solidjs/router` 2.x 还在 `next`（`2.0.0-next.24`）；生态（组件库、虚拟列表）尚未跟进。
- **收益**：Solid 2 把 async 变成响应式图的一部分，对「大网格里缩略图异步流式进入」「筛选切换时的并发渲染」这类场景直接有益；同时核心吸收了原属于 SolidStart 的职责。
- **触发条件**：Solid 2.0 GA + Kobalte/Ark UI 与虚拟列表方案公布兼容版本。
- **迁移要点**：先升 `solid-js` 与 `@solidjs/router`；检查 Kobalte 兼容性；检查虚拟化方案；检查 `createResource`/`Suspense` 相关写法。
- **风险**：RC 期 API 仍可能变动；迁移期间生态文档稀少。

### A2　Tauri 3.0 迁移

- **现状**：`3.0.0-alpha.0` 已发布。已知变更：
  - 移除 `wry` / `cef` feature flag 与 `tauri::Wry`，改为显式依赖 `tauri-runtime-wry` / `tauri-runtime-cef`
  - `tauri-build` 不再复制 resources 到 target 目录，dev 从源路径解析
  - 顶层导出重整、MSRV 提升到 1.95、Linux 端迁 GTK4
- **我们在 M0–M6 期间就已遵守的纪律**（见 `AGENTS.md` §6.2）：业务逻辑不进 `src-tauri`；不引用 `tauri::Wry`；不依赖 feature flag；资源路径走 Tauri path API。
- **触发条件**：Tauri 3 进入 beta 或 rc 且社区插件（dialog / fs / process / os / single-instance / updater）同步发布。
- **迁移顺序**：`tauri` + `tauri-build` → 运行时 crate 依赖 → 插件 API 变动 → 打包配置。
- **参考**：[tauri-apps/tauri#15068](https://github.com/tauri-apps/tauri/pull/15068)（运行时 crate 化）、[tauri-apps/tauri#14011](https://github.com/tauri-apps/tauri/issues/14011)（顶层导出整理）

### A3　Tauri CEF 运行时评估（备选与退路）

- **动机**：① 若 Windows 上「透明 webview + 原生 GPU 内容」合成出现不可解问题，CEF 提供可控的 Chromium 版本；② 消除 WebView2 随系统升级带来的行为漂移；③ 未来跨平台一致性。
- **代价**：包体积显著增大（CEF 远大于 WebView2 的系统依赖模式）、内存占用上升、构建链更复杂。
- **触发条件**：M0-W1 的渲染 spike 出现阻断性问题，或后期需要 WebView 行为完全可控。
- **参考**：Tauri 3 的 `tauri-runtime-cef`。

---

## B. RAW 解码后端（可插拔候选）

> 设计前提：`raybend-raw` 内做**后端可插拔**接口，其它模块不直接依赖任何具体解码库的类型。
> 解码一律在**独立 worker 进程**中执行（上游明确不保证恶意/损坏文件的安全性）。

| 编号 | 候选 | 许可 | 优点 | 缺点 / 风险 | 状态 |
| --- | --- | --- | --- | --- | --- |
| B1 | **rawler**（上游 [dnglab](https://github.com/dnglab/dnglab)） | LGPL-2.1-only | 纯 Rust、覆盖最广、同时给像素与元数据、自带 DNG 写出能力、可静态链接进 GPL-3 | API 不稳定不遵循 SemVer、无 GPU、官方明说 Windows 未支持、不为恶意文件做防护 | **当前首选** |
| B2 | rawler（RapidRAW fork：`CyberTimon/RapidRAW-DngLab`） | LGPL-2.1 派生 | 含其解码修复（可从 diff 学习/吸收） | 需确认 fork 新增代码的许可标注；跟随 fork 有维护耦合 | 待评估（可能直接采用或只吸收补丁） |
| B3 | [libraw](https://www.libraw.org/)（Rust 绑定 [rsraw](https://github.com/mdegans/rsraw)、[libraw-rs](https://github.com/paolobarbolini/libraw-rs)） | LGPL-2.1 / CDDL-1.0 | 成熟、相机覆盖广、可作质量对照基准 | C++ 依赖、线程模型一般、性能不突出 | 备选/对照基准 |
| B4 | [rawspeed](https://github.com/darktable-org/rawspeed) | LGPL-2.1 | 解包速度极快（Canon/Nikon 尤其） | 只解包不做颜色；C++；集成成本高 | 性能备选 |
| B5 | rawloader（[pedrocr/rawloader](https://github.com/pedrocr/rawloader)） | 宽松许可（需复核） | 纯 Rust、干净 | 覆盖少、维护弱 | 低优先 |
| B6 | [openraw](https://lib.rs/crates/libopenraw) | 需复核（Rust 原生重构版） | Rust 原生、专注文件结构解析 | 生态小、成熟度未知 | 观察 |
| B7 | [zenraw](https://github.com/imazen/zenraw)（imazen） | **AGPL-3.0-only 或商业授权** | 三种可换后端、与 zencodec 生态整合 | 许可与 GPL-3 组合需谨慎、太新 | 观察（不做首选） |
| B8 | 平台原生：macOS ImageIO / Windows WIC + Raw Image Extension | 系统组件 | 免费、与系统一致、mac 上质量好 | Windows 侧质量一般且依赖商店包；不可控 | 远期（mac 阶段） |
| B9 | Adobe DNG SDK | 专有 | 最省事 | 与 GPL-3 不兼容 | **排除** |
| B10 | 自研解析器 | 自有 | 完全可控、可针对性能优化 | 工作量巨大、相机覆盖是长期苦役 | 长期（仅在 B1/B2 出现阻断问题时考虑） |

**补充方向**：DNG 归档功能（把老 RAW 批量转 DNG 并嵌入原图）——rawler 本身是 DNG 写出器，这是少见的能力，可作为「档案化」卖点，但属于编辑/归档里程碑。

---

## C. 渲染与色彩

### C1　色彩管理（**已在 M0 预留，M 后期接入**）

- 显示器 ICC 通过 Windows `GetICMProfile`（`windows` crate）获取，并监听显示配置变化
- 转换用 [lcms2](https://github.com/kornelski/rust-lcms2)，把结果烘成 3D LUT（如 33³）上传 GPU 每帧采样
- **触发条件**：显影模块开始时（或用户反馈广色域显示器偏色时）

### C2　HDR / 10bit 输出

- Windows 需要 Advanced Color 与对应 DXGI 色彩空间；wgpu 侧支持有限
- **触发条件**：SDR 流程稳定后再评估

### C3　前端渲染演进：**canvas tile（登记为备选，不采用）**

- **为什么现在用虚拟列表而不是 canvas tile**：
  1. **实际瓶颈不是 DOM**。网格在「只渲染可见行 + 固定单元格尺寸」的前提下，DOM 节点数恒定，10 万条与 1 千条的差别只在数据层，不在渲染层。RapidRAW 的网格就是 `react-window` 虚拟列表（不是 canvas），运行在 Tauri 上；这直接说明这条路在同类产品中是成立的。
  2. **快速滚动时的问题不是「没画出来」，而是「解码跟不上」**。无论 DOM 还是 canvas，画布上画的都是已经解码好的位图；解码速度由缩略图尺寸、编码格式与解码线程决定。canvas 不能加速解码，只能减少节点创建与布局开销。
  3. **占位 tile 方案本身没问题**，但它解决的是「已解码位图还没到位」的视觉过渡，不解决「解码队列积压」。真正的解法是：解码离开主线程、用已解码位图 LRU、取消不可见的解码请求、缩略图尺寸与显示尺寸匹配（避免浏览器二次缩放）。
  4. **DOM 带来的能力是免费的**：选中态、键盘导航、框选、拖拽、无障碍、DevTools 调试、CSS 主题。换成 canvas 这些都要自己实现。
  5. **canvas 的收益场景很窄**：单屏需要同时显示数万个单元、或需要像素级自定义绘制（波形图/密度图叠加）。以相片管理器的展示密度，达不到这个量级。
- **因此**：canvas 只在「虚拟化列表实测达不到目标帧率」时才作为优化手段引入；当前已有一个中间方案——网格用 DOM 虚拟化，**覆盖层（蒙版、裁剪、密度图等）用 canvas**（RapidRAW 用 `konva` 就是这个分工）。
- **触发条件**：M2 的 10 万条目实测帧率不达标，且排除了解码与布局因素。
- **参考**：`react-window`（RapidRAW 用法）、`@tanstack/solid-virtual`（Solid 版）

### C4　多视口与多窗口

- 双显示器「网格 + 查看器」分离、多文档标签页撕出为独立窗口
- **触发条件**：核心闭环稳定后

---

## D. 编辑与显影模块（第二阶段主战场）

> 用户明确：**编辑不是第一阶段的重点**。这里只登记方向与前提条件。

### D1　场景参考（scene-referred）管线

- 从 RAW 线性数据开始处理，保证色阶连续（**不是**转成 JPEG 再编辑）
- 关键认知：RAW 编辑必须是「解码 → 线性化 → 场景参考处理 → 显示变换」的完整链路；任何在 8bit 显示参考空间里做的编辑都会损失色阶
- 参考：[darktable 的色彩管线文档](https://docs.darktable.org/usermangement/development/en/special-topics/color-pipeline/)（注意 darktable 是 GPL-3，算法可移植）

### D2　去马赛克算法候选

- 候选：RCD（darktable）、Markesteijn（X-Trans，darktable/RawTherapee）、AMaZE / DCB（RawTherapee）
- **许可前提已满足**：darktable 与 RawTherapee 都是 GPL-3，可移植（需保留署名与来源说明）
- **触发条件**：显影模块启动；此时需要在「直接用简单算法（快）」与「移植高质量算法（慢）」之间做取舍

### D3　降噪 / 锐化 / 镜头校正

- 镜头校正数据库：[lensfun](https://lensfun.github.io/)（LGPL-3，与 GPL-3 兼容）
- 降噪候选：小波（RawTherapee）、NLM、后期可接 AI 去噪
- GPU 化：wgpu compute shader（`AGENTS.md` §6.1 的渲染纪律已为此留口）

### D4　局部调整与蒙版

- 线性/径向渐变、画笔蒙版、色彩/亮度范围蒙版
- AI 主体/天空分割（见 E 节）
- 蒙版数据存储格式需要在 `develop_stack` 里预留字段（**M1 的 schema 设计时留位**）

### D5　色调映射

- AgX / filmic（darktable 的 AgX 是当代画质基准之一）
- 显示变换（display transform）与 C1 色彩管理一起做

### D6　DNG 归档

- 把老机型 RAW 批量转 DNG（可选嵌入原图）——rawler 自带此能力

### D7　联机拍摄（Tethering）

- Windows 上可用 [gphoto2](https://github.com/gphoto/libgphoto2)（RapidRAW 在 Unix 上用它）或厂商 SDK
- **触发条件**：核心闭环完成后，看用户需求

---

## E. AI 能力

### E1　推理后端引入方式

- 参考 RapidRAW：`ort`（ONNX Runtime，`load-dynamic` 动态加载）+ `tokenizers`
- **优点**：不捆绑 ORT 二进制、许可证干净（MIT）、模型可外置
- **前提**：模型分发策略（本地优先 vs 可选下载）、体积与首次启动体验

### E2　分割类

- 主体 / 天空 / 背景分割（用于蒙版），及后续的「一键换背景」类功能

### E3　语义搜索

- CLIP 类模型 + `tokenizers`：自然语言找照片（"海边日落"）
- 与 FTS5 的关键词搜索互补，**不能替代**确定性搜索

### E4　人脸聚类

- 检测 + 嵌入 + 聚类；`index.db` 里已为此预留位置（`AGENTS.md` §6.4）

### E5　原则

- **本地优先**：不上传用户照片；模型本地运行
- 硬件适配：CPU（ONNX）优先，DirectML/NPU 作为加速选项（需评估依赖复杂度）

---

## F. 平台扩展

### F1　macOS

- 解码：ImageIO / CoreImage（免费且质量好）
- 渲染：Metal（wgpu 已支持）
- 打包：签名与公证（notarization）、App Store 政策与 GPL-3 的冲突（**注意：GPL-3 与 App Store 条款不兼容**，需走独立分发）
- 路径：NFD 规范化问题（`AGENTS.md` §7.3 已预留）

### F2　Linux

- Tauri 3 起 GTK4；Flatpak 分发；RAW 解码与色彩管理差异
- **优先级最低**

---

## G. 其它能力

| 编号 | 方向 | 说明 | 触发条件 |
| --- | --- | --- | --- |
| G1 | 插件/脚本系统 | 用户可用 Lua 或 WASM 扩展（导出规则、自定义元数据面板等） | 核心 API 稳定后 |
| G2 | Windows Shell 集成 | 注册 `IThumbnailProvider` 让资源管理器显示 raybend 渲染的缩略图。**风险**：进程内 shell 扩展，崩溃会影响 explorer.exe，签名与测试成本高 | 发布后 |
| G3 | 云端同步 | 多机同步 catalog 与编辑。**注意**：若做成网络服务，AGPL 的条款就会咬合；当前 GPL-3 不受影响 | 不考虑近期 |
| G4 | 视频与 Live Photo | 视频缩略图与时码；大幅增加复杂度 | 相片流程稳定后 |
| G5 | 打印与色彩校准 | 软打样、打印布局 | 色彩管理（C1）完成后 |
| G6 | 多语言 | 从一开始预留 i18n（建议 i18next 或等价方案），首个版本只做中文 + 英文 | M1 UI 骨架时留位 |
| G7 | CLI 与无头模式 | 复用 `crates/raybend` 做命令行导入/导出/校验（这也是分层架构的回报） | 核心库稳定后 |
| G8 | 分发与捐赠 | 发布渠道、捐赠入口、第三方许可声明页 | M6 |

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-15 | 初版：登记 A–G 七类远期方向；确立 rawler 为首选解码后端与候选清单；记录 canvas tile 的取舍理由 |
