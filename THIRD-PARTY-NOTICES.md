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
| 【待 M0-1 落定】wgpu / naga | 29.x 或 30.x | MIT / Apache-2.0 | GPU 渲染 | 兼容 |
| 【待 M0-1 落定】Solid / Vite / Tailwind CSS | 见 `package.json` | MIT | 前端框架与构建 | 兼容 |
| 【待 UI 设计阶段引入】Ark UI（`@ark-ui/solid`） | 5.x | MIT | UI 组件原语 | 兼容 |
| 【待 UI 设计阶段引入】Tabler Icons（`@tabler/icons-solidjs`） | 3.x | MIT | 图标 | 兼容 |
| 【待 M0-4 引入】rusqlite（bundled SQLite） | — | MIT + SQLite 公有领域 | 本地数据库 | 兼容 |

> **完整清单待补**：以上为当前可预见的直接依赖。M0-1 依赖落定、以及每个里程碑引入新依赖时，
> 需补齐**完整清单（含间接依赖）**；发布前（M6）应据锁文件生成一次机器可核对的完整清单。

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
