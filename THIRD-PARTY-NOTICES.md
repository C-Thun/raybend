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
| ravif（含 rav1e、avif-serialize） | 0.13.0 / 0.8.1 / 0.8.9 | **BSD-3-Clause**（rav1e 为 BSD-2-Clause） | **AVIF 编码**（全系统缓存图的格式，M3-W3 引入） | 兼容（宽松许可，BSD 系与 AGPL-3.0 无冲突） |
| **dav1d**（经 `image` 的 `avif-native` → `dav1d` / `dav1d-sys` / `av-data`，后三个是 MIT 的 Rust 绑定） | **1.5.0**（Windows 静态库）/ 1.4.1（WSL 系统库） | **BSD-2-Clause**（C 库）；绑定 crate MIT | **AVIF 解码**（Rust 侧要像素的地方：编辑器过渡帧、avif 导入；M3-W3 引入） | 兼容（宽松许可）。**不是 Rust 依赖而是系统/静态 C 库**：Windows 侧由 `scripts/build-dav1d-win.cmd` 从 `https://code.videolan.org/videolan/dav1d` 的 `1.5.0` tag 源码构建成静态库（`C:\rb-deps\dav1d-1.5.0\`，不打 DLL 进安装包）；WSL 侧用发行版包 `libdav1d-dev` |
| notify / notify-types | 8.2.0 / 2.1.0 | **CC0-1.0** / **MIT OR Apache-2.0** | M4-W1 原生有界文件监听；Linux inotify（MIT）、Windows 系统接口；不递归全库 | 兼容（公有领域贡献许可） |
| sha2 | 0.10.9 | **MIT OR Apache-2.0** | LUT 原文件字节 SHA-256，导入判重；已有间接依赖改为直接使用 | 兼容（宽松许可） |
| webp / libwebp-sys / libwebp | 0.3.1 / 0.9.6 / 1.3.1（绑定随附源码） | **MIT OR Apache-2.0** / **MIT** / **BSD-3-Clause** | LUT 封面有损 WebP 质量 80；随 cargo 编译静态 C 编码器，无额外运行时 DLL；解码仍走 image-webp | 兼容（宽松许可）。2026-09-26 替换封面严重丢色的 webp-rust / bin-rs |
| mp4parse（`avif-native` 带进来） | 0.17 | MPL-2.0 | AVIF/HEIF 容器解析（不碰 AV1 位流） | 兼容（MPL-2.0 为文件级 copyleft，可链接） |
| rav1e / rayon | 1.12 | MIT / Apache-2.0 | ravif 的多线程编码（`image` 的 `rayon` feature 打开；不开的话 AVIF 编码慢 8 倍以上） | 兼容 |
| **lensfun**（纯 Rust 移植 `vdavid/lensfun-rs`，crates.io 包名就是 `lensfun`） | **0.7.0** | **代码 LGPL-3.0-or-later**；**内置的 XML 校准库 CC BY-SA 3.0** | **镜头校正**（畸变 / 横向色差 / 暗角；M3-W4 引入）。用法：`Database::load_bundled()` —— XML 库 gzip 后**嵌在二进制里**（约 5 MB 解压后，1543 支镜头），不分发资源文件 | **兼容**：LGPL-3.0 可经 GPL-3.0 路径与本项目 AGPL-3.0 组合（与 rawler 同一套论证）；数据部分原样分发 + 署名（见下方「数据来源」）。⚠️ 上游 API 仍是 0.x（beta），**只允许在 `crates/raybend/src/lens/` 内使用** |

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
| **potrace**（`node-potrace`） | 2.1.8 | **GPL-2.0** | **开发期**把品牌书法字描摹成矢量路径（`scripts/generate-marks.mjs`） | **不进产物**：只产出一堆坐标数字（`src/components/mark-paths.ts`）；用工具跑一遍不构成对工具的衍生，与用 ImageMagick / ffmpeg 同类。若日后想避开 GPL 工具，可换零依赖的 `imagetracerjs`（描摹质量略逊） |

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
| 2026-09-17 | §1c 补 potrace(GPL-2.0，**开发期描摹工具**，只产坐标不进产物) |
| 2026-09-24 | 新增 **dav1d 1.5.0（BSD-2-Clause）**：AVIF 解码（`image` 的 `avif-native`）；Windows 走一次性构建的静态库、WSL 走系统包。连带登记 `mp4parse`(MPL-2.0) |
| 2026-09-26 | 新增 **webp-rust 0.3.1** 与 bin-rs 0.0.10（均 MIT）：LUT 封面质量 80 有损 WebP 编码 |
| 2026-09-25 | 新增 **lensfun 0.7.0**（纯 Rust 移植，LGPL-3.0-or-later；内置 XML 库 CC BY-SA 3.0）：镜头校正。**数据来源**：LensFun 项目（https://lensfun.github.io/ ，作者 Andrew Zabolotny 与 LensFun 贡献者）的校准数据库，经 `vdavid/lensfun-rs` 原样打包分发；Rust 移植由 David Veszelovszki 完成。M3-W4 引入 |


## BM3D 移植（2026-09-25）

`crates/raybend/src/develop/bm3d.rs` 的 DCT、归一化 Hadamard、硬阈值 / Wiener 协同滤波、块分组与加权聚合来自 **RapidRAW** 的 `src-tauri/src/denoising.rs`（CyberTimon / RapidRAW contributors，GNU AGPL v3）。参考提交：`f00145c11fd57043476574a384be7409a0a18e76`。

- 源文件：https://github.com/CyberTimon/RapidRAW/blob/f00145c11fd57043476574a384be7409a0a18e76/src-tauri/src/denoising.rs
- 上游许可：https://github.com/CyberTimon/RapidRAW/blob/f00145c11fd57043476574a384be7409a0a18e76/LICENSE
- 修改：剥离 Tauri / 烘焙落盘逻辑，改为线性 u16 编辑阶段；独立亮度/色度强度、有界并行 tile、取消；修正参考块覆盖、窗口零边缘、SSD 单位与坐标截断；确定性候选选择、两像素粗搜加最优候选邻域细搜。
- BM3D 算法出处：K. Dabov, A. Foi, V. Katkovnik, K. Egiazarian, “Image Denoising by Sparse 3-D Transform-Domain Collaborative Filtering,” IEEE TIP 2007。https://webpages.tuni.fi/foi/GCF-BM3D/
- 未引入该学术站点受单独许可约束的 MATLAB/Python 二进制或源代码；本次实际移植源是上述 AGPL 的 RapidRAW 实现。项目许可证全文见根目录 `LICENSE`。


## LUT 封面编码修复（2026-09-26）

封面编码改用 `webp`（Jared Forth / contributors，MIT OR Apache-2.0）与 `libwebp-sys`
（XianYou / Kornel Lesiński / contributors，MIT）封装的 libwebp（Google / contributors，BSD-3-Clause）。
仅用于封面编码；库随源码静态构建，Windows 复用现有 MSVC 工具链。

- webp：https://github.com/jaredforth/webp
- libwebp-sys：https://github.com/NoXF/libwebp-sys
- libwebp：https://github.com/webmproject/libwebp

### libwebp 版权与许可

Copyright (c) 2010, Google Inc. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of Google nor the names of its contributors may
    be used to endorse or promote products derived from this software
    without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
