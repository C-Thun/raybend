# raybend — 项目指南（AGENTS.md）

> 本文件是 raybend 的**项目级 AGENTS.md**，只保留**每次会话都需要**的纪律、命令与索引；
> 细节记忆按主题放在 `memory/`（§10 有完整索引与记忆体规范），用到再读。
> 全局约定（包管理、URL 书写、称呼等）见 `~/.pi/agent/AGENTS.md`，本文件不重复。
>
> **需要人类做的事**（装工具链、授权、提供样本、目视确认、真机 E2E 等）**直接在对话里说**，
> 不维护「待办清单」文件；需要留痕的写进 `implementations/` 记录或相应文档，
> 人类做完之后 Agent 要**自己验证**并把结论记进同一处。
>
> 建立时间：2026-09-15 ｜ 最近重构：2026-09-27（记忆体分层：本文件瘦身，细节迁入 `memory/`）

---

## 1. 项目定位

raybend（中文名**「光伴」**，产品名 `RayBend`）是一个**相片管理软件**，目标是成为 ON1 Photo RAW 这类商业软件的优秀开源替代。

**命名规则**：开源/技术侧一律小写 `raybend`（仓库名、路径、URL、存储键、包名）；
**对外产品宣称英文一律 `RayBend`**（界面文案、官网、安装包与窗口标题、分享卡片）；中文名「光伴」。
两层不互相渗透：官网页面上不出现小写 `raybend`（域名与仓库链接除外，那是 URL）。

- **主战场**：相片管理全流程——导入 → 浏览 → 评级/打标 → 筛选/搜索 → 集合整理 → 导出。
- **编辑已是第一阶段主线**（2026-09-21 重排）：M3 = GPU 显影工作台，同一界面对 RAW/JPG 做无损编辑，
  含常规调整、CUBE LUT、SOOC/issue；路线见 `memory/PLAN.md`，技术方向见 `memory/FUTURE.md` D 节。
- **平台**：Windows 优先（Win10/11）；macOS / Linux 属远期。
- **相机支持策略**：不追求最新机型即时适配，跟得上主流即可。
- **许可**：**AGPL-3.0-only**（见根下 `LICENSE`）。本项目主体是本地桌面应用，AGPL 的网络条款只在「对外提供网络服务」时咬合；它带来的好处是 rawler（LGPL-2.1-only）、darktable / RawTherapee（GPL-3.0）与 RapidRAW（AGPL-3.0）的代码都可合法复用。项目以开源方式发布，不计划闭源收费。
- **设计立场**：要有行业软件的质感，但不做 20 年前那种拥挤界面；主动吸收 Web 应用的易用性与高可视化特性。不做 Affinity 的克隆，也不是 Web 应用的桌面壳。

---

## 2. 硬约束（不可违反）

