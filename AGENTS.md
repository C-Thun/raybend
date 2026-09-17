# raybend — 项目指南（AGENTS.md）

> 本文件是 raybend 的**项目级 AGENTS.md**：核心信息、硬纪律、目录说明、记忆/文档索引，以及调研阶段挖出的**必须处理的关键问题**。
> 全局约定见 `~/.pi/agent/AGENTS.md`（包管理、URL 书写、plannotator 流程等），本文件不重复。
>
> 建立时间：2026-09-15 00:18:03 CST
> 相关文档：近中期计划见 `PLAN.md`，远期方向见 `FUTURE.md`，**视觉与配色体系见 `DESIGN.md`**，**待人类协助事项见 `ASSISTANCE.md`**。

<!-- -->

> [!IMPORTANT]
> **开工前先查 `ASSISTANCE.md` 的「一、现在需要你做」。**
> **只要那一节非空**，任何任务在真正开始前（哪怕只是规划阶段）必须立即停止，
> 并向用户提示先协助完成；待用户确认后，Agent 需**逐项验证**才可继续推进。本规则优先于本文件中其他一切流程约定。
>
> **例外：`website/`（官网）不受这条约束。** 官网的开发**不被 `ASSISTANCE.md` 阻塞**；
> 官网需要人出手的事（生成图片素材、截图、配 DNS / Pages 设置等）**直接在对话里说**，
> 不要往 `ASSISTANCE.md` 里写 —— 那份清单只管**应用本体**（用户 2026-09-17 定）。
>
> 该节**为空**则照常推进：`ASSISTANCE.md` 的其余部分（「待你定夺」「需要你目视验证」）是**排队等时机**的，
> 不阻塞日常开发 —— Agent 到各自时机要主动提示，不要拖到收尾才说。
> （2026-09-15 修订：原规则是「文件有内容就停」，结果非阻塞条目越积越多、「停」成了常态，
> 铁律反而失效；现按「是否真的阻塞」分级。）

---

## 1. 项目定位

raybend（中文名**「光伴」**，产品名 `RayBend`）是一个**相片管理软件**，目标是成为 ON1 Photo RAW 这类商业软件的优秀开源替代。

**命名规则（用户 2026-09-17 定）**：开源/技术侧一律小写 `raybend`（仓库名、路径、URL、存储键、包名）；
**对外产品宣称英文一律 `RayBend`**（界面文案、官网、安装包与窗口标题、分享卡片）；中文名「光伴」。
两层不要互相渗透：官网页面上不出现小写 `raybend`（域名与仓库链接除外，那是 URL）。

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
11. **遇到难题：先绕过、记录、继续走；攒到绕不过去再统一报告**（用户 2026-09-16 定）。
    碰到实在不好解决的问题时**立刻评估**一次：这是不是已经触到能力边界？
    - **能绕过** → 绕过它，把「问题 + 绕法 + 风险」记进 `ASSISTANCE.md` 的
      **「二、绕过去了的问题」**，然后**继续往前走**。不要在单个问题上原地磨。
    - **绕不过去**（继续走会踩空，或者会把已经做完的部分污染掉）→ 立刻停下来，
      写成「一、现在需要你做」里的一条，交人类找外援。**此时不要自己硬试** ——
      硬试的代价通常是把干净的部分也弄脏。
    - 判据不是「难不难」，而是**「绕过之后剩下的事还成不成立」**。
    - 例：某条命令在 Windows 上偶发失败但重试能过 → 绕过并记录；
      某个依赖根本没有 Windows 实现 → 停下来问。
    目的：**进度只能往前走**。把一堆小问题攒成一次汇报，比每碰到一个就停一次有效得多；
    但也不能攒着不说 —— 「绕不过去」就是必须开口的那一刻。

