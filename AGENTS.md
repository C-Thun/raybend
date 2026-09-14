# raybend — 项目指南（AGENTS.md）

> 本文件是 raybend 的**项目级 AGENTS.md**：核心信息、硬纪律、目录说明、记忆/文档索引，以及调研阶段挖出的**必须处理的关键问题**。
> 全局约定见 `~/.pi/agent/AGENTS.md`（包管理、URL 书写、plannotator 流程等），本文件不重复。
>
> 建立时间：2026-09-15 00:18:03 CST
> 相关文档：近中期计划见 `PLAN.md`，远期方向见 `FUTURE.md`，**视觉与配色体系见 `DESIGN.md`**，**待人类协助事项见 `ASSISTANCE.md`**。

<!-- -->

> [!IMPORTANT]
> **开工前先查 `ASSISTANCE.md`。**
> 只要该文件存在**且有实际内容（不为空）**，则**任何任务在真正开始前（哪怕只是规划阶段）必须立即停止**，
> 并向用户提示先协助完成 `ASSISTANCE.md` 中的事项；待用户确认完成后，Agent 需**逐项验证**后才可继续推进。
> 本规则优先于本文件中其他一切流程约定。

---

## 1. 项目定位

raybend（中文名**「光伴」**，产品名 `RayBend`）是一个**相片管理软件**，目标是成为 ON1 Photo RAW 这类商业软件的优秀开源替代。

- **主战场**：相片管理全流程 —— 导入 → 浏览 → 评级/打标 → 筛选/搜索 → 集合整理 → 导出。
- **第一阶段不做编辑**：RAW 只做最基础支持；显影/编辑模块作为后续里程碑（见 `PLAN.md`、`FUTURE.md`）。
- **平台**：Windows 优先（Win10/11 近两年版本即可，不考虑 Win7/8）；macOS / Linux 属远期。
- **相机支持策略**：不追求最新机型即时适配，跟得上主流即可。
- **许可**：**AGPL-3.0-only**（见 `LICENSE`）。选它的原因很实际：本项目主体是本地桌面应用，AGPL 的额外网络条款只在「对外提供网络服务」时才咬合；而它带来的好处是 rawler（LGPL-2.1-only）、darktable / RawTherapee（GPL-3.0）与 RapidRAW（AGPL-3.0）的代码都可以合法复用。项目以开源方式发布，不计划闭源收费。
- **设计立场**：要有行业软件的质感，但不做 20 年前那种拥挤界面；同时主动吸收 Web 应用的易用性与高可视化特性。不做 Affinity 的克隆，也不是 Web 应用的桌面壳。

---

## 2. 硬约束（不可违反）

1. **发布与推送必须由人类执行**：`git push`、打 tag、生成/上传发布物、上传到任何包注册表（npm/商店/发布页），全部由人操作。Agent 不得代劳。
2. **commit 可以由 Agent 自行管理**：实现完成后可自行 `git commit`。提交信息用中文，格式 `<type>: <subject>`（如 `feat: 实现相片仓注册表`）。
   - 例外：处于 plannotator review 流程时，遵守全局规则 —— 审查期间**禁止** commit。
3. **工具链只有 pnpm + cargo**：前端与脚本用 **pnpm**，Rust 用 **cargo**。禁止引入 npm / yarn / bun 或混用锁文件——仓库只允许 `pnpm-lock.yaml` 与 `Cargo.lock`。（`npm install -g` 仅用于本机全局 CLI 工具，不用于本项目依赖。）
4. **前端不写图像算法**：像素、色彩空间、视口变换、渲染管线全部属于 Rust。前端只负责交互状态、矢量覆盖层与 UI。
5. **不引入 SolidStart**（桌面应用无 SSR 需求，SolidStart v2 面向 Solid v1，与 Solid 2 不配套）。
6. **Pencil 设计纪律**：所有界面设计用 Pencil MCP 绘制 `.pen` 文件，存放于 `design/`；每个 `.pen` 必须配一个**同名 `.md`** 说明界面结构、状态与交互。
7. **实施记录纪律**：每次编写/改动完成后，在 `implementations/` 下写一个 `.md` 记录，文件名以 `YYYY-MM-DD_` 开头后接简短命名；**文件内容开头必须写明本次改动的具体完成时间（精确到秒）**。
8. **测试分工：Agent 只做冒烟，E2E 归人类**：
   - **Agent 只做冒烟测试**：编译通过、命令能跑通、进程能启动、日志无报错、单元测试通过 —— 到此为止。
   - **真正的 E2E 测试交给人类执行**：涉及 GUI 交互与视觉正确性、多显示器与 DPI、真实照片库、性能体感、色彩正确性这类验证，一律由人类在真实环境确认，Agent 不得声称它已验证过。
   - **E2E 若要自动化**：除非用户特别指定要实现，否则不由 Agent 临时手跑一遍完事；需要时写成**可重复执行的自动化测试代码**。
   - 推而广之：**“进程起来了”不等于“功能对了”**。Agent 报告中必须清楚区分「已验证（冒烟）」与「未经人类验证」两部分。