1. **发布与推送必须由人类执行**：`git push`、打 tag、生成/上传发布物、上传到任何包注册表，全部由人操作。
2. **commit 可由 Agent 自行管理**：提交信息用中文，格式 `<type>: <subject>`。
3. **工具链只有 pnpm + cargo**：前端与脚本用 pnpm，Rust 用 cargo。禁止 npm / yarn / bun 或混用锁文件——仓库只允许 `pnpm-lock.yaml` 与 `Cargo.lock`。（`npm install -g` 仅限本机全局 CLI 工具。）
4. **前端不写图像算法**：像素、色彩空间、视口变换、渲染管线全部属于 Rust。前端只负责交互状态、矢量覆盖层与 UI。
5. **不引入 SolidStart**（桌面应用无 SSR 需求；Start v2 面向 Solid v1，与 Solid 2 不配套）。
6. **Pencil 设计纪律**：界面设计用 Pencil 画 `.pen` 存 `design/`，每个 `.pen` 配**同名 `.md`**（§5.1）。
7. **实施记录纪律**：每次编写/改动完成后在 `implementations/` 写一份记录（§5.2）。
8. **测试分工——Agent 只做冒烟，E2E 归人类**：编译通过、命令能跑、进程能起、日志无报错、单元测试通过，到此为止；GUI 交互与视觉正确性、多显示器与 DPI、真实照片库、性能体感、色彩正确性由人类在真实环境确认。**「进程起来了」≠「功能对了」**；报告必须区分「已验证（冒烟）」与「未经人类验证」。E2E 自动化须写成可重复执行的测试代码，不临时手跑。
9. 新增顶层框架或大型依赖前先在会话中讨论，候选方案登记到 `memory/FUTURE.md`。
10. **单元测试必须齐备**（交付模块必须同时交付测试）：覆盖边界（空输入/单元素/上下限/非法值/Unicode 与中文/超长路径/大小写规范化/并发竞争/溢出截断），`cargo test` 与 `pnpm test` 保持秒级；重负载降为小规模合成数据或 `#[ignore]` 手动基准。
11. **遇到难题：先绕过、记录、继续走**。能绕过就把「问题 + 绕法 + 风险」写进 `implementations/` 并在对话里说一声；绕不过去（继续会踩空或污染已完成部分）就先做其它能做的，攒到没有可开工任务时汇总请求协助，不要自己硬试。判据是「绕过之后剩下的事还成不成立」，不是「难不难」。**进度只能往前走。**
12. **复用优先于重复硬编码**——同一个能力只允许有一套实现；「同一个东西两个组件 / 两种表达」本身就是 bug。动手前先找现成的：① 仓里已有的（`components/ui/`、`lib/`、同类工作区那份）→ **读它的源码**（读，不是猜）；② 有 → 改它、一般化它，**绝不写第二份**；③ 没有 → 才新建，并在 `implementations/` 写清「为什么现有的扩不了」；④ 两侧数据形状不同属**适配层**，不是写两份的理由。Pencil 画稿只是参考，优先复用现代组件库。重构是日常动作：改到哪儿顺手收敛到哪儿。
13. **本地应用实时性优先，丢掉 web 式缓存思维**：桌面程序外部的磁盘状态会变，「载入一遍不动」是错的；默认每次进入/展开/重读重新读盘（readdir 是微秒级）。要防的是「疯狂扫描」（同秒反复全盘扫描、无上限递归），不是多扫几遍。实时性与性能冲突时，把「多久算过期」写出来让人类拍板。
14. **一轮清单要连续做完**：人类一次给一批事项 = 「一直做到没有可做的」；停下来只有两种正当理由——真的需要人类协助（直接说）、继续会踩空或污染已完成部分。中间过程只写 `implementations/`，不逐项请示（§5.5）。
15. **新增功能必须评估命令体系接入**：用户可触发的功能要检查是否登记进统一命令注册表（`features/commands/catalog.ts`），进而出现在命令面板（`Ctrl+K`）、快捷键设置与标题栏菜单；❗**默认热键（`defaultKey`）或明确留空（写明理由）必须在同一次改动里给出**，不做事后审计。选键先过冲突与保留规则（`lib/commands.ts::detectConflicts`、`lib/key-chords.ts::RESERVED_CHORDS`）；键位文案走 `keyLabel` / `formatChord`。不是所有功能都必须变成命令，但实施记录要写清「已接入（含热键决定）」或「不接入及理由」。
16. **数据库 schema 变化只走现有迁移框架**（`crates/raybend/src/store/migration.rs` + `store/migrations/*.sql`）：版本闸门、`VACUUM INTO` 迁移前快照、逐条事务、完整性检查。禁止启动时临时 `ALTER`、另造迁移通道、静默删库重建；localStorage 等设备级偏好用**版本化 key + 显式一次性迁移**。
17. **Solid 壳组件透传 children：`untrack` 写在插入点**——`{untrack(() => props.children)}`（就写在 JSX 那一行）；**禁止**提到组件 body 里提前求值。A/B 两面的完整教训、判据与回归脚本见 `memory/ARCHITECTURE.md` §9。
18. **禁止全盘搜索**：不许 `find /`、`grep -r /` 这类从根往下的扫描（会把 `/mnt` 一起卷进来）。先查本仓；第三方源码走确定路径：cargo 依赖在 `~/.cargo/registry/src/index.crates.io-*/`、pnpm 依赖在仓内 `node_modules/`、参考实现在 `/home/andares/repos/refers/<name>/`；不确定路径先 `ls` 父目录一层。
19. **新增/改动功能必须同步评估 XMP 侧影响**（崔总 2026-09-30 定）：凡是改动会进 sidecar 的数据（编辑栈 / issue / 评级 / 色标 / 标签 / 文字 / 地点等）或其存储布局（目录、命名、`_RAW/` 规则），必须在**同一次改动**里判断并接好对 XMP sidecar 写出与读回的影响（规格：`specs/xmp-sidecar.md`），**不得延后欠账**——例：将来改地理位置功能的落地方式时，`photoshop:` / `Iptc4xmpCore:` 那一侧的映射要一起动。

