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
| `pnpm build` | 前端构建到 `dist/`（**跑任何 Windows 构建之前先跑它** —— `dist/` 是编译期嵌进 exe 的） |
| `pnpm tauri dev` | 起桌面窗口 |
| `pnpm tauri <参数>` | Tauri CLI 透传；AI 资源按 `--ai=auto\|required\|off` 解析 |
| `pnpm preview` | 预览构建后的前端 |

### Windows 构建与调试

产品版在 Windows 侧验证；跑之前必须先 `pnpm build`（见上）。

| 命令 | 作用 |
| --- | --- |
| `pnpm debug:win` | 一键 debug 构建：前端 → Windows cargo → 产物核对（可接 `-- --ai=…`） |
| `pnpm check:win` | 产物体检：与 `dist/` 的时间戳、内嵌前端资源、worker 协议 |
| `pnpm clean:win` / `pnpm clean:wsl` | 只清理失效 target 产物，不付冷构建的代价 |
| `pnpm check:color-win [profile…]` | Windows 侧色彩探针（在 Windows 目标上构建并跑色彩示例） |
| `pnpm check:lens-ipc-win --launch <库 id> <资产 id>` | 只读的镜头元数据 IPC 冒烟；`--expect-profile=` / `--expect-metadata-warning` 加断言 |

### 发布（由人类执行）

| 命令 | 作用 |
| --- | --- |
| `pnpm release <test\|patch\|minor\|major> [--channel beta\|test\|release] [--win-msi\|--win-nsis] [--unsigned] [--with-updater] [--dry-run] [--allow-dirty] [--skip-build] [--win-dir <目录>]` | 升版、构建、打包并产出 `release-out/` —— 不 commit、不 tag、不上传 |
| `pnpm release:finalize <bundle> --manifest <json> --out <目录> [--allow-unsigned] [--base-url <https://…>]` | 手动路径：对已有 bundle 生成哈希、清单与更新 JSON |
| `pnpm release:publish <release-out/vX.Y.Z> [--execute] [--no-crates]` | 默认只读预览；`--execute` 才提交、tag、push、建 Release、上传资产、把核心库同步发到 crates.io，并等官网工作流。`--no-crates` 跳过 crates.io；beta / test 版从不发它 |
| `pnpm licenses:generate` | 重新生成 `public/legal/third-party.json` |

### AI 资源（离线相片标签）

识别全在本机：模型包与 CPU 运行库都是**构建输入**，不在运行时下载。
构建命令（`pnpm tauri` / `pnpm debug:win` / `pnpm release`）都接 `--ai=auto|required|off` —— `auto`（默认）用已登记的包，没有就回退到无 AI 版（会打印原因）；`required` 要求每个输入都在且已校验，否则失败；`off` 不碰模型源也不联网。

| 命令 | 作用 |
| --- | --- |
| `pnpm ai:prepare [--ai=auto\|required\|off]` | 解析并冻结本次构建用的 AI 输入（模型包 + CPU 运行库 DLL），把计划打成 JSON |
| `pnpm ai:use [-- <模型库>]` | 登记**已经导出好**的包（新机器 `git clone` + `git lfs pull` 后就够） |
| `pnpm ai:library:init [-- <路径>]` | 在指定路径初始化模型库；不传路径就用本机默认位置 |
| `pnpm ai:export [-- <模型库>]` | 经模型库导出/校验包，并把源登记给本项目 |

模型本体在**独立的 `model-registry` 仓库**里，与 raybend 检出并列放。raybend 的 Git 只保留兼容契约与准入摘要；本机源指针（`ai-model-source.local.json`）被 Git 忽略。

```bash
# 一次性：把模型库放到 raybend 旁边
git clone https://github.com/C-Thun/model-registry ../model-registry
cd ../model-registry && git lfs install && git lfs pull   # ONNX 编码器走 LFS：指针不是权重

# 登记包并构建
cd ../raybend
pnpm ai:use -- ../model-registry     # 包已导出且已提交：只需登记
pnpm debug:win -- --ai=required      # 想要无 AI 版就 --ai=off
```

从 0 开始构建 ONNX 包（只在模型或配方变了时才需要；要求 Python 3.12、能联网、≥4 GiB 空闲空间与 Git LFS）：

```bash
cd ../model-registry
pnpm model:export                    # 固定上游 revision → FP32 / opset17 ONNX → 库内提交

cd ../raybend
pnpm ai:library:init -- ../model-registry   # 库还没有归属标记时才需要
pnpm ai:export -- ../model-registry         # 导出 + RayBend profile + 本机源登记
pnpm ai:prepare -- --ai=required            # 构建前校验并冻结输入
```

注意：只用 CPU，产品不依赖 CUDA / Python / PyTorch；`RAYBEND_AI_RUNTIME_DIR` 可指向本机 onnxruntime 目录，替代已核对缓存与固定官方 ZIP；重新导出导致摘要变化属**待审阅事件**，不会自动更新（协议 [specs/ai-model-library-build.md](specs/ai-model-library-build.md)，v1 阈值与质量 [docs/ai/tinyclip-v1/README.md](docs/ai/tinyclip-v1/README.md)）。

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

### 性能

| 命令 | 作用 |
| --- | --- |
| `pnpm perf:browse [行数]` | 网格虚拟滚动基准（默认 10 万行） |
| `pnpm perf:grid [行数]` | 网格管线吞吐：按时间分组、行模型、虚拟窗口 |
| `pnpm perf:win [--launch]` | Windows 真机浏览采样（CDP） |

### 周边与历史脚本

这里的东西都不在构建或发布链上。

| 命令 | 作用 |
| --- | --- |
| `pnpm shot` | 按主题 / 密度 / URL / 尺寸截图 —— 开发期视觉自查 |
| `pnpm crash:drill` / `pnpm migrate:drill` | M1 存储期的演练（worker 崩溃隔离；catalog 迁移 / 备份 / 故意损坏）—— 动恢复逻辑时重跑 |
| `pnpm spike:win` | M0-2 / M2-W1 的渲染 spike（构建 → 开窗 → 测量 → 落报告），留作测量工具 |
| `scripts/build-dav1d-win.cmd` | Windows 侧一次性构建 dav1d 静态库的辅助脚本（其环境变量封在 `scripts/lib/dav1d-win.mjs`） |
| `scripts/ai/*.py`、`scripts/ai/*.ps1` | 模型工作的研究 / 归档脚本（阈值复算、参考向量、预处理一致性、Windows worker 探针）—— 不是产品依赖 |

内部模块在 `scripts/lib/`（`release-*.mjs`、`ai-*.mjs`、`cdp.mjs` 等）：它们是上面这些入口的库，本身不是命令。

## 技术栈

Rust · Tauri 2 · wgpu（原生 GPU 渲染）· SolidJS · Tailwind CSS · SQLite · rawler（RAW 解码）

## 文档

- 使用说明（应用内按 F1 也有一份）：[docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- 发布、签名与更新通道：[docs/release.md](docs/release.md)
- 隐私：[docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- 工程决策与记忆（中文）：[AGENTS.md](AGENTS.md)、[memory/](memory/)

## 许可

[AGPL-3.0-only](LICENSE)，第三方组件清单见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。若把光伴作为网络服务对外提供，需按 AGPL 第 13 条开放对应源码。