9. 新增顶层框架或大型依赖前先在会话中讨论，候选方案统一登记到 `FUTURE.md`。
10. **单元测试必须齐备**——交付任一模块时，必须同时交付与之匹配的单元测试：
    - **覆盖边界，不只测正常路径**：空输入 / 单元素 / 上下限 / 非法值 / Unicode 与中文 / 超长与非法路径 / 大小写与规范化差异 / 并发竞争 / 溢出与截断……按模块性质取用。
    - **考虑执行效率**：`cargo test` 必须保持在秒级。真实照片、大文件、十万行数据这类重负载要降为小规模合成数据，或标记为 `#[ignore]` 的手动基准，不要拖慢日常测试。
    - 与第 8 条不冲突：**单元测试属于 Agent 的职责**（冒烟的一部分），E2E 才归人类。
11. **遇到 `ASSISTANCE.md` 有内容就停下来**：见本文件开头的「开工前先查 `ASSISTANCE.md`」。需要人类做的事（装工具链、授权、提供样本、目视确认、在真机上跑 E2E 等）一律写进 `ASSISTANCE.md`，不要口头带过或自己硬干。

---

## 3. 版本基线（2026-09-15 核实）

| 层 | 选型 | 当前版本 | 备注 |
| --- | --- | --- | --- |
| 外壳 | Tauri | `@tauri-apps/cli` 2.11.4（3.0 处于 alpha） | 分层为 3.0 迁移做准备，见 §6.2 |
| 构建 | Vite | 8.x | RapidRAW 亦用 Vite 8 |
| UI | Solid | **1.9.x（选定）**；`@solidjs/router` 1.0.0 | Solid 2.0 仍是 RC（2.0.0-rc.8），迁移登记 `FUTURE.md` |
| 样式 | Tailwind CSS | 4.3.3 | CSS-first 配置 + CSS 变量令牌层 |
| 组件原语 | **Ark UI**（`@ark-ui/solid`） | 5.39.x | 已定；实测对比见 §7.6 |
| 图标 | **Tabler**（`@tabler/icons-solidjs`） | 3.46.0 | 已定；实测对比见 §7.7 |
| RAW 解码 | rawler 0.8.0（LGPL-2.1-only） | 上游 dnglab | 可插拔后端，候选见 `FUTURE.md` |
| 渲染 | wgpu + WGSL | 需锁定版本 | **版本锁定有前例教训**：RapidRAW 将 wgpu 降到 29.0 以规避 Apple 设备 P3 色偏 |
| DB | SQLite（rusqlite + 迁移工具） | — | 见 §6.4 存储架构 |
| 色彩 | lcms2（预留，后期接入） | — | 第一阶段不做色彩管理 |
| 类型桥 | specta / tauri-specta | — | Rust 类型 → TS 类型，避免手写漂移 |
| Rust 工具链 | **1.98.1**（`rust-toolchain.toml` 锁定，不改全局默认） | rawler 要 1.89，Tauri 3 要 1.95 | 全局默认仍是用户自己的版本，仓库内自动切换 |

---

## 4. 目录结构