---

## 3. 版本基线（2026-09-15 核实）

> ⚠️ 下表只描述 raybend 软件本体；`website/`（官网）是**另一套选型**，见 §4 与 `website/AGENTS.md`。

| 层 | 选型 | 当前版本 | 备注 |
| --- | --- | --- | --- |
| 外壳 | Tauri | `@tauri-apps/cli` 2.11.4（3.0 alpha） | 分层为 3.0 迁移做准备（`memory/ARCHITECTURE.md` §6.2） |
| 构建 | Vite | 8.x | |
| UI | Solid | **1.9.x**；`@solidjs/router` 1.0.0 | Solid 2 仍 RC，迁移登记 `memory/FUTURE.md` |
| 样式 | Tailwind CSS | 4.3.3 | CSS-first + CSS 变量令牌层 |
| 组件原语 | Ark UI | 5.39.x | 选型对比 `memory/ARCHITECTURE.md` §7.6 |
| 图标 | Tabler | 3.46.0 | 选型对比 `memory/ARCHITECTURE.md` §7.7 |
| RAW 解码 | rawler 0.8.0（LGPL-2.1-only） | 上游 dnglab | 可插拔后端（`memory/ARCHITECTURE.md` §6.3） |
| 渲染 | wgpu + WGSL | 需锁定版本 | RapidRAW 曾降到 29.0 规避 Apple P3 色偏 |
| DB | SQLite（rusqlite + 迁移框架） | — | `memory/ARCHITECTURE.md` §6.4 |
| 色彩 | lcms2（预留，后期接入） | — | 第一阶段不做色彩管理 |
| 类型桥 | specta / tauri-specta | — | Rust 类型 → TS 类型 |
| Rust 工具链 | **1.98.1**（`rust-toolchain.toml` 锁定） | | 不改全局默认 |

---

## 4. 目录结构

```text
raybend/
├── AGENTS.md                  # 本文件：每次会话的纪律核心与索引
├── README.md / README.zh-CN.md # 对外说明（给用户/访客）
├── LICENSE                    # AGPL-3.0 原文（留在根下：GitHub/SPDX 识别）
├── THIRD-PARTY-NOTICES.md     # 第三方许可登记（根下）
├── memory/                    # ★ 记忆库（文件主名全部大写）
│   ├── PLAN.md                # 近期计划（未完成的部分）
│   ├── FINISHED.md            # 已完成部分：摘要 + 证据指针 + 历史附录
│   ├── FUTURE.md              # 远期方向登记册
│   ├── ARCHITECTURE.md        # 架构与工程基线（分层/版本/架构决定/调研/构建排障）
│   ├── DESIGN.md              # 视觉与配色体系（唯一事实来源）
│   ├── REVIEW.md              # 定期评审与方向修正的结论账本
│   └── FUNCTION-<模块>.md     # 模块功能记忆（BROWSE / IMAGING / REPOSITORY，可按需增加）
├── specs/                     # 设计规范：具体需求的总结文档（文件名小写，如 m4-w1.md）
├── design/                    # 设计稿：xxx.pen + 同名 xxx.md 成对
├── implementations/           # 实施报告：YYYY-MM-DD_<简述>.md（首行精确到秒）
├── docs/                      # 给用户/开发者看的信息类文档（指南、手册、发布说明）
├── legal/                     # 协议原文与许可数据（第三方许可文本、overrides）
├── src/                       # 前端（Solid + Tailwind）
├── src-tauri/                 # Tauri 外壳（package.name = "raybend-desktop"）
├── crates/raybend/            # 核心库（不依赖 tauri）
├── website/                   # 官网（独立技术栈，见 website/AGENTS.md）
├── prompts/                   # 功能口述原文（.pd，模块功能意图的原始输入）
└── todos/                     # 临时需求记录（还没开工的复杂需求原文；开工后转 specs/）
```

**分层原则**：`src-tauri` 只做「窗口 + WebView + 命令转发」薄壳，业务逻辑全部在 `crates/` 内且**不依赖 tauri**（为 Tauri 3 迁移与 CLI/无头模式留路）。前端分层与依赖方向见 `memory/ARCHITECTURE.md` §0–§4。