12. **`ASSISTANCE.md` 的「一、现在需要你做」非空就停下来**：见本文件开头的「开工前先查 `ASSISTANCE.md`」。
   需要人类做的事（装工具链、授权、提供样本、目视确认、在真机上跑 E2E 等）一律写进 `ASSISTANCE.md`
   **对应分节**（阻塞 / 待定夺 / 需验证），不要口头带过或自己硬干；
   人类做完后 Agent 要**验证并把该条压缩归档**，让「现在需要你做」这一节始终保持短小 —— 它空着就是最好的状态。

   **例外：`website/`（官网）不走这套。** 官网的开发**与本条无关、不被 `ASSISTANCE.md` 阻塞**；
   官网需要人做的事（图片素材 / 截图 / 域名与 Pages 配置等）**直接在对话里向用户提**，
   不写进 `ASSISTANCE.md`（那份清单只服务应用本体）。见 `website/AGENTS.md`。

---

## 3. 版本基线（2026-09-15 核实）

> ⚠️ **下表只描述 raybend 软件本体。** `website/`（官方站点）走的是**另一套完全不同的选型**
> （Solid 2.0 线、Tailwind v4、Lucide、纯静态站），对照见 §4 的「`website/` 是官网」。

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
├── plans/                     # 单工作单元的计划（plans/M1-5.md；一次一个，见 §5.4）
├── design/                    # Pencil 设计稿：xxx.pen + xxx.md（同名）
├── implementations/           # 实施记录：YYYY-MM-DD_<简述>.md（文件内首行写精确时间）
├── src/                       # 前端（Solid + Tailwind）
├── src-tauri/                 # Tauri 外壳（窗口 / 命令 / IPC 边界），package.name = "raybend-desktop"
│   ├── tauri.conf.json        # productName = "RayBend"
│   └── src/{main.rs, lib.rs}
├── website/                   # ⚠️ 官方站点（**独立技术栈**，见下方「`website/` 是官网」）
└── crates/
    └── raybend/               # 核心库（package.name = "raybend"，不依赖 Tauri）
        └── src/{lib.rs, error.rs, media/, index/, raw/, thumbnail/, render/}