```text
raybend/
├── AGENTS.md                  # 本文件：核心信息与纪律
├── ASSISTANCE.md              # 待人类协助事项（有内容则先停下来处理它）
├── DESIGN.md                  # 视觉与配色体系（唯一事实来源）
├── PLAN.md                    # 近中期计划：Milestone（阶段）→ Wave（波次）
├── FUTURE.md                  # 远期方向登记册
├── THIRD-PARTY-NOTICES.md     # 第三方许可登记
├── LICENSE                    # AGPL-3.0-only
├── README.md
├── Cargo.toml                 # [workspace]；profile 也必须在这里
├── rust-toolchain.toml        # 锁定 Rust 1.98.1
├── package.json / pnpm-lock.yaml / vite.config.ts / index.html
├── plans/                     # 各里程碑的详细计划（plans/M0.md …）
├── design/                    # Pencil 设计稿：xxx.pen + xxx.md（同名）
├── implementations/           # 实施记录：YYYY-MM-DD_<简述>.md（文件内首行写精确时间）
├── src/                       # 前端（Solid + Tailwind）
├── src-tauri/                 # Tauri 外壳（窗口 / 命令 / IPC 边界），package.name = "raybend-desktop"
│   ├── tauri.conf.json        # productName = "RayBend"
│   └── src/{main.rs, lib.rs}
└── crates/
    └── raybend/               # 核心库（package.name = "raybend"，不依赖 Tauri）
        └── src/{lib.rs, error.rs, media/, index/, raw/, thumbnail/, render/}
```

`crates/raybend-ipc`（IPC 契约与 specta 生成）在 M1 出现真实契约时再拆，不提前建空壳。

**分层原则**：`src-tauri` 只做「窗口 + WebView + 命令转发」的薄壳，业务逻辑全部在 `crates/` 内，且**业务 crate 不依赖 tauri**。这是为了 Tauri 3.0 迁移（见 §6.2）与未来做 CLI/无头模式时不需要重写。

`crates/raybend` 内的模块按职责划分，后续可能拆分为独立 crate（如 `raybend-raw`、`raybend-cache`、`raybend-jobs`），拆分时机由 PLAN 的里程碑决定。

---

## 5. 纪律细则

### 5.1 设计纪律（`design/`）

- 命名：`design/<模块>-<视图>.pen` 与 `design/<模块>-<视图>.md` 成对存在，**文件名必须一致**。
- `.md` 内容至少包含：视图用途、区域划分、每个区域包含的控件、状态（默认/hover/激活/禁用/空态/加载态）、交互与快捷键、与其它视图的跳转关系。
- 设计稿先行：界面实现前先出 `.pen`，实现过程中如需偏离设计，先改 `.pen` 再改代码。

### 5.2 实施记录纪律（`implementations/`）

- 命名：`implementations/YYYY-MM-DD_<简短英文或中文描述>.md`，例如 `2026-09-16_catalog-schema-init.md`。
- 内容开头必须写明确切的完成时间，精确到秒，例如：

  ```text
  # catalog schema 初始化
  完成时间：2026-09-16 21:34:07 CST
  ```

- 正文至少包含：本次改动的范围、涉及文件、关键决策与理由、验证方式（跑了什么命令/看到什么结果）、遗留问题。
- 一天内多次改动写多个文件；不要追加到同一个文件里堆叠。

### 5.3 构建与运行命令（已实测，2026-09-15）

```bash
# —— WSL 侧（日常开发）——
pnpm install
pnpm tauri dev            # Linux/webkit2gtk 版窗口（经 WSLg 显示）
cargo check --workspace   # Rust 侧快速检查（WSL 侧 target/）

# —— Windows 侧（真实产品环境：WebView2）——
# 前端在 WSL 构建，Windows 只跑 Rust，不需要在 Windows 装 Node。
pnpm build                              # ① WSL 里产出 dist/
export CARGO_TARGET_DIR='C:\rb-target\raybend'
export WSLENV='CARGO_TARGET_DIR'        # ② 跨 WSL→Windows 透传环境变量（cmd 的 set 经互操作不可靠）
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build --workspace'
/mnt/c/rb-target/raybend/debug/raybend-desktop.exe   # ③ 运行（产物在 C: 本地）
```

