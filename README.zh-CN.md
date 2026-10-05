<div align="center">

<img src="assets/logo-wide.webp" alt="RayBend · 光伴" width="440" />

**光伴 · 本地优先的开源相片管理软件**

导入 → 浏览 → 编辑 → 导出，一条完整的工作流。免费，无订阅，照片不出你的磁盘。

[English](README.md) ｜ 简体中文 ｜ [官网](https://raybend.cthun.com/) ｜ [下载](https://github.com/C-Thun/raybend/releases)

</div>

---

## 为什么做光伴

市面上的相片管理软件不出三种情形：要么太贵，要么按年订阅，价格合理的又不够稳定。照片库是几十年攒起来的东西，管理它的工具不该是一张年年续费的账单。

光伴给出的是另一个答案：一条完整的工作流 —— 导入 → 浏览 → 编辑 → 导出 —— 照片与你的全部整理都留在你自己的磁盘上。无账号，无上传。永远免费，开源（AGPL-3.0）。

## 功能

- **一套工作流** —— 导入、浏览、编辑、导出，四个完整的工作区依序衔接，切换即达。
- **界面即功能** —— 每一项操作都以按钮落在界面上，不藏于菜单层级；命令与快捷键属于进阶配置，留给熟练之后。
- **多源导入** —— 单次导入可同时取自多个来源，多卡槽相机一次导完；命名与目录结构依模版生成，序号自动递增。
- **按时间聚合** —— 照片按拍摄日期与时段自然成组；先见一天脉络，再看一张照片。
- **多图对比** —— 候选照片并置一屏，从容取舍。
- **无损编辑** —— 以调色为核心，一切调整不触碰原片；一张照片可存多个版本，风格可存可复用。
- **开放的文件格式** —— 评级、色标与编辑结果写进标准 XMP sidecar，别的软件认得。RAW 原片永不改写。
- **一切在本地** —— 无账号，无上传，离线可用全部功能；照片与整理结果都在你自己的磁盘上。
- **外观与语言** —— 明暗两套基调，紧凑宽松两档密度，中英双语，随取随换。

## 支持的格式

### 照片（位图）

| 方向 | 格式 |
| --- | --- |
| **导入** | JPG · PNG · TIFF · WebP · AVIF · HEIC \* |
| **导出** | JPG · PNG · WebP · AVIF |

\* HEIC 会被识别、导入并建索引；解码还需要 libheif 系解码器，目前 HEIC 小图显示占位图。

### 相机 RAW

RAW 导入能力跟随 [rawler](https://github.com/dnglab/dnglab)。扫描器当前认得的扩展名：

`ari` `arw` `cr2` `cr3` `crw` `dcr` `dcs` `dng` `erf` `iiq` `kdc` `mef` `mos` `mrw` `nef` `nrw` `orf` `pef` `raf` `rw2` `sr2` `srf` `srw` `3fr`

两条刻意独立的路径：

- **小图（网格、胶片带）**取自相机写在 RAW 文件里的内嵌 JPEG，不碰传感器数据 —— 标准 TIFF 的 `JPEGInterchangeFormat`、松下 `JpgFromRaw`、DNG/3FR 的 JPEG 压缩条带、奥林巴斯 / OM SYSTEM 的 MakerNote 预览。这是毫秒级路径，而且**解码器机型库还不认识的新机身也能出图**。
- **完整解码（看图、编辑）**走 rawler，在隔离的 worker 进程里执行 —— 一张坏文件不会带崩应用。Bayer 传感器覆盖最广，富士 X-Trans 跟随 rawler。

已知缺口登记在 [`memory/FUTURE.md`](memory/FUTURE.md) §B：适马 Foveon X3 按口径不做；尼康 HE/HE★ 压缩、索尼新的 `arw6` 压缩与最新哈苏机身等上游支持 —— 这类文件**仍能出真小图**，但暂时打不开编辑器。奥林巴斯高分辨率 `.ORI` 与哈苏 `.FFF` 尚未接入扫描器。

同名的 JPG 与 RAW 视为一张照片的两个文件。

## 下载

Windows 10 / 11（64 位），免费。安装包见 [GitHub Releases](https://github.com/C-Thun/raybend/releases)。

## 从源码构建

需要 Node 与 pnpm，以及 `rust-toolchain.toml` 锁定的 Rust 工具链。日常开发在 WSL/Linux 即可；要产出 Windows 产品版，还需要 Windows 侧的 Rust/MSVC 与一次性构建的 dav1d 静态库 —— 完整说明见 [发布指南](docs/release.md)。

```bash
pnpm install
pnpm tauri dev        # 桌面窗口
pnpm build            # 只构建前端；dist/ 在编译期嵌进可执行文件
```

## 命令行

整套系统的操作都收在 pnpm 脚本里（Rust 侧用 cargo）。完整的发布流程见 [发布指南](docs/release.md)，这里只是索引。

### 日常开发

| 命令 | 作用 |
| --- | --- |
| `pnpm dev` | 只起 Vite 前端 —— 下面那些浏览器检查要连的页面 |
| `pnpm tauri dev` | 起桌面窗口 |
| `pnpm build` | 前端构建到 `dist/`（**跑任何 Windows 构建之前先跑它** —— `dist/` 是编译期嵌进 exe 的） |
| `pnpm preview` | 预览构建后的前端 |
| `pnpm tauri <参数>` | Tauri CLI 透传；AI 资源按 `--ai=auto\|required\|off` 解析 |

### 质量门

| 命令 | 作用 |
| --- | --- |
| `pnpm typecheck` | `tsc --noEmit` |
| `pnpm test` | 前端单元测试 |
| `pnpm test:release` | 发布脚本自己的单元测试 |
| `pnpm lint:colors` | 颜色令牌层之外不许出现硬编码颜色 |
| `pnpm lint:arch` | 前端分层与依赖方向 |
| `pnpm lint:i18n` | 翻译覆盖 |
| `cargo test --workspace` | Rust 测试（图像、RAW、存储、迁移） |

### 冒烟与回归

浏览器类检查需要另开一个终端跑 `pnpm dev`。

| 命令 | 作用 |
| --- | --- |
| `pnpm smoke:ui [url]` | 页面真的渲染出东西 —— 防白屏的控制台报错闸门 |
| `pnpm check:startup` | 启动 / 系统偏好 |
| `pnpm check:flowbar` | 工作流切换 |
| `pnpm check:browse` | 库非空时进浏览 |
| `pnpm check:import` | 导入确认流程（走到 `import_start`） |
| `pnpm check:export` | 导出工作区启动 |
| `pnpm check:external` | 外部编辑器往返 |
| `pnpm check:color-status [url]` | 色彩管线状态探针（显示配置与变换状态） |
| `pnpm shot` | 按主题 / 密度 / URL / 尺寸截图 |
| `pnpm crash:drill` | worker 崩溃隔离演练 |
| `pnpm migrate:drill` | 数据库迁移演练 |

### 性能

| 命令 | 作用 |
| --- | --- |
| `pnpm perf:browse [行数]` | 网格虚拟滚动基准（默认 10 万行） |
| `pnpm perf:grid [行数]` | 网格管线吞吐：按时间分组、行模型、虚拟窗口 |
| `pnpm perf:win [--launch]` | Windows 真机浏览采样（CDP） |

### Windows 构建与诊断

| 命令 | 作用 |
| --- | --- |
| `pnpm debug:win` | 一键 debug 构建：前端 → Windows cargo → 产物核对 |
| `pnpm check:win` | 产物体检：与 `dist/` 的时间戳、内嵌前端资源、worker 协议 |
| `pnpm spike:win` | 渲染 spike：构建 → 开窗 → 测量 → 落报告 |
| `pnpm clean:win` / `pnpm clean:wsl` | 只清理失效 target 产物，不付冷构建的代价 |
| `pnpm check:color-win [profile…]` | Windows 侧色彩探针（在 Windows 目标上构建并跑色彩示例） |
| `pnpm check:lens-ipc-win --launch <库 id> <资产 id>` | 只读的镜头元数据 IPC 冒烟；`--expect-profile=` / `--expect-metadata-warning` 加断言 |

### 发布（由人类执行）

| 命令 | 作用 |
| --- | --- |
| `pnpm release <test\|patch\|minor\|major> [--channel beta\|test\|release] [--win-msi\|--win-nsis] [--unsigned] [--with-updater] [--dry-run] [--allow-dirty] [--skip-build] [--win-dir <目录>]` | 升版、构建、打包并产出 `release-out/` —— 不 commit、不 tag、不上传 |
| `pnpm release:finalize <bundle> --manifest <json> --out <目录> [--allow-unsigned] [--base-url <https://…>]` | 手动路径：对已有 bundle 生成哈希、清单与更新 JSON |
| `pnpm release:publish <release-out/vX.Y.Z> [--execute]` | 默认只读预览；`--execute` 才提交、tag、push、建 Release、上传资产并等官网工作流 |
| `pnpm licenses:generate` | 重新生成 `public/legal/third-party.json` |

### AI 资源

| 命令 | 作用 |
| --- | --- |
| `pnpm ai:prepare [--ai=auto\|required\|off]` | 校验或落地本次构建要用的本地 AI 运行库与模型资源 |
| `pnpm ai:library:init` / `ai:export` / `ai:use` | 管理本地模型库，以及构建指向的模型来源 |

## 技术栈

Rust · Tauri 2 · wgpu（原生 GPU 渲染）· SolidJS · Tailwind CSS · SQLite · rawler（RAW 解码）

## 文档

- 使用说明（应用内按 F1 也有一份）：[docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- 发布、签名与更新通道：[docs/release.md](docs/release.md)
- 隐私：[docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- 工程决策与记忆（中文）：[AGENTS.md](AGENTS.md)、[memory/](memory/)

## 许可

[AGPL-3.0-only](LICENSE)，第三方组件清单见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。若把光伴作为网络服务对外提供，需按 AGPL 第 13 条开放对应源码。