```

`crates/raybend-ipc`（IPC 契约与 specta 生成）在 M1 出现真实契约时再拆，不提前建空壳。

### `website/` 是官网，不是应用本体（**不要弄错**）

**`website/` 是本项目的官方站点**（官网）源码；**应用本体**是上面的 `src/` + `src-tauri/` + `crates/`。
两者**技术栈不同、依赖各自独立、构建与发布流程也不同**：

| 维度 | 应用本体 | 官网（`website/`） |
| --- | --- | --- |
| 框架 | SolidJS **1.9.x** + `@solidjs/router` 1.x | **SolidStart 2.0 + SolidJS 2.0**（`@solidjs/vite-plugin` 的 start 模式 + `filesystem-routing` + `@solidjs/router` 2.x） |
| 样式 | Tailwind CSS 4.3 + CSS 变量令牌层 | Tailwind CSS v4（CSS-first，**没有** `tailwind.config.js`） |
| 组件原语 / 图标 | Ark UI + **Tabler** | 不用组件原语库；图标用 **Lucide**（`lucide-solid`） |
| 形态 | Tauri 2 桌面应用（Windows 优先） | **纯静态站点**，与 Tauri 无关 |
| 发布 | 人类打安装包（NSIS / MSI） | **GitHub Actions 构建 → 发布到 GitHub Pages**（GitHub 的 pages 服务） |

- **依赖各自独立**：官网有自己的 `package.json` / `pnpm-lock.yaml` / `pnpm-workspace.yaml` / `node_modules`，
  **不在本仓的 pnpm 关系内** —— 根目录 `pnpm install` 不会装上它，改官网要在 `website/` 里单独跑命令。
- **本文件其余部分（§3 版本基线、§5 纪律、§6 架构决定、§7 调研结论）默认只约束应用本体**，
  不要把本体的选型或纪律硬套到官网（官网不用 Ark UI / Tabler，也不是 Tauri 应用）。
- 官网编译产物是 `website/dist/client` 的**纯静态文件**，**不进** `pnpm release` / Tauri 发版流程；
  它的构建与发布由 **`.github/workflows/website.yml`** 负责（push `master` / 正式版 release / 手动触发 →
  GitHub Pages），一次性配置与自定义域名步骤见 `website/AGENTS.md` §5。
- 官网的细则（选型版本、目录约定、命令、部署、Solid 2 反应式纪律）见 **`website/AGENTS.md`**。

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

# —— 前端质量门（改完就跑这几条；CI 尚未建立，先靠习惯）——
pnpm typecheck            # 类型检查
pnpm test                 # 单元测试（node --test，零依赖，应当保持在秒级）
pnpm lint:colors          # 色值只允许出现在 tokens.css
pnpm lint:arch            # 分层依赖方向（ARCHITECTURE.md §1/§2）
pnpm lint:i18n            # 界面文案只允许出现在语言包（DESIGN.md §11.1）
pnpm build                # 生产构建

# —— 运行时冒烟（需另一个终端先 `pnpm dev`）——
pnpm smoke:ui                  # 默认打应用外壳；也可传 URL
pnpm smoke:ui http://localhost:1420/dev/kitchen-sink

# —— 发版（只准备产物；**不** commit / tag / push，那些由人做）——
pnpm release test              # 自测包（版本号不变）
pnpm release patch --dry-run   # 先看计划

# —— Windows 侧（真实产品环境：WebView2）——
# 前端在 WSL 构建，Windows 只跑 Rust，不需要在 Windows 装 Node。
pnpm build                              # ① WSL 里产出 dist/
export CARGO_TARGET_DIR='C:\rb-target\raybend'
export WSLENV='CARGO_TARGET_DIR'        # ② 跨 WSL→Windows 透传环境变量（cmd 的 set 经互操作不可靠）
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build -p raybend-desktop --features custom-protocol'
/mnt/c/rb-target/raybend/debug/raybend-desktop.exe   # ③ 运行（产物在 C: 本地）
```

**四条硬规矩（实测踩坑）：**

1. **Windows 构建的产物必须落在 Windows 本地盘**（`C:\rb-target\...`）。9p 共享（`\\wsl.localhost`）不支持 rustc 增量编译的锁文件语义，会报 `os error -2147024895`，且会把 Windows 产物污染进 WSL 的 `target/`。
2. **跨 WSL→Windows 传环境变量用 `WSLENV`**，不要用 cmd 的 `set VAR=x & ...`（`&` 前的空格会进值，且引号经互操作会丢）。
3. 前端改动后必须重新 `pnpm build`（dist 是编译期嵌入的）；Rust 改动则只需重跑 cargo。
4. **`--features custom-protocol` 必须加，否则 Windows 版会白屏/页面打不开**。
5. **启动 Windows 的 exe 直接用 `/mnt/c/...` 路径跑，不要经 `cmd.exe /c "start \"标题\" 路径"`**。
   实测：那种写法的引号会被 WSL→cmd 的互操作吃掉，结果弹出 **「Windows 找不到文件 '\RayBend\'」**——
   参数被切碎，一个标题字串被当成了文件路径。直接跑最稳：

   ```bash
   (cd /mnt/c/rb-target/raybend/debug && ./raybend-desktop.exe >/dev/null 2>&1 &)
   ```

   （若确实需要用 `start`，就得接受这层引号嵌套很难写对；直接执行免去全部转义问题。）

6. **`src-tauri/tauri.linux.conf.json` 是开发期便利，不是最终形态**（M1-4 加的）：
   它保留系统标题栏，只为 WSLg 下还能拖边缩放（产品在 Windows 上是沉浸式 `decorations: false`）。
   **发布 Linux 版之前必须处理**（`FUTURE.md` G11）。
   另：Tauri 的平台配置合并是 **RFC 7396 merge patch** —— **数组会被整体替换**，
   所以那个文件必须重复整份窗口配置，改主配置的窗口尺寸时别忘了同步它。