**三条硬规矩（实测踩坑）：**

1. **Windows 构建的产物必须落在 Windows 本地盘**（`C:\rb-target\...`）。9p 共享（`\\wsl.localhost`）不支持 rustc 增量编译的锁文件语义，会报 `os error -2147024895`，且会把 Windows 产物污染进 WSL 的 `target/`。
2. **跨 WSL→Windows 传环境变量用 `WSLENV`**，不要用 cmd 的 `set VAR=x & ...`（`&` 前的空格会进值，且引号经互操作会丢）。
3. 前端改动后必须重新 `pnpm build`（dist 是编译期嵌入的）；Rust 改动则只需重跑 cargo。

实测参考：首次 Windows 全量构建约 3–4 分钟；WSL 侧 `cargo check` 首次约 2–3 分钟。

---

## 6. 已确定的架构决定

### 6.1 渲染架构 = 原生 wgpu + 透明挖洞（方案 B）

- WebView2 是覆盖整个窗口的一层；照片视口在 webview 中间**挖洞（透明）**，Rust 用 wgpu 直接绘制到窗口表面。
- **接口纪律（防返工的三条红线）**：
  1. 视口状态由 Rust 独有：`Viewport { zoom, pan_px, rotation, fit_mode, clip_rect, dpr, surface_format }`；前端只发送交互意图，不做坐标数学。
  2. 覆盖层（蒙版、裁剪柄、直方图采样框等）使用 Rust 提供的**同一变换矩阵**，禁止前端自行推导像素对齐。
  3. WGSL 源码与 uniform 结构体属于 Rust crate，前端不接触像素格式与色彩空间。
- **分类使用**：Library/网格视图用 DOM 虚拟化（webview 内，便于选中/键盘/拖拽）；Develop 视口用原生 wgpu 直绘。
- **风险与退路**：Tauri 官方未一级支持「webview 上叠加原生 GPU 内容」（相关 issue #8246、#13740），但社区已有多例可用实现（RapidRAW 的 WGPU 直绘、`clearlysid/tauri-wgpu-cam`）。若 Windows 上出现不可解的合成/DWM 问题，退路是切换 Tauri 的 CEF 运行时（自带宽高一致的 Chromium），已登记 `FUTURE.md`。
- 色彩管理**预留不做**：第一阶段只保证 SDR 下不变形，ICC/HDR 在后期里程碑接入。

### 6.2 为 Tauri 3.0 迁移做准备（现在就要遵守）

Tauri 3.0 已进入 alpha（`3.0.0-alpha.0`），已知关键变更：

- **运行时 crate 化**：移除了 `wry` / `cef` feature flag，`tauri::Wry` 被移除，改为显式依赖 `tauri-runtime-wry` / `tauri-runtime-cef`。
  → **纪律**：不要在任何地方引用 `tauri::Wry` 具体类型，也不要在 `Cargo.toml` 里假定 feature flag；把窗口句柄抽象成我们自己的 `NativeWindowHandle` 类型，包在 `src-tauri` 内。
- `tauri-build` 不再把 resources 复制到 cargo target 目录，dev 运行改为从源路径解析资源。
  → **纪律**：资源路径一律走 Tauri 的 path API，不硬编码相对路径，不假设 target 目录里有资源。
- 顶层导出整理（issue #14011）：避免深层 `use tauri::...` 的边角导出，只用稳定的一级 API。
- MSRV 提升到 1.95；Linux 端迁 GTK4（对 Windows 优先的本项目暂不相关）。
- **迁移预案**：等 3.0 进入 beta/rc 时，执行顺序为「先升 `tauri` 与 `tauri-build` → 再改运行时 crate 依赖 → 最后处理插件 API 变动」。因为业务逻辑不在 `src-tauri` 内，预期工作量可控。

### 6.3 RAW 解码层（可插拔）