### `website/` 是官网，不是应用本体

独立技术栈（SolidStart 2.0 + Solid 2.0 + Tailwind v4 + Lucide，纯静态站）、独立依赖、
GitHub Actions → Pages 发布，**不进** `pnpm release` 流程。细则见 `website/AGENTS.md`。

---

## 5. 纪律细则

### 5.1 设计纪律（`design/`）

- 命名：`design/<模块>-<视图>.pen` 与同名 `.md` 成对；`.md` 至少含视图用途、区域划分、控件、状态（默认/hover/激活/禁用/空态/加载态）、交互与快捷键、与其它视图的跳转关系。
- **设计稿先行**：界面实现前先出 `.pen`，人类定案后才写代码；实现中要偏离设计，先改 `.pen` 再改代码。

### 5.2 实施记录纪律（`implementations/`）

- 命名 `implementations/YYYY-MM-DD_<简短描述>.md`；内容开头必须写明确切完成时间（精确到秒）。
- 正文至少含：改动范围、涉及文件、关键决策与理由、验证方式（跑了什么命令/看到什么结果）、遗留问题。
- 一天多次改动写多个文件，不堆叠进同一文件。

### 5.3 构建与质量门

```bash
# —— WSL 侧（日常开发）——
pnpm install
pnpm tauri dev            # Linux/webkit2gtk 窗口（WSLg 显示）
cargo check --workspace

# —— 前端质量门（改完就跑）——
pnpm typecheck && pnpm test
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
pnpm build

# —— 运行时冒烟（需另一终端 pnpm dev）——
pnpm smoke:ui [url]

# —— Windows 侧（真实产品环境；日常用封装脚本）——
pnpm debug:win             # build + 跨编译 + check:win + 运行（含 dav1d 环境）
pnpm check:win             # 产物时间与内容核对（见下）
pnpm release [patch|minor|major] --win-msi ...   # 或 --win-nsis（可组合）；打包/publish 由人类执行
# ❗ release 只在**任务清单里真有发版任务**时才跑（崔总 2026-09-28 定）：
#    给崔总出测试产物用 pnpm debug:win（快）；release 一次 15 分钟起，还会顶版本号。
```

**关键红线**（完整排障手册、十条硬规矩与事故案例在 `memory/ARCHITECTURE.md` §8）：

- **先 `pnpm build` 再构建 Windows，构建完必须 `pnpm check:win`**——`dist/` 是编译期嵌进 exe 的；
  「主程序新、worker 旧」也要防（`-p raybend-desktop -p raybend` 一起建，协议版本握手会报错）。
- Windows 构建产物必须落在 Windows 本地盘（debug 默认 `C:\rb-target\...`；release 自动探测 `%LOCALAPPDATA%\raybend\build`，可用 `--win-dir` 覆盖）；跨 WSL 传环境变量用 `WSLENV`；
  `--features custom-protocol` 必须加（否则白屏）；启动 exe 直接用 `/mnt/c/...` 路径，不经 `cmd start`。
- dav1d 静态库环境变量已封进 `scripts/lib/dav1d-win.mjs`，别在别处再写一份。
- `pnpm clean:win` / `clean:wsl` 清理只进不出的 target 产物（cargo 从不回收旧单元）。
- 换图标后「看起来没换」多半是 Windows 图标缓存——用 exe 资源字节比对判定，别用眼睛。

### 5.4 规划纪律

- **一次规划只覆盖一个工作单元**（最小颗粒度 = 一个波次）；`memory/PLAN.md` 只做路线级描述，
  具体方案开工前写入 `specs/<milestone>-<单元>.md`；不提前细化未开工单元。
- 完整规划原则见 `memory/PLAN.md` §0。specs 里的勾选项是**验收清单**：做完即勾、状态如实；
  工作进度的唯一事实来源是 `todo`（全局纪律）。历史文件中对 plannotator 的提及是当时流程的记录，不回改。
- 目录名沿革：`specs/` 原名 `plans/`（2026-09-26 改名）；`implementations/` 历史记录里的旧写法不改。

### 5.5 连续工作范围

- 规划任务列表用 pi 的 `todo` 工具（单份事实来源，纪律见 `~/.pi/agent/AGENTS.md`）。
- **规划好后必须做到列表任务全部无法进行时再停下**；无法进行的判据：需人类介入（E2E、授权、设计意图缺失等），或同一任务尝试超 5 次仍难解。
- 此时**不是停止工作**：跳过卡住的部分，继续做清单里其它能做的，直到只剩需要人类协助的部分，再一次性汇总汇报。