7. **先 `pnpm build` 再构建 Windows，构建完必须 `pnpm check:win`**（2026-09-16 血的教训）：
   `dist/` 是**编译期嵌进 exe** 的，改了前端不重建 dist，产物里就是旧界面 ——
   而当时「exe 里有资源名」的核对是**假绿**（dist 自己旧，旧名字当然对得上），
   结果让人类拿着「没有修复的版本」白测一轮。
   `pnpm check:win` 既比**时间**（exe 必须比 dist 新）也比**内容**（资源名逐个命中），
   不合格会直接打印补救命令。

### 5.3.1 为什么必须加 `--features custom-protocol`（重要，别拆掉）

已定位到源码级。`tauri` 的 `build.rs`：

```rust
let custom_protocol = has_feature("custom-protocol");
let dev = !custom_protocol;        // ← dev 由 feature 决定，不是 debug/release！
```

而 `tauri-codegen` 在 `dev == true` 且配置了 `devUrl` 时，**产出的嵌入资源是空的**：

```rust
} else if dev && config.build.dev_url.is_some() {
    let assets = EmbeddedAssets::default();   // ← 空资源
```

于是 webview 只能去连 `devUrl`（`http://localhost:1420`）—— 本机没服务时就报「无法访问此页面」。

**表现对照**：

| 路径 | 命令 | 结果 |
| --- | --- | --- |
| WSL 开发 | `pnpm tauri dev` | ✅ 正常（Vite 在 1420 上服务，且 CLI 不加该 feature → `dev=true` 是对的） |
| WSL 开发 | `cargo run` | 同上（**也会去连 1420**，所以不走这条） |
| Windows 裸 `cargo build` | 无 feature | ❌ 窗口出来了但页面打不开 |
| Windows 生产 | `--features custom-protocol` | ✅ 嵌入真实资源 |

> Tauri CLI 的 `tauri build` 会自动加这个 feature；**但我们的 Windows 构建路径不走 CLI**，必须手写。
> `src-tauri/Cargo.toml` 的 `[features]` 段里已注明这一点 —— **不要当模板残留删掉**
> （M0-1 “删净模板演示”时曾把它误删，直接导致了这个 bug）。

### 5.3.2 Windows 侧验证纪律

**“窗口出现了”不等于“功能对了”。** Windows 侧跑完必须至少确认：

1. 页面**真的画出来了**（不是白屏/错误页）—— **由人类目视**（`AGENTS.md` §2.8）
2. 产物**比 `dist/` 新**（否则跑的可能是旧前端）
3. Agent 侧可做的程序化冒烟：检查 exe 里含的是 **`dist/assets/` 当前的资源文件名**
   （含则说明资源确实被嵌入；内容字节是 brotli 压缩的，搜原始字符串搜不到是正常的）
4. **换了图标后，「看起来没换」多半是 Windows 的图标缓存，不是产物没换**（2026-09-17 实测）：
   同一个路径的 exe 被覆盖时，资源管理器/任务栏会继续显示旧图标。
   判定要用**证据**，别用眼睛：
   - 文件图标 = exe 的 `RT_ICON` 资源（6 张，与 `src-tauri/icons/icon.ico` 逐字节比对）；
   - 窗口/任务栏图标 = `icons/icon.ico` 的**第 1 个条目**（tauri-codegen 取 `entries()[0]`）解码成 RGBA 后的字节；
   - 想「眼见为实」就把 exe 复制成**新文件名**（新路径不撞缓存），或清缓存：`ie4uinit.exe -show`、
     删 `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db` 后重启资源管理器。
   已固定的任务栏图钉把图标缓存在 `.lnk` 里，要**取消固定再固定**。

### 5.3.3 本次「吃一堑」汇总（2026-09-15）