- 首选 **rawler 0.8.0**（上游 [dnglab](https://github.com/dnglab/dnglab)），但必须在 `raybend-raw` 内做成**后端可插拔**接口，禁止其它模块直接依赖 rawler 类型。
- 已知事实：rawler 是 **LGPL-2.1-only**（已核实与 GPLv3 兼容〔LGPL v2.1 → GPLv3 的转换路径〕，因而可静态链接进本项目的 AGPL-3.0）、**API 不稳定且不遵循 SemVer**、**没有 GPU 依赖（纯 CPU，rayon）**、dnglab README 明说 **Windows 未官方支持**，且明确声明「**不要把 dnglab/rawler 用于处理不可信文件**」。
  → **纪律**：RAW 解码必须在**独立 worker 进程**中执行，崩溃不得带走主进程；对文件做大小/格式预检。
- rawler 提供：CFA 像素、black/white level、白平衡系数、色彩矩阵（`xyz_to_cam` / `color_matrix`）、active/crop area、orientation、以及**嵌入式预览与缩略图**。
- rawler 不提供：高质量去马赛克、降噪、镜头校正、色调映射、色彩管理。这些属于未来的 `raybend-develop`，第一阶段只有最基础处理。
- 第一阶段策略：**优先使用 RAW 内嵌 JPEG 预览**做网格缩略图与快速查看（毫秒级，比解码快 1–2 个数量级），真正解码走后台任务。

### 6.4 存储架构 = 全局库 + 每仓 catalog（分层）

目录布局：

```text
%LOCALAPPDATA%\raybend\
  app.db              # 全局：应用设置 / 仓库注册表 / 全局标签词典 / 任务队列 / 缓存索引
  cache\<repo_uuid>\  # 默认缩略图与预览缓存（可在设置中改路径）
  logs\

<照片仓根目录>\        # 用户指定，可多根
  .raybend\
    repo.json         # 仓标识：uuid / 名称 / 根目录集合 / 策略
    catalog.db        # 主库：资产、元数据、标签、集合、编辑栈、修订
    index.db          # 派生索引（缩略图索引、人脸、相似度）—— 可删可重建
  ...照片...
```

- **真相源规则**：本地编辑/评分/关键词以 **DB 为准**，XMP 只是互操作通道；RAW 永不写回原文件（只写 `.xmp` sidecar）。
- **写并发**：SQLite 单写者 → 采用**单一写者 actor**（专属线程 + 专属连接，所有写操作串行化），读走连接池；批量事务；`PRAGMA journal_mode=WAL, synchronous=NORMAL, busy_timeout=5000, foreign_keys=ON`。
- **跨仓搜索**：`ATTACH` 多库 + `UNION ALL`；必要时在 `app.db` 维护轻量定位表做快速筛选。
- **备份**：升级前自动 `VACUUM INTO` 快照（保留 7 份）；程序版本低于库版本时**拒绝打开**并提示。
- **禁止**把 catalog 放在云同步盘 / 网络盘上 —— 启动时检测并警告（SQLite 数据损坏的头号来源）。

### 6.5 缩略图 / 预览缓存

- 三个尺度：`GRID`(256–512) / `SCREEN`(视口分辨率) / `FULL`(1:1，默认不缓存或只缓存最近 N 张 + pinned)。
- **两种语义必须分离**：中性缩略图（只依赖 RAW，永久缓存）与编辑后渲染（依赖编辑栈，频繁失效）。第一阶段只有中性缩略图。
- 缓存键：`(asset_uid, kind, render_signature)`，`render_signature` 含 `pipeline_ver` → 算法升级后旧缓存自动变孤儿并被 GC。
- 存储形态：512px 网格缩略图放 **SQLite BLOB**（WebP/QOI，20–60KB/张）；SCREEN 级放**文件系统分片目录**（两级十六进制分片）。
- 淘汰策略：容量上限（默认 `max(10GB, 可用空间 10%)`）+ **frecency**（非纯 LRU）+ 保留期 + `pinned` + 孤儿回收；UI 提供缓存面板（按仓/类型/大小展示 + 一键清理）。
- **红线**：删掉整个缓存目录后，功能降级但完全可用。
- 默认位置 `%LOCALAPPDATA%\raybend\cache\`，设置中可按仓覆盖（例如随外接盘以便离线出图）。

---

## 7. 调研挖出的关键真相（必须记住）

### 7.1 SQLite 的边界（结论：够用，风险在写并发）

- 单库上限 281TB；WAL 下「多读者 + 单写者」，现代 NVMe 上写者可达数千 TPS。
- 量级估算：10 万张照片的元数据约 100–300MB，100 万张约 1–3GB（含索引）。行业先例：Lightroom `.lrcat`、darktable `data.db`+`library.db`、digiKam 四个 SQLite 库，没有一个是「SQLite 撑不住」。
- **真正的风险是并发写**（导入 + 缩略图生成 + AI 分析同时写）：用 §5.4 的单写者 actor 解决。

### 7.2 中文搜索

SQLite FTS5 默认 `unicode61` 分词器**对中文基本无效**；必须使用 `tokenize='trigram'`（SQLite 3.34+），英文列可另建 `porter` 索引。

### 7.3 文件身份与跨平台路径

- **不要用路径做主键**：Windows 用 `GetFileInformationByHandleEx(FileIdInfo)` 得到 `(VolumeSerial, FileId128)`，可稳定追踪重命名/移动（NTFS 上可靠）；路径只作展示与 fallback。
- 跨平台注意：**macOS 使用 NFD 规范化且大小写不敏感，Linux 是字节串且大小写敏感，Windows 大小写不敏感**。
  → 现在就定死：路径存储用 **NFC 规范化值 + 保留原始名 + 额外的大小写折叠列**用于唯一索引。

### 7.4 嵌入式预览优先

几乎所有相机 RAW 都内嵌全尺寸 JPEG 预览与缩略图。读取它比解码快 1–2 个数量级。导入与浏览先取内嵌预览，真正做到「瞬间出图」；只有进入 1:1 或显影时才做完整解码。

### 7.5 参考实现：RapidRAW（`https://github.com/CyberTimon/RapidRAW`）

在相片编辑与「Tauri 做图像软件」这件事上做得很好，**找不到方案时优先参考它的实现思路**（注意许可，见下）。
真实技术栈（来自其 `src-tauri/Cargo.toml` 与 `package.json`）：

- **Rust 侧**：`rawler`（**用自己的 fork**：`CyberTimon/RapidRAW-DngLab`）、`wgpu 29.0`（注释：降级以规避 Apple 设备 P3 色偏）、`ort`（ONNX Runtime，`load-dynamic`）+ `tokenizers`（AI）、`image_hasher`（感知哈希/相似度）、`mozjpeg-rs` / `webp` / `jxl-encoder` / `jxl-oxide`（编解码）、`nalgebra` + `glam` + `half`（色彩数学 / f16）、`mimalloc`、`trash`、`kamadak-exif` + `little_exif`、`quick-xml`（XMP）、`fuzzy-matcher`（命令面板模糊搜索）、`gphoto2`（联机拍摄，仅 Unix）。
- **前端侧**：React 19 + **`lucide-react`** + **`react-window`**（网格是虚拟列表，**不是 canvas tile**）+ `konva` / `react-konva`（蒙版与裁剪的 canvas 覆盖层）+ `dnd-kit` + `zustand` + `i18next`（从一开始就国际化）。
- **没有 SQLite 依赖**：它是**sidecar 流派**（编辑状态写文件），没有 catalog 数据库。这正是 raybend 要走与之不同的路线的地方。
- **渲染**：编辑器视口用「透明挖洞 + wgpu 直绘」；官方博文记录改造前后**拖动滑块 20fps → 120fps**，瓶颈原本是「JPEG 编码 → IPC → 浏览器解码」。
- **许可：AGPL-3.0**（已核实 `LICENSE` 为 GNU Affero GPL v3）。
  → **本项目同样采用 AGPL-3.0-only，两边许可一致**：在保留版权与许可声明、并注明来源的前提下，其代码可被借鉴/移植进 raybend（需逐文件确认文件头的许可标注）。
  → 其 fork 的 rawler（`CyberTimon/RapidRAW-DngLab`）仍属 **LGPL-2.1 派生**，可直接作为我们的 rawler 来源（用于吸收其解码修复），使用前确认 fork 新增代码的许可标注。
  → 反向约束：一旦 raybend 对外提供网络服务，就必须按 AGPL 第 13 条向使用者提供对应源码。

### 7.6 组件原语：Kobalte vs Ark UI

两者都是 headless（无样式 + 无障碍）组件库，都要自己写样式（与 Tailwind 搭配无冲突）。区别：

| 维度 | Kobalte | Ark UI |
| --- | --- | --- |
| 出品 | kobaltedev 社区（Solid 原生） | Chakra 团队（`chakra-ui/ark`） |
| 底层 | Solid 原生实现 | 基于 **Zag.js 状态机**，多框架同构（React/Solid/Vue/Svelte） |
| 定位 | SolidJS 专属 UI 工具包 | 跨框架设计系统底座，45+ 组件 |
| 文档/生态 | 面向 Solid 用户 | 文档更完整，跨框架资料多 |
| 风险 | 跟随 Solid 版本演进 | 多框架抽象层厚，Solid 端偶有滞后；但 Zag 的状态机在复杂控件（日期选择、滑块、菜单）上逻辑更严谨 |
| 关键结论 | 若选 Solid 1.9，二者都可用；**Kobalte 更「薄」、Ark 更「全」** | 复杂的组合控件（日期区间、级联菜单）Ark 的状态机更省心 |

决策留到设计阶段（用 Pencil 画出第一批界面控件后再定），结论写入 `PLAN.md` 对应里程碑。

**✅ 已决（2026-09-15）**：本项目选 **Ark UI**（`@ark-ui/solid`）—— 原因是 `splitter`（可拖拽分栏）
与 `tree-view`（标签层级树）是我们最需要且最难自研的两个组件，只有它提供；其活跃度也明显领先。
实测数据（版本号、组件数、活跃度）见 `plans/M0.md` 文末附录。

### 7.7 图标体系（核实结果，2026-09-15）

| 图标集 | 数量 | 风格 | 许可 | 填充态 | 影像领域覆盖 |
| --- | --- | --- | --- | --- | --- |
| **Lucide** | ~1600（sitemap 实测） | 仅描边（24px 网格） | ISC（Feather 派生部分 MIT） | ❌ **官方明确不支持 fill**；可用 `fill=currentColor; strokeWidth=0` 变通，官方 issue 自述「对约 60–70% 图标可用」 | **比预期好**：实测存在 `pipette`(吸管)、`contrast`、`crop`、`frame`、`proportions`、`layers`、`tags`、`folder-tree`、`gallery-thumbnails`、`git-compare`、`badge-check`、`star-half`、`grip-vertical`、`sliders-horizontal`、`combine`、`lasso`、`swatch-book`、`table-2`、`map-pin`(及多种变体)；实测**缺失** `history`、`flip-horizontal` |
| **Tabler** | 6,184（5,130 描边 + **1,054 填充**） | 24px / 2px 描边，描边与填充**成对文件** | MIT | ✅ 独立 filled 文件 + `icon-filled` class | 数量最大，领域图标较全 |
| **Phosphor** | 1,248 图标 × 6 字重 | thin/light/regular/bold/**fill**/**duotone** | MIT | ✅ 官方 fill + duotone，**按 16px 起设计** | 中等 |
| **Heroicons** | ~300（含变体约 888 文件） | outline / solid / mini(20) / micro(16) | MIT | ✅ solid 变体 | 弱，纯通用 UI |
| **Material Symbols** | 2,500+ | outlined / rounded / sharp × **FILL 可变轴**（0↔1 连续插值） | Apache-2.0 | ✅✅ 单一可变字体即可切换 | **最强**：实测含 `raw_on`、`tonality`、`vignette`、`dehaze`、`healing`、`gradient`、`auto_fix_high`、`burst_mode`、`tune`、`straighten`、`exposure`、`add_photo_alternate` |

**结论与纪律**：

- 第一阶段**不需要自绘几十个图标**；原则是「**选定一套 → 只用套内图标做组合 → 降低数量**」。
- 首选 **Tabler**（唯一同时满足「数量大 + 描边/填充成对 + MIT + 小尺寸清晰」）；次选 Lucide（生态最成熟、风格最现代，但激活态需用别的手段表达而非填充）。
- 编辑模块的领域专用图标（histogram / vignette / tonality / dehaze 等）从 **Material Symbols** 借用（Apache-2.0 可与 MIT 混用，但混用需注意描边粗细与网格差异）。
- 图标尺寸规范：16 / 20 / 24 三档，激活态优先用**强调色 + 背景块**表达（Web 设计里有大量成熟做法），不依赖填充变体。
- **✅ 已决（2026-09-15）**：本项目选 **Tabler**（`@tabler/icons-solidjs`）；
  实测数据见 `plans/M0.md` 文末附录。选型原则仍是「只用套内图标做组合、降低数量」，不自绘几十个图标。

### 7.8 Tauri 分发与 npm

- **Tauri 桌面应用不需要发布到 npm**，也不需要占位：分发物是安装包（NSIS / MSI）与自动更新元数据，npm 只服务于 JS 库消费者。`@tauri-apps/cli` 是构建期依赖，不是产品发布物。
- 只有当 raybend 将来对外提供**可被 JS 项目引用的库/插件**（例如 CLI 封装、SDK、编辑器 Web 组件）时才需要占位 npm 包名；届时再评估（当前只能占用 `@cthun/*` 作用域）。
- 待办：代码签名方案（避免 SmartScreen 拦截）与自动更新通道，属于发布里程碑。

---

## 8. 必须处理的问题清单（按优先级）

| # | 问题 | 影响 | 处理时机 |
| --- | --- | --- | --- |
| 1 | **去马赛克与画质算法自研成本高**（GPL 实现需移植而非直接可用） | 决定产品成败，但不属于第一阶段 | 编辑里程碑；现在只记录 |
| 2 | 渲染架构的 Windows 合成/DPI 风险（透明 webview + 原生 GPU 内容） | 返工成本极高 | M0 可行性验证首要项 |
| 3 | rawler 的 LGPL 派生 + API 不稳定 + 无 GPU + 不保证恶意文件安全 | 崩溃/合规 | M0 起就用可插拔接口 + worker 进程隔离 |
| 4 | 文件身份主键与跨平台路径语义定错 → 后期迁移灾难 | 高 | catalog schema 设计时（M1） |
| 5 | catalog 损坏 / 升级丢数据（用户最不可原谅的失败） | 极高 | M1 就引入快照备份与版本闸门 |
| 6 | 范围失控：一人做 ON1 级产品 | 项目死亡 | 严格按 PLAN 的里程碑走，编辑一律后置 |
| 7 | 代码签名 / 分发 / SmartScreen / 自动更新 | 发布期阻塞 | 发布里程碑 |
| 8 | GPU 驱动兼容与回退（DX12/Vulkan/软件渲染）、device lost 恢复 | 稳定性 | M1 渲染层实现时 |
| 9 | 中文分词与本地化（i18n 从第一天留位，参考 RapidRAW 用 i18next） | 后期返工 | M1 UI 骨架 |

---

## 9. 初始化状态

**项目初始化由人类完成**（避免 Agent 在没有对话环境时硬来）。初始化清单见 `PLAN.md` 的「M0 前置：初始化」。

---

## 10. 文档索引

| 文件 | 内容 |
| --- | --- |
| `AGENTS.md` | 本文件：定位、硬约束、版本基线、目录、纪律、架构决定、关键真相、问题清单 |
| `ASSISTANCE.md` | **待人类协助事项清单**（有内容则先停下来处理它） |
| `DESIGN.md` | 视觉与配色体系（唯一事实来源） |
| `PLAN.md` | 近中期开发计划：Milestone（阶段）→ Wave（波次），含初始化清单与完成定义 |
| `plans/M0.md` 等 | 各里程碑的详细计划（planner 产物） |
| `FUTURE.md` | 远期方向登记册：框架迁移、RAW 后端候选、渲染演进、编辑模块、AI、平台扩展 |
| `THIRD-PARTY-NOTICES.md` | 第三方组件与参考项目的许可登记 |
| `design/*.pen` + `design/*.md` | Pencil 设计稿与其说明（成对存在） |
| `implementations/*.md` | 每次改动的实施记录（文件名带日期，内容首行带精确时间） |