### 5.6 评审纪律（`memory/REVIEW.md`）

`REVIEW.md` 记「每轮 review 的判断、取舍与去向」，只记结论与落点，**规格正文写回拥有它的那份文件**；
维护规则见该文件 §0。结论归人类拍板，Agent 不得把「待决」自行改成「已采纳」。

---

## 6. 已确定的架构决定

> 本节是**摘要**；完整论证、目录布局、风险与退路在 `memory/ARCHITECTURE.md` §6。

- **6.1 渲染架构 = 原生 wgpu + 透明挖洞（方案 B）**：WebView2 覆盖全窗，照片视口在 webview 中间挖透明洞，Rust/wgpu 绘制其下原生 GPU 底板（Windows 产品默认 **DX12 + DirectComposition visual + Opaque**）。Library/网格用 DOM 虚拟化，Develop 视口用原生 wgpu 直绘。**接口三条红线**：① 视口状态由 Rust 独有，前端只发交互意图；② 覆盖层用 Rust 提供的同一变换矩阵，禁止前端自行推导像素对齐；③ WGSL 与 uniform 属于 Rust crate，前端不接触像素格式与色彩空间。透明链是硬约束：`html`/`body` 必须常驻透明。坐标契约（四种量不得混用、DPR 读运行时值）见 §7.9 与 `docs/native-viewport-coordinate-guide.md`。
- **6.2 为 Tauri 3.0 迁移做准备**：不引用 `tauri::Wry` 具体类型与 feature flag，窗口句柄包成自己的 `NativeWindowHandle`；资源路径走 Tauri path API。
- **6.3 RAW 解码层（可插拔）**：首选 rawler 0.8.0，后端可插拔接口在 `raybend` 内，禁止其它模块直接依赖 rawler 类型；**解码必须在独立 worker 进程**执行（崩溃不带崩主进程），对文件做大小/格式预检；第一阶段优先用内嵌 JPEG 预览做缩略图（快 1–2 个数量级）。
- **6.4 存储架构 = 全局库 + 每仓 catalog（分层）**：`%LOCALAPPDATA%\com.cthun.raybend\`（app.db、备份、缩略图缓存、luts、日志）+ 库根（catalog.db 真相源、photos/、cache/full/ 大图）。真相源规则：本地编辑以 DB 为准、XMP 只是互操作通道、RAW 永不写回。写并发 = 单写者 actor + 读连接池 + WAL。库身份 = catalog.db 内唯一 ID，app.db 记多路径；路径挂了哪个库靠**读 ID 比对**。禁止 catalog 放云同步/网络盘。完整目录布局与规则见 `memory/ARCHITECTURE.md` §6.4，业务规格见 `memory/FUNCTION-REPOSITORY.md`。
- **6.5 缩略图 / 预览缓存**：三种图（thumb 384 / 胶片带 192 / 预览 1920）的规格与生成节点见 `memory/FUNCTION-IMAGING.md`（唯一事实源）。小图在 `%LOCALAPPDATA%\...\cache\_sources\thumbs.db`，1920 AVIF 在库根 `cache/full/`；AVIF 质量 90 / 4:4:4。红线：删掉整个缓存目录后功能降级但可从原图重建；`app.db` / `catalog.db` / LUT 本体不是可删缓存。

---

## 7. 调研挖出的关键真相

> 完整调研记录在 `memory/ARCHITECTURE.md` §7；这里只留一句话结论 + §7.9 坐标契约摘要。

- **7.1 SQLite**：够用（行业先例 Lightroom/darktable/digiKam），真风险在写并发——用单写者 actor 解决。
- **7.2 中文搜索**：FTS5 必须 `tokenize='trigram'`（unicode61 对中文无效）。
- **7.3 文件身份**：不用路径做主键（Windows `FileIdInfo`）；路径 NFC 存储 + 原始名 + 大小写折叠列。
- **7.4 嵌入式预览优先**：读内嵌 JPEG 比完整解码快 1–2 个数量级。
- **7.5 参考实现 RapidRAW**（https://github.com/CyberTimon/RapidRAW ）：Tauri 图像软件的实证（wgpu 直绘 20fps→120fps）；AGPL-3.0 与本项目兼容，找不到方案时优先参考（注意其 rawler 是 LGPL fork）。
- **7.6 / 7.7 已决选型**：Ark UI（splitter/tree-view 只有它提供）+ Tabler 图标（描边/填充成对、MIT、6 千余枚）；实测数据见 `memory/FINISHED.md` 附录 A.10 与 `memory/ARCHITECTURE.md` §7.6/§7.7。
- **7.8 Tauri 分发**：桌面应用不发 npm；签名与更新通道属发布里程碑（R3-01 已拍板 unsigned 直下）。

### 7.9 原生视口的坐标契约（2026-09-19 真机血泪；完整报告 `docs/native-viewport-coordinate-guide.md`）

**四种量不能混用**：

| 量 | 来源 | 用途 |
| --- | --- |
| native scale | Tauri `scale_factor()` | 原生窗口逻辑尺寸、诊断 |
| **WebView DPR** | `window.devicePixelRatio` | **DOM CSS → surface 物理像素**（含显示 DPI × 文字缩放 × 页面缩放） |
| DOM viewport | `window.innerWidth/innerHeight` | WebView 实际 CSS 视口 |
| image zoom | Rust `Viewport.zoom` | 图像像素 → 物理像素（1:1 恒为 1.0） |

铁律：**前端只上报原始事实**（rect、clientX/Y、DPR），物理换算全在 Rust；**DPR 必须读运行时值**
（本机 125% × 文字 110% = 1.375，写死必错）；别用 `screenX/Y × dpr` 猜容器偏移。
诊断红旗与验证顺序见 `memory/ARCHITECTURE.md` §7.9。

---

## 8. 必须处理的问题清单

已迁至 `memory/PLAN.md` §5（按优先级、含 2026-09-27 状态核对）。

## 9. 初始化状态

项目初始化已完成（M0，2026-09-15，由人类执行）；记录见 `memory/FINISHED.md`。

---

## 10. 文档索引与记忆体规范

**根下只放三份**：`README.md`（对外）、`AGENTS.md`（本文件）、`THIRD-PARTY-NOTICES.md`（许可登记）、`LICENSE`（协议原文，留给工具链识别）。

**记忆体规范**：agent 记忆存 `memory/`，**文件主名全部大写**；与具体模块功能相关的记忆一律
`memory/FUNCTION-<模块>.md`（现有 BROWSE / IMAGING / REPOSITORY，新模块照此增加）。
本表以外的目录/文件不属于记忆体（如 `prompts/` 是口述原文输入，`legal/` 是协议存放处，`todos/` 是**临时需求记录**）。

**`todos/` 约定（崔总 2026-09-28 定）**：复杂需求先在对话里接到、还没开工的，除了登记进
pi 的 todo 工具，**同时把需求原文落一份到 `todos/`**（文件名 `YYYY-MM-DD-<简述>.md`，
内含原文、背景、落点提示与验收要点）—— 防「todo 工具里的描述丢细节」。开工时以它为源
写 `specs/`，做完删除或移走；它是需求暂存，不是进度表（进度只看 todo 工具），也不是 spec。

| 文件 / 目录 | 内容 |
| --- | --- |
| `AGENTS.md` | 本文件：定位、硬约束、目录、纪律核心、术语、索引 |
| `memory/PLAN.md` | 近期计划（未完成排期）、问题清单、决策记录 |
| `memory/FINISHED.md` | 已完成部分摘要 + 证据指针 + 历史路线附录 |
| `memory/FUTURE.md` | 远期方向登记册（框架迁移、RAW 后端、渲染演进、AI、平台扩展） |
| `memory/ARCHITECTURE.md` | 架构与工程基线：前端分层、版本基线、架构决定、调研存档、构建排障、untrack 纪律 |
| `memory/DESIGN.md` | 视觉与配色体系（唯一事实来源） |
| `memory/REVIEW.md` | 定期评审与方向修正（结论账本，维护规则见其 §0） |
| `memory/FUNCTION-BROWSE.md` | 浏览模式规格：三列结构、toolsbar、选择逻辑、看图/对比、胶片带、标签、浮层、术语细则 |
| `memory/FUNCTION-IMAGING.md` | 图像格式与显示规格（唯一事实源）：格式范围 + 三种图规格 + issue/latest 切换语义 |
| `memory/FUNCTION-REPOSITORY.md` | 库与导入规格：库身份、导入模版、序号、重名、RAW 分流、目录透传 |
| `specs/` | 单工作单元的详细方案与需求规格（`<milestone>-<单元>.md`，文件名小写） |
| `design/*.pen` + `.md` | Pencil 设计稿与说明（成对） |
| `implementations/*.md` | 实施记录（文件名带日期，内容首行带精确时间） |
| `docs/` | 给人看的信息文档（用户指南、隐私、发布手册、坐标契约报告等） |
| `legal/` | 协议原文与许可数据（说明见 `legal/README.md`） |
| `website/AGENTS.md` | 官网专属指南（独立技术栈） |
| `docs/native-viewport-coordinate-guide.md` | 原生视口坐标契约完整报告（动原生视口前必读） |

---

## 11. 术语约定（词汇表）

> 代码、设计稿、文档、提交信息一律用本表叫法；详细视觉规则见 `memory/DESIGN.md`，
> 界面结构细则见 `memory/FUNCTION-BROWSE.md`。

### 11.1 界面区域

| 术语 | 含义 | 对应节点 |
| --- | --- | --- |
| **`titlebar`** | 最上一条：应用图标/名、菜单、拖拽区、主题/密度开关、窗口三键 | `TitleBar` |
| **`flowbar`** | 第二条：工作流切换（导入/浏览/编辑/导出）+ 图片信息区 + 开关组 | `FlowBar` |
| **`toolsbar`** | 第三条：工具按钮，**内容居中**、随工作流变、无内容整行隐藏 | `ToolsBar` |
| **`workspace`** | 三条下面的工作区，完全跟着工作流走 | `Workspace` |
| **工作流**（flow） | 导入 / 浏览 / 编辑 / 导出，**有序流水线** | `FlowChip` |
| **`statusbar`** | 中列最底部的状态/控制位置；**位置术语**，不保证对应同一组件 | `…StatusBar` |

**toolsbar 三段式**（left / center / right，人类 2026-09-23 定）：mid 按整窗居中占满整条；
left / right 叠在 mid 之上各自贴边（分别装 workspace 左列 / 右列相关的**面板开关**），
不参与 mid 宽度计算；挤压时 left/right 不透明底遮住 mid、内侧 20px 渐隐；无内容整行隐藏。
一组互斥开关：按一个关其它，再按同一个整组关掉；任何档位都允许鼠标开回来。
细则与画稿见 `memory/FUNCTION-BROWSE.md` §13.1 与 `design/editor.md`。

**statusbar 命名规则**：说 `statusbar` 必须带当前 mid 内容前缀（`tiles statusbar` / `view statusbar` /
`film statusbar`）——它**跟 mid 组件走，不跟 flow 走**；同一套 tiles 在 import / browse 都叫
`tiles statusbar`。view 与 film statusbar 服务单张照片，视为同一个东西。

**workspace 结构红线**（人类 2026-09-20，不可违反）：workspace 下**只有纵向分列，没有跨列行**，
每条列自己到底；一切「底部信息条」住在它所属的那一列里。
（三处同源记述：本节 + `memory/DESIGN.md` §8.7 + `memory/FUNCTION-BROWSE.md` §5.8。）

### 11.2 勾选（check）vs 选中（select）——必须分清

| 术语 | 含义 | 表现 | 范围 |
| --- | --- | --- | --- |
| **勾选** | 「已加入待处理集合」 | 左侧圆形主色勾选框 | 可多选，互不影响 |
| **选中** | 「当前正在看/正在操作的对象」 | 整行/整片主色底 | **同一时间全局只有一个** |

两者互相独立；勾选要显式点圈，选中是「点行本身」的结果；跨面板的选中状态是同一个状态，必须同步。
❗ 不要因为某目录被选中而强制展开树（`memory/DESIGN.md` §12.4.1）。

### 11.3 移除 / 排除 / 删除（三者不同，统一用禁行图标，不用叉号）

| 术语 | 含义 | 弹窗 |
| --- | --- | --- |
| **移除**（remove） | 从**当前集合**里拿掉 | 默认要，`Shift` 跳过（`easy destroy`） |
| **排除**（exclude） | 本次导入不带这张（不动库、不动磁盘） | 同上 |
| **删除**（delete） | **移到系统回收站**（不提供永久删除；先文件后记录，回收失败不动 DB） | 要（**无 `Shift` 快通道**，它真动磁盘） |

### 11.4 组件 × 工作流的组合表达

四个工作流 `import` / `browse` / `edit` / `export`（顺序即流水线）。workspace 中列组件：

| 词 | 指什么 |
| --- | --- |
| **`tiles`** | 图片列表组件（网格/列表，一次多张） |
| **`view`** | 单张图片查看 |
| **`film`** | 上看图 + 下横向胶片带 |
| **`editor`** | Rust 原生 GPU 驱动的编辑界面 |

位置组合：`import left` / `browse mid` / `export right`（位置级）；更常说 `browse tiles` / `browse view` / `browse film`。
这套词只描述位置与职责，不指定实现文件。

**结构红线**（详见 `memory/FUNCTION-BROWSE.md` §5.1/§5.8/§13.2）：
import 与 browse 的照片区必须经同一个 `PhotoViewingStage` / `PhotoViewingController`；
`PhotoGrid` 进 view / film / compare **只能隐藏，不能卸载**；胶片带选择只调 `TilesSource.select / setAnchor`；
胶片带尺寸组件只有一份、偏好按工作流分开持久化；`TilesControlBar` 只能由 `TilesShell` 装配
（受控滑杆不得重建它的 DOM——untrack 纪律见 §2.17 / `memory/ARCHITECTURE.md` §9）。

### 11.5 其他常用术语

| 术语 | 含义 |
| --- | --- |
| **库**（repository） | 用户选择的相片仓根目录；**没有库就无法导入** |
| **来源**（source） | 待导入的目录（磁盘 / NAS / 云） |
| **最近**（Recent） | 自动记录的最近 50 个导入目录 |
| **已选目录** | 已勾选、准备导入的目录集合（横条列表） |
| **`shortpath`** | 路径缩写：盘符 + 中间各级首字母 + 末级全名（`memory/DESIGN.md` §12.3） |
| **`flowinfo`** | flowbar 右侧图片信息区；**跟随当前工作流**，没选中就清空 |
| **`easy copy`** | 指向信息组 → 细边框 +「点击复制」气泡；点击后变「已复制」 |
| **`easy destroy`** | 移除类快速通道：默认弹确认，`Shift` 跳过 |
| **反转**（inversion） | 单击 ↔ `Ctrl`+单击可对调的**固有能力**（开关挂在数据源上，按工作流/面适配）；判定唯一处 `lib/selection.ts::clickMode`。口径 `memory/FUNCTION-BROWSE.md` §5.2.4 |
| **回退**（fallback） | `Shift`+`Ctrl`+单击**不是操作**（设定上不存在）；该组合被捕获并回退到当前场景的单击（标准=replace，反转=toggle）。**没有自己的语义，不要为它设计行为** |
| **`issue`（定稿）** | 一张图的一个非破坏性定稿版本，可多个、各有独立预览；`SOOC` 是写死的特殊 issue（相机直出 JPG）。见 `memory/FUNCTION-IMAGING.md` 与 `specs/issue-xmp-contract.md` |
| **取消 / 确认按钮顺序** | 全系统统一：**取消在左，确认（往前进类）在右** |
| **中性面**（neutral surface） | 四级灰面：`surface-track` < `surface-main` < `surface-bar` < `surface-layer` |
| **主色底 / 辅色底** | 全局反馈：**指向=辅色底，点击后=主色底** |
| **紧凑 / 宽松** | 全局两档密度；只影响间距，不影响字号与图标大小 |
| **按时间分组 / 时间片** | 同一拍摄日内相邻间隔 > 1 小时断为新片（`memory/DESIGN.md` §12.7） |
| **内嵌预览**（embedded preview） | 相机写在 RAW 里的 JPEG 预览，提取快 1–2 个数量级 |
| **编辑栈**（develop stack） | 全部非破坏性编辑操作的序列；定稿落成 issue |
| **`fullscreen`** | 无 UI 沉浸式单图浏览（独立无边框窗口，`F11`）；与主窗口看图态是两回事 |
| **`shortcut`（快捷键）** | 命令注册表里的默认键位（`CommandSpec.defaultKey`）；新功能必选项（§2.15） |
| **动态反差**（Dynamic Contrast） | 一根拉杆的局部色调映射（`dynamicContrast`，0–100 单极）；与全局 `contrast` 不是一回事 |
