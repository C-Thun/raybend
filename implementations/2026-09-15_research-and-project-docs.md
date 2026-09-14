# 立项调研与三份基线文档建立

完成时间：2026-09-15 00:20:02 CST

## 本次改动范围

本仓库此前只有 `LICENSE`（GPL-3.0）与 `README.md` 及一次 initial commit。本次不写任何代码，只完成「立项调研 → 决策收敛 → 基线文档落盘」。

新增文件：

- `AGENTS.md` —— 项目核心信息、硬纪律、版本基线、目录结构、架构决定、调研关键真相、必须处理的问题清单、文档索引
- `PLAN.md` —— 近中期计划，Milestone（阶段）→ Wave（波次），含 M0 初始化清单与各里程碑 DoD、决策记录表
- `FUTURE.md` —— 远期方向登记册（A 迁移 / B RAW 后端 / C 渲染色彩 / D 编辑 / E AI / F 平台 / G 其它）
- `implementations/2026-09-15_research-and-project-docs.md` —— 本文件

未创建（由人类在 M0-W0 初始化时建立）：`design/`、`src/`、`src-tauri/`、`crates/`、脚手架与配置。

## 关键决策与理由

| 决策 | 结论 | 依据 |
| --- | --- | --- |
| 许可 | 维持 GPL-3.0，不闭源盈利 | 用户决定。附带收益：rawler（LGPL-2.1-only）可直接静态链接（FSF 明示 LGPL v2.1 与 GPLv2/GPLv3 兼容），且可合法移植 darktable / RawTherapee（均 GPL-3）的算法 |
| 渲染架构 | 原生 wgpu + 透明挖洞（方案 B） | 用户决定；RapidRAW 实证该路径（滑块拖动 20fps → 120fps，原瓶颈是 JPEG 编码 + IPC + 浏览器解码） |
| 存储 | 全局库 `app.db` + 每仓 `catalog.db`（分层） | 用户接受。换来仓可随盘移动、损坏隔离、按仓备份 |
| 缓存 | `%LOCALAPPDATA%` 默认 + 可按仓改为随仓 | 用户决定 |
| UI 框架 | Solid 1.9.x，**不引入 SolidStart** | Solid 2.0 仍是 RC（rc.8）；SolidStart v2 稳定版面向 Solid v1，与 Solid 2 不配套；桌面应用无 SSR 需求 |
| 色彩管理 | 预留，第一阶段不做 | 用户决定 |
| Tauri 3.0 | 分层 + 不引用 `tauri::Wry` / 不依赖 feature flag | Tauri 3.0.0-alpha.0 已把运行时而 crate 化 |

## 调研得到的关键事实（已写入 `AGENTS.md` §7）

1. **rawler 0.8.0**：LGPL-2.1-only；无任何 GPU/wgpu 依赖（纯 CPU，rayon）；提供 CFA 像素 + black/white level + WB 系数 + 色彩矩阵 + crop/active area + orientation + 内嵌预览；**不提供**去马赛克/降噪/镜头校正/色彩管理。dnglab 官方声明「Windows 未官方支持」，并明说**不保证处理不可信文件的安全性**（因此需 worker 进程隔离）。
2. **RapidRAW = AGPL-3.0**（不是 GPL-3）。其技术栈（实测其 `Cargo.toml` / `package.json`）：`rawler`（自建 fork）、`wgpu 29.0`（因 Apple P3 色偏而降级锁定）、`ort` + `tokenizers`（AI）、`image_hasher`、`mozjpeg-rs`/`webp`/`jxl-*`、React 19 + `lucide-react` + `react-window`（网格是**虚拟列表而非 canvas**）+ `konva`（覆盖层）+ `zustand` + `i18next`，且**没有任何 SQLite 依赖**（sidecar 流派）。
   → 可参考思路，不可直接复制代码进 GPL-3 项目。
3. **图标体系实测**：Lucide 无官方 fill（官方文档明说），但对相片管理领域覆盖远好于预期（实测存在 `pipette`/`contrast`/`crop`/`frame`/`proportions`/`layers`/`gallery-thumbnails`/`git-compare`/`badge-check`/`star-half`/`grip-vertical`/`swatch-book`/`lasso` 等，实测缺失 `history`、`flip-horizontal`）；Tabler 6,184 枚且**描边/填充成对**；Phosphor 1,248 × 6 字重含 fill/duotone；Material Symbols 含 FILL 可变轴且实测有 `raw_on`/`tonality`/`vignette`/`dehaze`/`healing`/`gradient`/`burst_mode` 等影像专用图标。
4. **canvas tile 取舍**：快速滚动的瓶颈在解码而非 DOM；DOM 虚拟化（RapidRAW 的 `react-window` 用法）在 10 万级已足够。canvas 登记为备选，仅在实测帧率不达标时启用。理由已写入 `FUTURE.md` §C3。
5. **Tauri 分发不需要 npm**：产物是安装包，npm 只服务 JS 库消费者，无需占位包名。

## 验证方式

- 决策与版本号来源：`crates.io` / `registry.npmjs.org` dist-tags 实测（Tauri CLI 2.11.4、solid-js 1.9.15 与 2.0.0-rc.8、@solidjs/start 2.0.5、tailwindcss 4.3.3、lucide-solid 1.46.0）
- 许可结论来源：FSF「Various Licenses and Comments about Them」原文 "GNU Lesser General Public License (LGPL) version 2.1 … It is compatible with GPLv2 and GPLv3."
- RapidRAW 技术栈来源：其仓库 `src-tauri/Cargo.toml`、`package.json`、`LICENSE`
- 图标覆盖来源：Lucide `sitemap.xml` 逐名检索；Google Fonts icon metadata 检索
- 文档校验：pi-lens markdownlint 通过（clean）

## 遗留问题

1. 组件原语（Kobalte / Ark UI）与图标体系（Tabler / Lucide / Material Symbols）的最终选型，留到 M0-W5 画出第一批界面后定夺。
2. 三个里程碑范围内的细节方案（表结构、渲染 spike 的具体验证项、导入 UI 的交互）尚未细化，按约定在开工前逐里程碑单独讨论。
3. 项目脚手架初始化由人类执行（`PLAN.md` §M0-W0 已给出方向与验收标准）。
