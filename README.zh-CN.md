<div align="center">

<img src="assets/logo-wide.webp" alt="RayBend · 光伴" width="440" />

**光伴 · 本地优先的开源相片管理软件**

导入 → 浏览 → 编辑 → 导出，一条完整的工作流。免费，无订阅，照片不出你的磁盘。

[English](README.md) ｜ 简体中文 ｜ [官网](https://raybend.cthun.com/) ｜ [下载](https://github.com/C-Thun/raybend/releases)

</div>

---

商业相片管理软件要么太贵，要么按年订阅；而照片库是几十年攒起来的东西，管理它的工具不该是一张年年续费的账单。光伴是那个免费、开源的替代品：一条 **导入 → 浏览 → 编辑 → 导出** 的完整工作流，照片、编辑与整理都留在你自己的磁盘上。无账号，无上传。

## 功能

- **一套工作流** —— 导入、浏览、编辑、导出，四个完整的工作区依序衔接，切换即达。
- **界面即功能** —— 每一项操作都以按钮落在界面上，不藏于菜单层级；命令与快捷键留给熟练之后。
- **多源导入** —— 单次导入可同时取自多个来源，多卡槽相机一次导完；命名与目录结构依模版生成，序号自动递增。
- **按时间聚合** —— 照片按拍摄日期与时段自然成组。
- **多图对比** —— 候选照片并置一屏，从容取舍。
- **无损编辑** —— 以调色为核心，一切调整不触碰原片；一张照片可存多个版本，风格可存可复用。
- **开放的文件格式** —— 评级、色标与编辑结果写进标准 XMP sidecar，别的软件认得。RAW 原片永不改写。
- **一切在本地** —— 无账号，无上传，离线可用全部功能。
- **外观与语言** —— 明暗两套基调，紧凑宽松两档密度，中英双语，随取随换。

## 支持的格式

| | 格式 |
| --- | --- |
| **照片** | JPG · PNG · TIFF · WebP · AVIF · HEIC \* |
| **导出** | JPG · PNG · WebP · AVIF |
| **相机 RAW** | `ari` `arw` `cr2` `cr3` `crw` `dcr` `dcs` `dng` `erf` `iiq` `kdc` `mef` `mos` `mrw` `nef` `nrw` `orf` `pef` `raf` `rw2` `sr2` `srf` `srw` `3fr` |

\* HEIC 会被导入并建索引，但显示与编辑还需要一个尚未内置的解码器 —— 缩略图目前显示占位图。

RAW 导入与解码跟随 [rawler](https://github.com/dnglab/dnglab)，机型覆盖随上游版本推进。同名的 JPG 与 RAW 视为一张照片的两个文件。

## 下载

Windows 10 / 11（64 位），免费。安装包见 [GitHub Releases](https://github.com/C-Thun/raybend/releases)。

## 从源码构建

需要 Node 与 pnpm，以及 `rust-toolchain.toml` 锁定的 Rust 工具链。日常开发在 WSL/Linux 即可；要产出 Windows 产品版，还需要 Windows 侧的 Rust/MSVC 与一次性构建的 dav1d 静态库 —— 完整说明见[发布指南](docs/release.md)。

```bash
pnpm install
pnpm tauri dev        # 桌面窗口
pnpm build            # 前端构建；dist/ 在编译期嵌进可执行文件
```

开发期的完整命令索引（测试、质量门、Windows 调试构建、AI 资源）见 [docs/development.md](docs/development.md)。

## 发布构建

安装包在本地构建（WSL 前端 + Windows 侧 Rust/MSVC），由维护者发布；完整手册（签名、构建目录、排障）见[发布指南](docs/release.md)。

**一次性准备**（换机器时才需要重做）：

```bash
# Windows 侧：与前端 CLI 同版本的 cargo-tauri（版本不匹配时 release 会在构建前报错）
cmd.exe /d /c "cargo install tauri-cli --version 2.11.4 --locked"
# WSL 侧：GitHub CLI 登录（发布时用；与 git 的 SSH 认证是两回事）
gh auth login
# 更新签名密钥：只生成一次，私钥与密码自己保管、不进仓库
pnpm tauri signer generate -w ~/.config/raybend/updater.key
```

只在**带更新的构建**（`--with-updater`）需要这两个环境变量（下面是 bash；fish 用 `set -Ux`）：

```bash
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.config/raybend/updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='生成密钥时设的密码'
```

**日常发版**（一条命令，或两条）：

```bash
pnpm release patch --win-msi --unsigned --publish   # 升版 + 打 MSI + 发布，一次到底
pnpm release 0.2.0 --win-msi --unsigned --publish   # 指定版本号（或 --version v0.2.0）
pnpm release patch --win-msi --unsigned             # 或者先准备…
pnpm release:publish release-out/v0.1.2 --execute   # …验收安装器后再发布
```

带更新清单加 `--with-updater`；跳过 crates.io 同步加 `--no-crates`；发布动作（提交/tag/推送/上传/等官网）只由 `--publish` 或第二条指令执行。

**AI 模型（编译期外挂）**：默认零配置 —— 构建时自动找**同级**的 `../model-registry`；找不到就出无 AI 版并在日志打印原因（`--ai=required` 改为直接失败，`--ai=off` 明确不要）。模型库在别处时，把它的**绝对路径**登记一次（只写在本机，Git 忽略）：

```bash
pnpm ai:use /绝对路径/model-registry      # 要求该目录是已初始化的 model-registry
```

CPU 运行库（onnxruntime DLL）自动从上游固定地址下载并校验 SHA-256；想用本机已有的 DLL 就设 `RAYBEND_AI_RUNTIME_DIR=<目录>`，想换下载缓存位置就设 `RAYBEND_AI_CACHE_DIR`。

## 技术栈

Rust · Tauri 2 · wgpu（原生 GPU 渲染）· SolidJS · Tailwind CSS · SQLite · rawler（RAW 解码）

## 文档

- 使用说明（应用内按 F1 也有一份）：[docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- 开发命令：[docs/development.md](docs/development.md)
- 发布、签名与更新通道：[docs/release.md](docs/release.md)
- 隐私：[docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- 工程决策与记忆：[AGENTS.md](AGENTS.md)、[memory/](memory/)

## 许可

[AGPL-3.0-only](LICENSE)，第三方组件清单见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。若把光伴作为网络服务对外提供，需按 AGPL 第 13 条开放对应源码。
