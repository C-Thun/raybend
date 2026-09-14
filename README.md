# raybend · 光伴

一个**相片管理软件**。目标是把「导入 → 浏览 → 评级 → 筛选 → 整理 → 导出」这条工作流做到足够顺手，成为商业相片管理软件的开源替代。

| | |
| --- | --- |
| 英文名 / 仓库名 | `raybend` |
| 中文名 | 光伴 |
| 产品名（安装包与窗口标题） | `RayBend` |

## 状态

**早期开发中**，当前处于 **M0：可行性验证与项目骨架** 阶段，尚无可供日常使用的功能。

- 开发计划与里程碑：[`PLAN.md`](PLAN.md)
- 远期方向登记：[`FUTURE.md`](FUTURE.md)
- 核心信息、纪律与架构决定：[`AGENTS.md`](AGENTS.md)

## 技术选型

| 层 | 选型 |
| --- | --- |
| 应用外壳 | Tauri 2（Windows 上为 WebView2） |
| 前端 | TypeScript + SolidJS + Tailwind CSS |
| 核心逻辑 | Rust（`crates/raybend`，业务层不依赖 Tauri） |
| 图像渲染 | 原生 wgpu 直绘 + 透明 WebView 挖洞 |
| 存储 | SQLite（全局库 `app.db` + 每个相片仓一份 `catalog.db`） |
| RAW 解码 | rawler（做成可插拔后端） |
| UI 组件 / 图标 | Ark UI / Tabler Icons |

## 平台

Windows 优先（Win10/11 近两年版本）；macOS 与 Linux 属远期规划。

## 工具链

统一使用 **pnpm**（前端）与 **cargo**（Rust），不引入 npm / yarn / bun 等第二套工具链。

```bash
pnpm install              # 安装前端依赖
pnpm tauri dev            # 开发运行（Linux/WSL 下为 webkit2gtk）
cargo check --workspace   # Rust 侧检查
```

> Windows 侧的构建与运行方式见 `AGENTS.md` 的「如何运行」一节。

## 许可

**AGPL-3.0-only** —— 全文见 [`LICENSE`](LICENSE)。

选择它的实用理由：本项目主体是本地桌面应用，AGPL 的额外网络条款只在「对外提供网络服务」时才生效；同时它使本项目可以合法复用同许可与 GPL 系项目（rawler、darktable、RawTherapee 等）的实现。

若将来把 raybend 作为网络服务对外提供，必须按 AGPL 第 13 条向使用者提供对应源码。

第三方组件与参考项目的许可清单见 [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)。