| # | 坑 | 后果 | 教训 |
| --- | --- | --- | --- |
| 1 | M0-1 清理模板时把 `[features] custom-protocol` 当成模板残留删了 | **Windows 版一直是白屏**，而 WSL 侧因为走 `pnpm tauri dev` 完全正常，所以问题被掩盖了很久 | **「模板自带」不等于「模板残留」**。删配置性代码前先搞清它的用途；`tauri build` 才注入的东西，我们走裸 cargo 就必须自己写明 |
| 2 | 验证只看 `MainWindowHandle` / `MainWindowTitle` | 声称「Windows 运行验证通过」，而实际页面根本打不开 | **“窗口出现了”不等于“功能对了”**（§2.8）。验证清单必须包含「内容看得见」 |
| 3 | 用 `cmd.exe /c "start \"标题\" 路径"` 启动 exe | 弹「找不到文件 `\RayBend\`」，干扰用户 | WSL→Windows 的**引号嵌套不可靠**；直接用 `/mnt/c/...` 跑 |
| 4 | exe（02:50）比 `dist/`（10:28）旧 | 即使逻辑正确，跑的也是旧前端 | **产物时间戳必须晚于 `dist/`**，构建前先 `pnpm build` |

实测参考：首次 Windows 全量构建约 3–4 分钟；WSL 侧 `cargo check` 首次约 2–3 分钟。

### 5.4 规划纪律

- **一次规划只覆盖一个工作单元**（一个波次，或一个 milestone 里连续可交付的几波）。
- **`PLAN.md` 只做路线级描述**（每个 milestone 含哪些波次、完成定义是什么）。**具体实现方案在每个工作单元开工前单独规划**，写入 `plans/<milestone>-<单元>.md`（如 `plans/M0-1.md`），并走 plannotator 评审。
- **不提前细化未开工的工作单元** —— 项目规模决定了前置规划必然失真；开工时按当时情况重新规划。
- **已归档的旧规划**放 `PLAN.md` 附录（如附录 A），并明确标注「仅供参考、不是待办清单」。
- 规划文件中的勾选项会被 plannotator 进度跟踪，且 `mark_done` **会真实改写文件** —— 所以计划文件的范围要小、要准，否则勾选状态随变更失真、tracker 也会堆满未开工条目。

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
  app.db              # 全局：应用设置 / 库注册表（库 ID + 多路径 + 状态）/ 全局标签词典 / 任务队列
  backups\            # 迁移前快照（VACUUM INTO，保留 7 份）
  cache\<repository_id>\ # 默认缩略图与预览缓存（可在设置中改路径）
  logs\

<库根目录>\            # 用户指定，可多个
  catalog.db          # 库真相源：库 ID、资产、元数据、导入模版、序号计数、编辑栈、修订
  photos\             # 导入的落地目录（模版决定 photos/ 内的相对路径）
    2026-08-15\
      MYP0001.png
      _RAW\
        MYP0001.ORF   # 同名 RAW 放同级的 _RAW/（见 REPOSITORY.md §4.1）
  index.db            # 派生索引（缩略图索引、人脸、相似度）—— 可删可重建；真需要时才建
```

> **布局于 2026-09-15 按用户口述变更**：`catalog.db` **直接在库根**（不再藏于 `.raybend/`），
> 导入物落在库根的 `photos/`，`repo.json` 取消（库身份记在 `catalog.db` 内 + 中央 `app.db` 登记）。
> **完整业务规格见 `REPOSITORY.md`**（库身份 / 同路径多库 / 同库多路径 / 在线离线 / 导入模版 / 序号 / 重名 / RAW 分流 / 目录透传）。

