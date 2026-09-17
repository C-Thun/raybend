# 第三方组件许可声明（THIRD-PARTY-NOTICES）

> 本文件记录 raybend（中文名「光伴」）依赖或参考的第三方组件及其许可。
> **本项目自身采用 AGPL-3.0-only**（全文见 `LICENSE`）。
>
> 收录规则：
>
> - 「运行时/构建期依赖」= 会被编译或打包进产物的组件 ⇒ **必须登记**，并说明与 AGPL-3.0 的兼容性。
> - 「参考项目」= 只用来学习思路、或未来可能移植算法 ⇒ 也登记，避免日后遗忘许可约束。
> - 新增任何第三方依赖时，**同步登记到本文件**。

---

## 1. 运行时 / 构建期依赖

| 组件 | 版本 | 许可 | 用途 | 与 AGPL-3.0 的关系 |
| --- | --- | --- | --- | --- |
| 【待 M0-3 落定】rawler（上游 [dnglab](https://github.com/dnglab/dnglab)） | 0.8.0 | **LGPL-2.1-only** | RAW 解码 | **兼容**：LGPL v2.1 可经 GPLv2-or-later / GPLv3 路径与本项目组合（FSF 许可说明：LGPL v2.1「compatible with GPLv2 and GPLv3」；AGPL-3.0 §13 允许与 GPL-3.0 作品组成单一作品） |
| 【待 M0-1 落定】Tauri 及其官方插件 | 2.x | MIT / Apache-2.0 | 应用外壳 | 兼容（宽松许可） |
| wgpu / naga | **30.0.1**（2026-09-17 落定，见 `FUTURE.md` C6） | MIT / Apache-2.0 | GPU 渲染（方案 B：webview 挖洞 + 直绘） | 兼容 |
| pollster | 0.4 | MIT / Apache-2.0 | 在渲染线程里跑 wgpu 的 async 初始化 | 兼容 |
| 【待 M0-1 落定】Solid / Vite / Tailwind CSS | 见 `package.json` | MIT | 前端框架与构建 | 兼容 |
| 【待 UI 设计阶段引入】Ark UI（`@ark-ui/solid`） | 5.x | MIT | UI 组件原语 | 兼容 |
| 【待 UI 设计阶段引入】Tabler Icons（`@tabler/icons-solidjs`） | 3.x | MIT | 图标 | 兼容 |
| 【待 M0-4 引入】rusqlite（bundled SQLite） | — | MIT + SQLite 公有领域 | 本地数据库 | 兼容 |

> **完整清单待补**：以上为当前可预见的直接依赖。M0-1 依赖落定、以及每个里程碑引入新依赖时，
> 需补齐**完整清单（含间接依赖）**；发布前（M6）应据锁文件生成一次机器可核对的完整清单。

---

## 1b. 品牌素材（**自研，不属于第三方**）

| 素材 | 位置 | 许可 |
| --- | --- | --- |
| 应用图标 `logo.png` / `logo-small.png` | `src/assets/branding/`；打包图标由 `pnpm tauri icon` 生成到 `src-tauri/icons/` | **自研**（2026-09-17 用户确认「图片自己做的」），随项目 **AGPL-3.0-only** |
| 启动闪屏 `splash-cn.webp` / `splash-en.webp` | `public/splash/` | 同上 |

> 登记它们只为一件事：将来若有人问「这个 logo 从哪来的、能不能用」，答案在一处写着。
> **不是**第三方素材 ⇒ 不涉及外部授权。

---

## 1c. 官网（`website/`）的依赖

官网与应用本体是**两个独立包**（见根 `AGENTS.md` §4），所以它的第三方组件单独登记在这里。

| 组件 | 版本 | 许可 | 用途 | 与 AGPL-3.0 的关系 |
| --- | --- | --- | --- | --- |
| **Lucide**（只取图标数据，见 `website/scripts/generate-icons.mjs`） | — | **ISC** | 官网图标（生成到 `website/src/components/icons.tsx`） | 兼容（ISC 为宽松许可，保留版权声明即可） |
| Inter（`@fontsource-variable/inter`） | 5.3.0 | **SIL OFL-1.1** | 官网拉丁字体（自托管 woff2） | 兼容（OFL 允许嵌入与再分发，保留许可证即可） |
| Noto Sans SC（`@fontsource-variable/noto-sans-sc`） | 5.3.0 | **SIL OFL-1.1** | 官网中文字体（自托管，按 unicode-range 分块） | 同上 |
| SolidJS 2 / `@solidjs/router` / `@solidjs/meta` / `@solidjs/vite-plugin` | 2.0-rc / 3.0-next | MIT | 官网框架与构建 | 兼容 |
| Tailwind CSS 4（`tailwindcss` / `@tailwindcss/vite`） | 4.3.3 | MIT | 官网样式 | 兼容 |
| `filesystem-routing` | 0.2.1 | MIT | 官网文件式路由 | 兼容 |
| Vitest / jsdom / oxlint / TypeScript / Vite | 见 `website/package.json` | MIT / Apache-2.0 | 官网开发与测试（不进产物） | 兼容 |

> `lucide-solid`（ISC）只在**开发期**作为图标数据来源（与 `lucide-static` 同类用途）；
> 它的组件代码**不会**进官网产物 —— 原因见 `website/AGENTS.md`（Solid 2 不兼容）。

---

## 2. 参考项目（未直接包含其代码）

| 项目 | 许可 | 说明 |
| --- | --- | --- |
| [RapidRAW](https://github.com/CyberTimon/RapidRAW) | **AGPL-3.0** | 参考实现（Tauri + wgpu 直绘 + rawler fork）。**许可与本项目一致**，若复用其代码需保留版权与许可声明并注明来源 |
| [darktable](https://www.darktable.org/) | GPL-3.0-or-later | 色彩管线、去马赛克（RCD / Markesteijn）、AgX 等算法的移植参考。依 AGPL-3.0 §13 可组合，移植须保留署名与来源 |
| [RawTherapee](https://rawtherapee.com/) | GPL-3.0 | AMaZE / DCB 去马赛克等算法参考，同上 |
| [lensfun](https://lensfun.github.io/) | LGPL-3.0 | 镜头校正数据库（未来接入） |
| [zenraw](https://github.com/imazen/zenraw) | AGPL-3.0-only 或商业授权 | RAW 解码备选后端；**AGPL-3.0-only 与本项目许可一致**，无许可障碍 |

**明确不可用**：

| 组件 | 许可 | 原因 |
| --- | --- | --- |
| Adobe DNG SDK | 专有 | 与 AGPL-3.0 不兼容 |

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-15 | 初版：建立清单骨架，登记 rawler(LGPL-2.1) 与参考项目；详细依赖清单待 M0 各波次落定后补齐 |
| 2026-09-17 | wgpu 版本落定为 30.0.1（上游最新）+ 新增 pollster 0.4（渲染 spike 引入，M2-W1） |
| 2026-09-17 | 新增 §1b「品牌素材」：logo 与闪屏图**自研**，随项目 AGPL-3.0-only（人类确认） |
| 2026-09-17 | 新增 §1c「官网依赖」：Lucide 图标数据(ISC)、Inter / Noto Sans SC(OFL-1.1)、官网的 Solid 2 线与构建/测试工具 |