- **真相源规则**：本地编辑/评分/关键词以 **DB 为准**，XMP 只是互操作通道；RAW 永不写回原文件（只写 `.xmp` sidecar）。
- **写并发**：SQLite 单写者 → 采用**单一写者 actor**（专属线程 + 专属连接，所有写操作串行化），读走连接池；批量事务；`PRAGMA journal_mode=WAL, synchronous=NORMAL, busy_timeout=5000, foreign_keys=ON`。
- **库身份与多路径**（`REPOSITORY.md` §2）：库身份 = `catalog.db` 内的唯一 ID；`app.db` 记录「库 ID → 多个路径」。
  路径挂载了哪个库靠**读该路径下 catalog.db 的 ID** 比对，而不是靠路径字符串 —— 这是「同路径不同库 / 同库多路径」的机制。
  读库前一律先走统一的状态解析（在线/离线），失败转离线而非报错。
- **跨库搜索**：`ATTACH` 多库 + `UNION ALL`；必要时在 `app.db` 维护轻量定位表做快速筛选。
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
实测数据（版本号、组件数、活跃度）见 `PLAN.md` 附录 A.10。

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
  实测数据见 `PLAN.md` 附录 A.10。选型原则仍是「只用套内图标做组合、降低数量」，不自绘几十个图标。

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
| `website/AGENTS.md` | **官网（`website/`）专属指南**：SolidStart 2.0 / Solid 2.0 选型、目录约定、命令、站点实现约定（i18n / 素材占位 / 下载信息注入）、**GitHub Pages 部署 + 自定义域名步骤** —— 技术栈与本体不同，别混用 |
| `website/ASSETS.md` | **官网素材清单**：要人出手的截图（尺寸/取景要点/放哪）与 AI 生图提示词；给完图在 `src/data/media.ts` 填 `src` 即自动替换占位 |
| `BROWSE.md` | **浏览模式规格**：三列结构、toolsbar 的筛选/标记/标签/锁、选择逻辑（Shift 区间翻转）、看图与对比、胶片带、信息栏、标签体系、两个通用浮层（模态 + 右上角 toast） |
| `REPOSITORY.md` | **库与导入规格**：库物理结构、库身份与多路径、在线/离线、导入模版与变量、序号、重名、RAW 分流、目录透传 |
| `DESIGN.md` | 视觉与配色体系（唯一事实来源） |
| `PLAN.md` | 近中期开发计划：Milestone（阶段）→ Wave（波次），含初始化清单与完成定义 |
| `plans/M0-1.md` 等 | 单个工作单元的详细计划与完成记录（一次一个，命名 `plans/<milestone>-<单元>.md`，见 §5.4） |
| `FUTURE.md` | 远期方向登记册：框架迁移、RAW 后端候选、渲染演进、编辑模块、AI、平台扩展 |
| `THIRD-PARTY-NOTICES.md` | 第三方组件与参考项目的许可登记 |
| `design/*.pen` + `design/*.md` | Pencil 设计稿与其说明（成对存在） |
| `implementations/*.md` | 每次改动的实施记录（文件名带日期，内容首行带精确时间） |

---

## 11. 术语约定（词汇表）

> 目的：人机协作时避免歧义。**代码、设计稿、文档、提交信息一律用本表的叫法。**
> 详细视觉规则见 `DESIGN.md`；本表只管「叫什么」。

### 11.1 界面区域

| 术语 | 含义 | 对应节点名 |
| --- | --- | --- |
| **`titlebar`** | 最上一条：应用图标/名、菜单、拖拽区、主题开关、密度开关、窗口三键 | `TitleBar` |
| **`flowbar`** | 第二条：工作流切换（导入/浏览/编辑/导出）+ 图片信息区 + 开关组 | `FlowBar` |
| **`toolsbar`** | 第三条：工具按钮，**内容居中**、随工作流变、无内容时整行隐藏 | `ToolsBar` |
| **`workspace`** | 三条下面的工作区，完全跟着工作流走 | `Workspace` |
| **工作流**（flow） | 导入 / 浏览 / 编辑 / 导出 四个阶段。**是有序流水线**，不是并列选项 | `FlowChip / …` |

### 11.2 两个最容易混淆的状态（**必须分清**）

| 术语 | 含义 | 表现 | 范围 |
| --- | --- | --- | --- |
| **勾选**（check） | 「**已加入待处理集合**」（如加入待导入目录） | 左侧**圆形主色勾选框** | **可多选**，互不影响 |
| **选中**（select） | 「**当前正在看/正在操作的对象**」 | 整行/整片**主色底** | **同一时间全局只有一个**（按时间模式下选中可以是一个时间片或一天） |

- 两者**互相独立**：一行可以「已勾选且未选中」。
- **勾选**要用户显式去点那个圈；**选中**往往是「点行本身」的结果。
- 跨面板（`最近` ↔ `来源`）的**选中状态是同一个状态**，必须同步。
- ❗ **不要因为某目录被选中而强制展开树** —— 展开只跟用户操作相关（`DESIGN.md` §12.4.1）。

### 11.3 移除 / 排除 / 删除（三者不同）

| 术语 | 含义 | 图标 | 弹窗 |
| --- | --- | --- | --- |
| **移除**（remove） | 从**当前集合**里拿掉（如从「最近」列表、已选目录中去掉） | **禁行图标**（填充圆 + 横杠） | 默认要，`Shift` 点击跳过 |
| **排除**（exclude） | 本次导入**不带这张照片**（但不动库、不动磁盘） | **同一个禁行图标** | 同上 |
| **删除**（delete） | **移到系统回收站**（`trash` crate）—— **不提供永久删除** | 走 `⋯` / 右键菜单，不给常驻按钮 | 要（**不做 `Shift` 快通道**：它真在动磁盘） |

> **统一用禁行图标，不用叉号** —— 因为它们都不是破坏性删除。
>
> **删除的定案（M2-W1 阶段 4 落地）**：第一阶段只做「移到系统回收站」，**不实现永久删除**。
> 顺序是**先文件后记录**（回收失败就不动数据库，绝不出现「库里有、磁盘没」）；
> 空目录的删除（目录树 `⋯` 菜单）同样只删**空**目录，底层只调 `remove_dir` ——
> 那条路径不可能删到用户的照片。两边都**不走 `easy destroy` 的 `Shift` 快通道**：
> 那条规则是给「从集合里移除」定的，而这两个动作真在磁盘上留痕。

### 11.4 其他常用术语

| 术语 | 含义 |
| --- | --- |
| **库**（repository / repo） | 用户选择的相片仓根目录。**没有库就无法导入** |
| **来源**（source） | 待导入的目录（磁盘 / NAS / 云） |
| **最近**（Recent） | 自动记录的最近 50 个导入目录（**不需要用户收藏**） |
| **已选目录** | 已勾选、准备导入的目录集合（**横条列表**形式） |
| **`shortpath`** | 路径缩写形式：盘符 + 中间各级首字母 + 末级全名。两层算法见 `DESIGN.md` §12.3 |
| **`easy copy`** | 指向某个信息组 → 出细边框 + `点击复制` 气泡；点击后弹 `已复制` |
| **`easy destroy`** | 移除类操作的快速通道：默认弹确认，**按住 `Shift` 跳过** |
| **中性面**（neutral surface） | 四级灰面：`surface-track` < `surface-main` < `surface-bar` < `surface-layer` |
| **主色底 / 辅色底** | 全局反馈规则：**指向=辅色底，点击后=主色底**（`DESIGN.md` §5） |
| **紧凑 / 宽松**（compact / loose） | 全局只有两档密度。**只影响间距类尺寸，不影响字号与图标大小** |
| **按时间分组 / 时间片** | 见 `DESIGN.md` §12.7。同一拍摄日内，相邻间隔 > 1 小时则断为新片 |
| **内嵌预览**（embedded preview） | 相机写在 RAW 里的 JPEG 预览。提取比完整解码快 1–2 个数量级 |
| **编辑栈**（develop stack） | 对一张照片的全部非破坏性编辑操作的序列。第一阶段不做 |
