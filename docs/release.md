# RayBend 构建与发布：环境 → 指令 → 应急

> **怎么读**（崔总 2026-10-15 定的组织方式）：从上往下读完就能动手，不留「做到 A 才发现 B 在后面」。
>
> - **§1–§3 是环境**，三层台阶：① 本地开发调试 → ② 本地 debug 包 → ③ 正式发布。
>   **每层只在前一层之上加东西**，不重复；每层末尾有**验收**，验过再往上走。
> - **§4–§6 是指令**，同样三层。**§7 是变化与应急**（按层），**§8 是踩过的坑**（症状 → 根因 → 做法）。
> - 文末**附录**放细节与依据（更新密钥语义、证书对照、crates.io、AI 外挂、手工收口、磁盘…）。
>   正文不搬细节，只在你需要时指过去。
> - 发布动作（提交 / tag / push / 上传 / 公开）**一律由崔总执行**（AGENTS.md §2.1）；Agent 只准备与验证。

## 0. 三层台阶一览

| 层 | 目标 | 在这一层**新增**什么 | 验收（过了才算这一层好了） |
| --- | --- | --- | --- |
| ① 本地开发与调试 | 改代码、跑测试、开窗口看界面 | WSL2 + Node/pnpm + Rust + Linux 图形库 + Git | `pnpm tauri dev` 开得出窗口 |
| ② 本地 debug 包 | 出一个能装能跑的 Windows 安装包 | Windows：MSVC + cargo-tauri + dav1d 静态库 + **本地**构建盘 | `pnpm debug:win` 出包、`pnpm check:win` 通过 |
| ③ 正式发布 | 出一个公开发布的版本 | gh 登录 + 更新签名密钥 + 仓库 Pages/environment 配置 + crates.io | §3.2 的三条自检全过 |

一句话记法：**① 只要能跑；② 加能打包；③ 加能公开。**

每次发版真正要敲的只有 §6 的一条（或两条）命令；前面三节是一次性准备。

---

## 1. 第一层：本地开发与调试

### 1.1 需要什么

| 项 | 要求 | 本机现状（2026-10-15 核查） |
| --- | --- | --- |
| 系统 | WSL2 里的 Linux（Debian/Ubuntu 系），带 **WSLg**（窗口显示到 Windows 桌面） | ✅ |
| Node | `engines.node >= 22.12.0`（package.json 声明） | ✅ |
| pnpm | `>= 10`；`corepack enable` 即可 | ✅ |
| Rust | `rustup` + 仓库根 `rust-toolchain.toml` 自动锁 **1.98.1**（含 rustfmt / clippy）；不改全局默认工具链 | ✅ |
| Linux 图形/系统库 | webkit2gtk 4.1、GTK3、libsoup3、librsvg2、appindicator、libssl、build-essential、pkg-config | ✅ |
| AVIF 解码（WSL 侧） | `libdav1d-dev` 系统库走 pkg-config；**WSL 侧不需要**静态库，也不需要任何 `SYSTEM_DEPS_DAV1D_*` 变量 | ✅ |
| Git | 能 clone/pull 本仓（SSH key 配好；与 GitHub API 登录是两件事） | ✅ |

```bash
# 系统库（Debian/Ubuntu 系）
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev \
  libayatana-appindicator3-dev libdav1d-dev

corepack enable          # 提供 pnpm
pnpm install             # 拉齐前端工具链与 @tauri-apps/cli
```

### 1.2 验收

```bash
cargo check --workspace         # Rust 侧能编（首次会久一点）
pnpm typecheck && pnpm test     # 前端类型 + 单测
pnpm tauri dev                  # 能开出窗口（经 WSLg；Linux 用 tauri.linux.conf.json 的系统标题栏）
```

顺手的门（改完就跑）：`pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`、`pnpm build`。
运行时冒烟要另开一个终端跑 `pnpm dev`，再执行 `pnpm smoke:ui`。

起不来时的典型症状与做法见 §7.1。

### 1.3 这一层**不需要**什么（别提前装）

Windows、cargo-tauri、dav1d 静态库、`gh`、更新签名密钥、crates.io token —— 全是第二、三层的事。

---

## 2. 第二层：本地 debug 包（在第一层之上）

### 2.1 需要什么（新增）

| 项 | 要求 | 为什么 |
| --- | --- | --- |
| Windows Rust | **MSVC 工具链**（Windows 侧 `rustup` 默认即 MSVC），Windows 能跑 `cargo` | 交叉编译由 Windows 原生 cargo 执行 |
| VS Build Tools | 「使用 C++ 的桌面开发」工作负载 + Windows SDK | 编译链接 + 打 MSI（WiX） |
| cargo-tauri | **必须与 WSL 侧 `@tauri-apps/cli` 同版本**（当前 2.11.4） | 版本不一致脚本拒绝构建，防两边行为分叉 |
| dav1d 静态库 | `scripts/build-dav1d-win.cmd` 一次性构建，默认落 `C:\rb-deps\dav1d-1.5.0\`（meson + ninja + nasm + VS Build Tools） | Windows 没有 pkg-config；AVIF 解码要静态链接 |
| Windows 本地盘 | 构建目录必须是本机盘（**不能**网络盘 / UNC / WSL 挂载盘）：debug 默认 `C:\rb-target`，release 默认 `%LOCALAPPDATA%\raybend\build` | 跨盘构建又慢又不可靠，脚本会直接拒绝 |
| WSL 互操作 | WSL 里能调 `wslpath`、`cmd.exe`、`powershell.exe` | 脚本靠它们转换路径、发起 Windows 构建 |
| 磁盘 | Windows 侧 target 会长到 GB 级（本机 release target 实测 4.6 G） | 见附录 J |

```bash
# ① Windows 侧装与 WSL 一致的 cargo-tauri（在 WSL 里执行，经 cmd.exe 进 Windows）
cmd.exe /d /c "cargo install tauri-cli --version 2.11.4 --locked"

# ② Windows 侧构建 dav1d 静态库（换机器 / 换盘才需要重跑）
cmd.exe /d /c "scripts\\build-dav1d-win.cmd"
```

### 2.2 验收

```bash
pnpm debug:win          # pnpm build + 跨编译 + check:win + 运行（dav1d 环境已封装）
pnpm check:win          # 产物时间与内容核对：主程序/worker 协议、嵌入前端版本
pnpm check:diagnostics  # 可选：诊断层真机冒烟（日志 / 崩溃记录 / 导出报告）
pnpm spike:win          # 可选：原生 GPU 视口 spike 窗口
```

### 2.3 这一层的边界

- **不需要** `gh`、签名密钥、crates.io、Pages 配置 —— 那是第三层。
- debug 包**不带更新签名公钥**（`RAYBEND_UPDATER_PUBLIC_KEY` 只由 release 流程注入），构建通道是 `test`：
  按设计，更新功能在 debug 包里整体不可用（界面会说明「此构建没有内置更新签名公钥」）。
- `pnpm debug:win` 与 `pnpm release` 共用同一套 Windows 构建模块与 dav1d 封装，所以第二层验过，
  第三层的构建侧不会再冒新问题；第三层多出来的是签名、上传、官网、crates.io。

---

## 3. 第三层：正式发布（在第二层之上）

### 3.1 需要什么（新增）

每条都写清**不满足时的症状** —— 就是为了不出现「做到一半才发现」。

| 项 | 要求 | 不满足时的症状 |
| --- | --- | --- |
| GitHub CLI | WSL 里装 `gh` 并**用个人令牌登录**（scopes：`repo` + `workflow`），且该令牌**能触发**后续 Actions | 上传失败；或上传成功但 Release 事件**不触发官网 workflow**（用仓库自带 `GITHUB_TOKEN` 的典型症状） |
| 仓库远端 | `origin` 必须是 `C-Thun/raybend`，当前分支必须是 `master` | 脚本在动手前阻断（不自动切分支、不合并、不强推） |
| Pages | 仓库 Pages 来源 = **GitHub Actions**；自定义域 + HTTPS 按 `website/AGENTS.md` §5 配好 | Release 公开了、官网却不更新 |
| `github-pages` environment | Settings → Environments → `github-pages` 允许 **tag 规则 `v*`**（默认只有分支规则 `master`） | release 事件的 ref 是 tag，被环境规则直接拒绝：`Tag "v0.1.2" is not allowed to deploy to github-pages due to environment protection rules` |
| 更新签名密钥 | 私钥与密码放仓外安全位置；fish 里 `set -Ux TAURI_SIGNING_PRIVATE_KEY`（**文件路径**）与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`；公钥写进 `release.config.json` 的 `updaterPublicKey` | 加 `--with-updater` 时签名失败 / 找不到私钥 / 报密码错；密钥与公钥不配对时构建会打印警告 |
| crates.io | GitHub 登录 + **在 crates.io 设置页验证邮箱** + 建 API token（`publish-new` + `publish-update`）+ `cargo login` | 核心库同步那一步报 400（未验证邮箱）或未认证；**安装包本身不受影响** |
| AI 模型库（可选） | 同级 `../model-registry`，或 `pnpm ai:use <结对路径>` 登记；不想带 AI 就 `--ai=off` | 默认 `--ai=auto` 会**出无 AI 版**并在构建日志打印原因，不会失败 |
| 磁盘 | `release-out/` 有空间（每版约 130 M：MSI 120 M + 签名与清单） | 打包末尾失败 |

```fish
# 更新签名密钥（一次性；私钥永不进仓库。fish 语法，set -Ux 是持久变量）
chmod 600 $HOME/.config/raybend/updater.key
set -Ux TAURI_SIGNING_PRIVATE_KEY $HOME/.config/raybend/updater.key
set -Ux TAURI_SIGNING_PRIVATE_KEY_PASSWORD '生成密钥时设的密码'
```

```bash
gh auth status                                                                  # 登录与 scopes
gh api repos/C-Thun/raybend/environments/github-pages/deployment-branch-policies  # 应有 branch: master 与 tag: v*
```

密钥变量的语义（哪个变量谁读、互斥关系、源码依据）在**附录 A**；证书与商店路线在**附录 B**。

### 3.2 发布前自检（全过再发动真格的）

```bash
pnpm release test --win-msi --unsigned --dry-run                 # 计划与路径映射（不构建、不改文件）
pnpm release test --win-msi --unsigned --with-updater --dry-run  # 私钥可读 + 路径能转成 Windows 形式
gh auth status && git status --porcelain && git log --oneline -1 # 登录 OK、工作树干净、HEAD 就是要发的提交
```

再加一条**纪律**：正式发布一旦发动，**构建与发布期间不要动工作树**（任何改动、哪怕已提交都会让发布在校验处中断）。
细则见 §7.4，事故见 §8。

---

## 4. 指令 · 本地调试

| 命令 | 干什么 |
| --- | --- |
| `pnpm dev` | 只跑前端 dev server（浏览器里看界面，没有原生窗口与 Rust 后端） |
| `pnpm tauri dev` | 完整应用：Rust 后端 + 原生窗口（经 WSLg 显示）；日常主用 |
| `cargo check --workspace` | Rust 快速检查（`--workspace` 才有所有成员；测试目标见 §4.1） |
| `cargo test --workspace` | Rust 测试门：**只认这一条**（见 §4.1） |
| `pnpm typecheck` / `pnpm test` | 前端类型检查 / 单测（`node --test`） |
| `pnpm test:release` | 发布脚本自身的单测（改 `scripts/lib/**` 后必跑） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 三道静态门：硬编码色值 / 分层依赖 / 硬编码中文 |
| `pnpm build` | 前端产物进 `dist/`（**Windows 构建前必须先跑**，见 §4.2） |
| `pnpm smoke:ui` | 运行时冒烟（需另开 `pnpm dev`，可带 URL） |
| `pnpm shot` | 截图辅助 |
| `pnpm perf:browse` / `perf:grid` / `perf:win` | 性能基准（重负载场景） |
| `pnpm crash:drill` / `migrate:drill` | 崩溃记录 / 数据库迁移演练脚本 |
| `pnpm check:xmp` | XMP 侧车审计（Rust example） |

### 4.1 Rust 测试门的一条红线

**只认 `cargo test --workspace`**：`cargo check --workspace` 不编 `#[cfg(test)]` 目标，
`cargo test -p <crate>` 只编那个包 —— 新增的 crate / bin 的测试目标会整个漏掉。
2026-10-12 的教训：`crates/raybend-crashpad` 的测试从诞生起就编不过，直到某次全量才发现。
报「测试全绿」时必须写清范围与命令原文。

### 4.2 前端必须先构建

Windows 构建把 `dist/` **在编译期嵌进 exe**：改了前端却只跑 Windows 构建，界面就是旧的。
`pnpm debug:win` / `pnpm release` 会自己先跑 `pnpm build`；手工跑 `pnpm tauri build` 时要自己记得。

---

## 5. 指令 · 本地 debug 包

| 命令 | 干什么 |
| --- | --- |
| `pnpm debug:win` | 主入口：`pnpm build` → 镜像到 Windows 本地盘 → Windows cargo 构建（主程序 + worker）→ `check:win` → 运行 |
| `pnpm check:win` | 只核对已有产物：时间戳、嵌入的前端版本、主程序/worker 协议标签 |
| `pnpm check:diagnostics` | 诊断层真机冒烟（日志 / 崩溃记录 / 导出报告）；需要 debug:win 产物 |
| `pnpm spike:win` | 起原生 GPU 视口 spike 窗口（`--spike=1`） |
| `pnpm clean:win` / `pnpm clean:wsl` | 清理 Windows / WSL 侧构建缓存（只清「不在当前构建图里且已凉 ≥3 天」的产物） |
| `pnpm clean:report` | 磁盘占用总账（各缓存位置实测大小与清理命令） |

常用参数：

```bash
pnpm debug:win --ai=auto                  # 默认：模型库找不到就出无 AI 版（日志打印原因）
pnpm debug:win --ai=required              # 找不到就失败，绝不静默降级
pnpm debug:win --win-dir 'D:\raybend-build'   # 换构建盘（也接受 WSL 挂载路径写法）
```

- 构建目录默认由 Windows 的 LocalApplicationData 推出（debug 走 `C:\rb-target`）；`--win-dir` 优先于环境变量 `RAYBEND_WIN_BUILD_DIR`；
  未挂载盘、网络盘、UNC、相对路径会被直接拒绝。
- Windows 构建现场：`<构建盘>\…\source\`（源码镜像）+ `target\`（持久编译缓存）。
  镜像只含构建需要的东西（清单、工具链文件、`crates/`、`src-tauri/`、`src/`、`public/`、许可与本轮前端），不带 Git / node_modules。
- 并发保护：WSL 侧 `.release/build.lock` + 镜像同级 `source.lock`；被强杀后若报残留锁，确认两侧进程都已结束再删锁（§7.2）。

---

## 6. 指令 · 正式发版包

### 6.1 一条命令（推荐）

```bash
# 自增 patch：0.1.2 → 0.1.3，构建 MSI 并一路发完（提交 / tag / 推送 / 上传 / 公开 / 等官网）
pnpm release patch --win-msi --unsigned --publish

# 指定版本号（位置参数或 --version；接受 v 前缀与预发布标签；必须严格大于当前版本）
pnpm release 0.2.0 --win-msi --unsigned --publish

# 跳过 crates.io 同步（默认跟着发）
pnpm release patch --win-msi --unsigned --publish --no-crates
```

`--publish` 的硬规则：必须同时指定 `--win-msi` / `--win-nsis`；不能与 `--dry-run`、`--skip-build`
或 `test` 通道同用；它调用的发布逻辑与下面的两条指令**完全是同一个函数**，不是第二条通路。

### 6.2 两条指令（需要验收窗口时）

想在中间停一下，人工验一下安装器、核对/编辑 `release-out/v版本/RELEASE-NOTES.md`：

```bash
pnpm release patch --win-msi --unsigned           # 准备：构建 + 生成 release-out/v0.1.3/
# 在 Windows 装一遍、核对 RELEASE-NOTES.md 之后：
pnpm release:publish release-out/v0.1.3 --execute # 发布：提交 / tag / 推送 / 草稿 / 上传 / 公开 / 等官网 / crates.io
```

`release:publish` **默认只读预览**，必须显式 `--execute` 才会联网与改 git。

### 6.3 参数速查

| 参数 | 作用 |
| --- | --- |
| `patch` / `minor` / `major` / `x.y.z` | 版本来源（自增或指定；beta 线有自己的递增规则） |
| `test` | 不升版的自测通道：产物进 `release-out/test-<时间>/`，**不能公开上传** |
| `--win-msi` / `--win-nsis` | 安装器类型，可同时给（只编译一次，打两种）；**beta 与预发布只能 NSIS**（MSI 数值版本区分不了 beta） |
| `--unsigned` | 明确接受无 Authenticode 签名（官网直下路线） |
| `--with-updater` | 生成带签名更新的产物（`.sig` + `latest.json`）；需要 §3.1 的密钥 |
| `--ai=auto|required|off` | AI 模型外挂策略（附录 D） |
| `--no-crates` | 这次不同步 crates.io |
| `--publish` | 准备完直接发布（§6.1） |
| `--channel beta` | 预览通道（只有 NSIS） |
| `--win-dir <路径>` | 换 Windows 构建盘（同 §5） |
| `--dry-run` | 只打印计划与路径映射，不构建、不写文件 |
| `--skip-build` | 复用已有构建产物（发布侧仍会校验来源一致） |

### 6.4 两条指令各自做了什么（排障时看这段）

**第一条（准备）**：同步版本号（`package.json` + `Cargo.toml` workspace 版本 + `Cargo.lock` 里**全部 workspace 成员**，
由 `[workspace] members` 动态解析）→ 重生成许可清单 → 前端构建一次（WSL）→ 镜像源码到 Windows 本地盘 →
Windows cargo 构建主程序与 worker → 打所选安装器 → 核对嵌入前端与 worker 协议 → 验签（或显式 unsigned）→
生成全新的 `release-out/v版本/`。
产物目录含：安装器、可选 `.sig`、`SHA256SUMS`、`release-index.json`、`raybend-build.json`、
可选 `latest.json` 与 `RELEASE-NOTES.md`；**只收本版文件**，缺目标安装器直接失败，旧包不混入，目录必须全新。

**第二条（发布）**：核对安装器哈希、更新签名与地址、版本事务、许可锁摘要、构建时源码快照 →
只自动提交 `package.json` / `Cargo.toml` / `Cargo.lock` / `public/legal/third-party.json` →
创建本版 tag → 原子推送 `master` + 本版 tag → 建草稿 Release → 上传全部资产 → 核对远端 SHA-256 →
公开 Release → 等官网 Actions（Release 事件触发：注入版本与安装包直链、构建、发 Pages）→
（正式版）同步 crates.io。

### 6.5 重跑与中断的语义

- **网络中断**：保留草稿与已上传资产；修复后重跑同一条发布指令 —— 已上传资产按字节比对，只补缺项，冲突不覆盖；
  草稿里留下的零字节占位会被删除重传，已完成资产不删。
- **公开后官网失败**：报错，但 **Release 已公开的事实不回滚**；排障后重跑不会重传安装器，
  对匹配的已失败官网工作流会自动重跑一次并等待。再失败就照日志修，不反复重传或循环重跑。
- **不能用发布指令续跑的情形**：构建来源之后又出现了**新的独立提交**（HEAD 的父不再是构建来源）——
  校验会拒绝「HEAD 已离开构建来源」。要么重新构建，要么手工收口（附录 E）。
- **手工收尾发 crate 时**要趁着工作树还等于 tag 内容（`cargo publish` 要求干净工作树），否则 crate 内容与 tag 不一致。

---

## 7. 变化与应急（按层）

### 7.1 第一层

| 变化 / 症状 | 做法 |
| --- | --- |
| `pnpm tauri dev` 报找不到 webkit2gtk / pkg-config | 按 §1.1 装齐图形库；改动系统库后重开终端 |
| Rust 版本不对 / 提示 toolchain 缺失 | 交给 `rustup`：`rust-toolchain.toml` 会拉 1.98.1；不要 `rustup default` 改全局 |
| 依赖变更（升级 tauri / wgpu 等） | 改完跑 `pnpm install` + `cargo check --workspace` + `pnpm test:release`；版本基线变动要同步 `memory/ARCHITECTURE.md` |
| WSL 升级 / 换发行版 | 重跑 `corepack enable` 与 §1.1 的 apt 清单；Windows 侧不受影响 |
| AVIF 相关报错（WSL） | 确认 `libdav1d-dev` 在：`dpkg -l libdav1d-dev`；WSL 侧**不该**出现 `SYSTEM_DEPS_DAV1D_*` 的用法 |

### 7.2 第二层

| 变化 / 症状 | 做法 |
| --- | --- |
| 报「tauri-cli 版本与前端不一致」 | 按 §2.1 重装匹配版本；升级前端 `@tauri-apps/cli` 时同步重装 Windows 侧 |
| Windows 构建报找不到 dav1d | 确认 `C:\rb-deps\dav1d-1.5.0\` 在；不在就重跑 `scripts\build-dav1d-win.cmd`（需要 meson/ninja/nasm + VS Build Tools） |
| 换构建盘（C 满 / 换 SSD） | `--win-dir '<新盘>\raybend-build'` 或设 `RAYBEND_WIN_BUILD_DIR`；**第一次会冷编译**（十几分钟起） |
| 报「残留锁」 | 确认 WSL 与 Windows 两侧的构建进程都已结束，再删报错指出的锁（`.release/build.lock` 或镜像侧 `source.lock`） |
| 报「镜像目录已有归属其它脚本」 | 说明该目录不是本流程建的：换一个空目录，别手改标记文件 |
| 「看起来还是旧图标 / 旧界面」 | 先怀疑 Windows 图标缓存与 dist 未重建：用 exe 资源字节比对判定，别用眼睛；确认先跑了 `pnpm build` |
| MSI 打包失败报 VBScript / light.exe | 属 Windows 可选功能缺失，按 Tauri 官方 Windows 安装器文档配置，别一律归咎于路径 |
| 磁盘吃紧 | `pnpm clean:report` 看总账，再按附录 J 分别清理（旧 `C:\rb-target` 有 17.9 G 的前科） |

### 7.3 第三层

| 变化 / 症状 | 做法 |
| --- | --- |
| 换更新签名密钥 | 生成新密钥对 → 公钥写进 `release.config.json` → 旧版本无法自动更新到新密钥签的包（**要么保持密钥不变，要么接受一次手动升级**）|
| 报「找不到私钥 / 密码错」 | 确认 `TAURI_SIGNING_PRIVATE_KEY` 是**文件路径**且 WSL 能读；带密码的密钥必须给 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（不设时 CLI 会尝试交互提示，而经 `cmd.exe` 发起的构建没有交互） |
| 上传中断 | 见 §6.5：保留现场，重跑同一条发布指令 |
| Release 公开了但官网 404 / 旧内容 | 看 Actions 里 `website.yml` 这次 run 的失败原因；若是 environment 规则（`v*` tag 未放行）→ 见 §3.1 加规则后重跑 |
| crates.io 报 400 / 未认证 | 检查 crates.io 的邮箱是否已验证、token scope、`~/.cargo/credentials.toml`；失败无副作用，修好重跑（附录 C） |
| 核心库这次没变动 | 脚本会**自动跳过** crates.io（按上次发布 tag diff `crates/raybend`）；不是错误 |
| 想先发没有更新的版本 | 不加 `--with-updater` 即可（老客户端不会有新版本提示） |
| 密钥 / 仓库身份变了（换组织、换域名） | 同步改 `release.config.json`、来源校验（脚本只认本仓 master）、website 配置；这是待审阅事件，不要静默改 |

### 7.4 发布期纪律（红线）

- **构建 / 发布进行中禁止改工作树**：`pnpm release` 与 `release:publish` 都按**构建时的文件快照**校验工作树
  （`raybend-build.json` 的 `sourceFiles` + 版本文件逐字节比对）。期间任何改动 —— 哪怕只给 `package.json`
  加一行脚本、哪怕已经提交 —— 都会让发布在校验处中断，而且失败回滚会与已提交内容冲突
  （报错会变成 `AggregateError`，真实原因被吞掉）。动仓库前先看 `.release/build.lock` 与镜像侧 `source.lock`。
- **发布动作由人类执行**：Agent 不碰私钥、不 push、不打 tag、不发布。
- **发布提交只含版本文件**：`package.json` / `Cargo.toml` / `Cargo.lock` / `public/legal/third-party.json`；
  其它改动一律先由人提交，否则发布流程会阻断。
- **HEAD 必须等于构建来源**：发布前 `git log --oneline -1` 核对。

---

## 8. 踩过的坑（症状 → 根因 → 做法）

| 症状 | 根因 | 做法 / 落点 |
| --- | --- | --- |
| `cargo metadata --locked` 拒绝写锁：`cannot update the lock file … --locked` | 发版只同步了两个包在 `Cargo.lock` 里的版本，**新增的 workspace 成员漏了**（首次是 `raybend-crashpad`） | `versionEdits` 改为按 `[workspace] members` 动态解析（`d3a03ac`）；新增成员自动跟上 |
| 「创建后未找到 Release，请重跑同一指令」（草稿明明建成了） | `GET /releases/tags/<tag>` **对 draft 固定 404**（GitHub API 行为） | 改用列表端点匹配 `tag_name`（`940eaa4`），带断言防回归 |
| `cargo publish` 上传超时：`[28] Timeout was reached (Connection timed out after 30001 ms)` | 默认 30 s 上传超时，直连 crates.io 偶发 | 发布时固化 `CARGO_HTTP_TIMEOUT=600` / `CARGO_NET_RETRY=5`（`940eaa4`），已有环境变量不覆盖 |
| 官网部署失败：`Tag "v0.1.2" is not allowed to deploy to github-pages due to environment protection rules` | `github-pages` environment 只允许 `master` 分支，而 release 事件的 ref 是 tag | 加 tag 规则 `v*`（§3.1）；文档与流程都已写死这条前置 |
| 发布中途失败，报错是 `AggregateError` / `版本文件含升版以外的改动` | 构建期间改了工作树（提交了别的改动） | §7.4 红线：构建/发布期间不动工作树；本次靠 `git reset` + `git stash` + 快照对齐恢复 |
| `cargo publish` 报 `400 … A verified email address is required` | crates.io 有自己的邮箱字段，GitHub 验证过**不能代替** | 在 crates.io 设置页验证邮箱后重跑（无副作用）（附录 C） |
| 自测签名报「找不到私钥」 | `TAURI_SIGNING_PRIVATE_KEY_PATH` **只有 `signer sign` 读**，`build/bundle` 不读 | 构建用 `TAURI_SIGNING_PRIVATE_KEY`（路径或内容都行）；自测才用 `_PATH`，且两者**互斥**（附录 A） |
| 签名报 `incorrect updater private key password` | 密钥带密码却没给 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 设上密码变量；`cmd.exe` 发起的构建没有交互提示可以依赖 |
| Windows 上界面是旧的 / 白屏 | ① 忘了 `pnpm build`（`dist/` 编译期嵌进 exe）② 缺 `--features custom-protocol` | 走 `pnpm debug:win` / `pnpm release` 这两个入口，它们已经处理好；手工构建要自己带 |
| 主程序与 worker 协议对不上 | 只重建了主程序没重建 worker | 两个一起建（`-p raybend-desktop -p raybend`）；`pnpm check:win` 会核对 |
| Rust 测试「全绿」但某个 crate 的测试其实编不过 | 只跑了 `cargo test -p <crate>`（或只 `cargo check`） | 只认 `cargo test --workspace`（§4.1） |
| 旧构建目录占 17.9 G | 2026-09-29 前的默认位置 `C:\rb-target` 被新流程弃用后没人清 | `pnpm clean:report` 看账；`pnpm clean:win --target-dir 'C:\rb-target\raybend' --keep-days 0` |
| beta 版打 MSI 失败 | MSI 是数值版本，区分不了 `beta.N` | beta / 预发布一律 `--win-nsis`（脚本不做静默替换） |
| 调试包被拉到正式版 | 早期实现里 test/dev 通道也会自动检查更新 | 现在按通道分档：只有 release/beta 构建自动检查；debug 包连公钥都不带（§2.3） |
| 更新下载中断就得从头再来 | 官方插件的 `download()` 一次性收整包进内存，续不了传 | v2 自研 Range 续传（落盘 `.part` + 状态 `state.json`），规格 `specs/update-flow-v2.md` |
| AI 版突然变无 AI 版 | 模型在独立仓库（git LFS），本仓没有 | 默认 `--ai=auto` 会退化成基础版并打印原因；要强制就 `--ai=required`（附录 D） |
| 每次打 MSI 都报 4 条 `warning LGHT1076`（ICE03 / ICE40 / ICE57 / ICE61） | 全部来自 **Tauri 官方 WiX 模板**（写死的，不是我们的配置错） | **属正常噪音，不影响结果** —— 证据与逐条解释见 §8.1 |

### 8.1 看着像错误、其实是噪音的 MSI 警告

每次 Windows 打包末尾都会出现这四条（`light.exe` 的 ICE 校验警告，**不阻断构建**）：

```text
warning LGHT1076 : ICE03: String overflow ... Table: CustomAction, Column: Target, Key(s): DownloadAndInvokeBootstrapper
warning LGHT1076 : ICE40: REINSTALLMODE is defined in the Property table. This may cause difficulties.
warning LGHT1076 : ICE57: Component 'CMP_UninstallShortcut' has both per-user and per-machine data with an HKCU Registry KeyPath.
warning LGHT1076 : ICE61: This product should remove only older versions of itself. The Maximum version is not less than the current product. (0.1.3 0.1.3)
```

出处（`tauri-bundler/src/bundle/windows/msi/main.wxs`，都在模板里写死，不随我们的 `tauri.conf.json` 变）：

| 警告 | 模板里的源头 | 到底是怎么回事 |
| --- | --- | --- |
| ICE03 | 第 294 行 WebView2 **下载引导器**的 `CustomAction`（内联 PowerShell 命令；默认 `webviewInstallMode: downloadBootstrapper`） | 命令长 407 字符，ICE 按老的列宽限制报警；**但 MSI 表里存的是完整命令**（已读数据库核对，没截断）⇒ 引导器能正常跑 |
| ICE40 | 第 33 行 `<Property Id="REINSTALLMODE" Value="amus" />` | Tauri **故意**这么设（重装全部文件、重写注册表、重建快捷方式），为的是自更新时覆盖得干净 |
| ICE57 | 第 177 行卸载快捷方式组件（perMachine 包里写了 HKCU 的 KeyPath） | Tauri 模板如此；共享机器上可能给其他用户留个卸载快捷方式，不影响安装 / 升级 / 卸载本体 |
| ICE61 | 第 43 行 `<MajorUpgrade ... AllowSameVersionUpgrades="yes" />` | 为**自更新重装同版本**而开：装了同版本也要走「卸旧→装新」，ICE 于是抱怨 Maximum 不低于当前版 |

**要不要规避**：

- **建议：不规避**。四条都不改变产物行为；v0.1.3 的 MSI 已逐表核对：`RemoveExistingProducts` 在执行序列（升级会先卸旧版）、
  Upgrade 表两行分别指向 `WIX_UPGRADE_DETECTED` / `WIX_DOWNGRADE_DETECTED`（升级正常、降级拦截）、
  `ALLUSERS=1`（per-machine 安装）、引导器命令完整。
- 想只消掉 ICE03：把 `bundle.windows.webviewInstallMode` 换成 `embedBootstrapper`（内嵌约 1.5 MB 引导器，安装时仍联网拉 WebView2）
  或 `offlineInstaller`（内嵌完整运行时，安装包 +130 MB 量级）—— 代价是体积。
- 想全部消掉：自定义 `bundle.windows.wix.template` 接一份模板进仓 —— 从此要跟上游模板逐版 diff，**不推荐**。
- 想靠 `light.exe -sice:…` 抑制：Tauri 的 bundler 不暴露该参数（只能改上游或提 PR）。

排查手法（只读）：用 Windows 的 `WindowsInstaller.Installer` COM 查 MSI 的 `CustomAction` / `Property` / `Upgrade` / `InstallExecuteSequence`
四张表，就能把「警告到底有没有造成实际后果」问清楚 —— 别靠猜。

---

## 附录 A · 更新密钥与发布源

更新密钥免费自持，独立于 Authenticode。请自己生成并备份加密私钥和密码，不要放进仓库、对话、日志或自动化脚本。

**发布前可选：验签上一个真实产物**（不需要密码，只读）—— 用客户端同一套校验代码确认
「私钥 ↔ 公钥 ↔ 上一版产物」三者配对，避免发布出去才发现旧客户端验不过：

```bash
RAYBEND_VERIFY_ARTIFACT=$PWD/release-out/v0.1.2/RayBend_0.1.2_x64_en-US.msi \
RAYBEND_VERIFY_VERSION=0.1.2 \
  cargo test -p raybend-desktop --lib verifies_a_real_release -- --ignored --nocapture
# ✓ 通过 ⇒ 同一把私钥可以继续签下一版
```

```bash
pnpm tauri signer generate -w /您自己选择的仓外安全目录/raybend-updater.key
```

以安全环境变量/密钥管理提供私钥与密码。**变量关系（已核实，2026-10-14）**：

| 变量 | 用在哪里 | 本项目怎么用 |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | `tauri build`/`bundle` 与 `tauri signer sign` | **必须用这个**：值是私钥文件路径（或私钥内容）。路径会被脚本自动转成 Windows 可访问路径（本机实测 `/home/…` → `\\wsl.localhost\<发行版>\…`，Windows 侧 `Test-Path` 可读） |
| `TAURI_SIGNING_PRIVATE_KEY_PATH` | **只有 `tauri signer sign` 读**，`build`/`bundle` **不读** | 不要用它做发布构建，否则签名时“找不到私钥”；另外它与 `TAURI_SIGNING_PRIVATE_KEY` **同时设置会报互斥错误**（自测签名时才用它） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 与私钥同时使用 | 生成时设了密码就必须给（本机这对密钥**带密码**，实测 `signer sign` 不给密码会报 `incorrect updater private key password`）；生成时没设密码则不用 |

在**您自己的终端（fish）**里一次性设置（`set -Ux` 是持久变量，重开 shell 仍在）：

```fish
chmod 600 $HOME/.config/raybend/updater.key
set -Ux TAURI_SIGNING_PRIVATE_KEY $HOME/.config/raybend/updater.key
set -Ux TAURI_SIGNING_PRIVATE_KEY_PASSWORD '生成密钥时设的密码'
```

验证（分两层，都不输出私钥）：

```fish
# ① 签名自测：signer sign 把 TAURI_SIGNING_PRIVATE_KEY 当「内容」、_PATH 当「路径」，
#    两者同时存在会报互斥 —— 所以自测时用 PATH 变量并临时移除内容变量：
env -u TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PATH=$HOME/.config/raybend/updater.key pnpm tauri signer sign /tmp/rb-sign.txt

# ② 发布路径验证（不构建、不改文件）：确认私钥可读且路径能转成 Windows 形式
pnpm release test --win-msi --unsigned --with-updater --dry-run
```

⚙️ **实现差异（已读 tauri-cli 2.11.4 源码，2026-10-14）**：`bundle.rs` 用
`Path::new(&private_key).exists()` 判断「值到底是路径还是内容」，所以 `build/bundle` 路径两种都支持；
`signer/sign.rs` 则把两个变量分别绑到 `--private-key`（内容）与 `--private-key-path`（路径）两个互斥参数上。
❗ 本地（非 CI）构建若没设密码变量，CLI 会尝试在终端**交互提示输入密码**；发布构建经 `cmd.exe` 批处理发起，
不要依赖交互提示 —— 把 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 设上。构建时 CLI 还会比对私钥与 `release.config.json`
公钥是否匹配，不匹配会打印警告（首次构建可用它当配对自检）。

**这两个变量什么时候用到**：只有**构建带更新的安装包**（`--with-updater`）时 —— Tauri 在 Windows 侧为每个安装器生成 `.sig`，签名至此固化；`release:publish` 只是上传，不需要私钥。平时发版（不带 `--with-updater`）完全不需要它们。**不配给 Agent 的 bash**：私钥不由 Agent 保管，带签名的构建与发布全部由您执行。

**公钥写进仓库的公开配置**：`release.config.json`（仓库根）的 `updaterPublicKey`（`schema: 1`）—— 公钥是公开身份（会进安装包与更新清单），入库后每次发版不必再 export；环境变量 `RAYBEND_UPDATER_PUBLIC_KEY` 仍优先，供临时换密钥用。已接入（2026-10-14）。

```json
{ "schema": 1, "updaterPublicKey": "<pnpm tauri signer generate 打印的公钥>" }
```

```bash
pnpm release patch --win-msi --with-updater --unsigned
# 有发布者证书时去掉 --unsigned
```

**客户端更新行为（v2，崔总 2026-10-15 定；规格 `specs/update-flow-v2.md`）**：启动后静默检查一次（默认开，可在更新对话框里关；**测试/开发通道不自动检查**，避免调试环境被拉到正式版）；发现新版本**只问一次**「下载 / 跳过」，跳过只压自动提示（手动检查永远能看到）。下载在**后台**跑（Range 断点续传，进程退出后下次启动接着下），失败不打扰、下次启动静默重试一次；下载完成的**下一次启动**才问一次「安装 / 取消」（取消保留安装包）。界面只留**稳定 / 预览**两个源（预览先灰着留口），不提供自定义地址与公钥输入 —— 那是第三方构建在源码层的事（AGPL-3.0），仍保留签名校验。

**发布源**：稳定源为 GitHub 最新正式 release 的 latest.json；预览源为官网 /updates/beta.json，发布前必须实际部署该文件（预览通道 UI 尚未开放，文件链路保留）。NSIS 和 MSI 使用各自 windows-x86_64-nsis/msi 更新目标，未知/裸 exe 不跨用另一种安装器；稳定源拒绝预发布版本。启动检查与下载会联网 —— 隐私说明与 `updates.privacy` 文案必须与之一致（已同步）。

---

## 附录 B · 证书、SmartScreen 与商店路线

官网直下可以显式选择无 Authenticode 签名；如实写明发布者未知/信任提示，提供源代码 tag、SHA-256 和独立更新签名。官网下载、更新签名、发布者证书、SmartScreen 信誉各自解决不同问题。不要因缺少证书阻断开发，也不要承诺关闭提示。

SignPath Foundation 提供免费开源签名，可申请但需要审批、源仓与构建关联、MFA、签名/隐私政策。商业证书不是本项目的默认购买项；新 EV 也不保证即时 SmartScreen 放行。来源：[SignPath 条件](https://signpath.org/terms.html)、[微软签名选项](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)、[信誉机制](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)。签名服务返回的安装器应在最后生成 .sig/校验和，不能签完后再改二进制。

Microsoft Store 当前新入口个人/公司注册免费，仍需身份验证：[开户文档](https://learn.microsoft.com/en-us/windows/apps/publish/partner-center/open-a-developer-account?tabs=individual)、[注册入口](https://storedeveloper.microsoft.com/)。MSIX 认证后商店代签；EXE/MSI 商店提交仍需自己的发布者签名。不是“把 EXE 上传就得到免费签名”。商店信任也不延伸到官网 EXE。

### 证书路线对照（2026-09-27 核实）

上面是当前可直接执行的路线；这一节是“以后要买/申请证书”时的对照，避免重复调研。共同前提：**最短路径是让证书进入本机 Windows 证书存储**（Token 中间件或导入 .pfx），再设 `RAYBEND_SIGN_CERT_SHA1` 并去掉 `--unsigned` —— 脚本现有 `signCommand` 就是 signtool + 证书存储指纹，云签名服务都要另补一层适配。

| 路线 | 与当前脚本 | 关键限制 |
| --- | --- | --- |
| 证书存储（Token / .pfx） | **开箱可用** | Token 方案可能弹 PIN；续期换指纹 |
| SignPath Foundation（免费开源） | 需适配，且其模型要求在 SignPath 构建链上验证来源 | 需审批、公开源仓与构建关联、MFA、签名政策、每版人工批准；发布者显示 SignPath Foundation |
| 商业公开信任证书（OV / IV） | OV+Token 走证书存储路线即可 | 个人主体可走 IV 类（如 SSL.com IV 约 $379/年，可配 eSigner 云签名免 Token）；EV 不绕 SmartScreen，不必加钱 |
| Azure Artifact Signing（原 Trusted Signing） | 需适配（云签名 CLI） | $9.99/月起含 5000 次签名；**地区限制**：公开信任证书的组织限于 US/CA/EU/UK/AU/NZ/JP/KR/SG/CH/NO/IL，个人开发者仅 US/CA —— 中国大陆当前不在名单，本项目不可用 |
| 开源折扣证书 | 同“证书存储” | Certum Open Source Code Signing **不是免费**（约 €49 起），且需其加密卡或 SimplySign 云；官网当前显示 out of stock，需向 Certum 确认 |
| **国内代理通道** | 同“证书存储” | 国内**无**自主公开信任码签 CA（沃通 WoSign 的 5 张根已 2019-03 被微软正式废止），但有代理商卖国外品牌：沃通/天威诚信/零信/火山引擎。**个人主体只能买 IV**（OV/EV 需组织实体，沃通 EV 页明写“仅限单位用户申请”）。零信 ZoTrus 明确提供面向“未注册公司的个人软件开发者”的 IV，国产 UKey 顺丰快递 + 中文电话鉴证——省掉跨境验证与 Token 国际邮寄。沃通含税报价：标准 OV 3588 元/年、OV Pro 4888、EV 4288、EV Pro 6888 |
| 新兴免费开源签名服务 | 未核实 | OSSign https://ossign.org/ 、Necessary Code Signing https://sign.necessary.nu/ 等确实在运营，但签发主体与可持续性未经核实，只作观察项，不作为发行依赖 |

**为什么码签不像 SSL 那样能自助签发**（防止把“买张证书”想得和买域名证书一样简单）：

- **验证对象不同**：DV 型 SSL 只验证“你能控制这个域名”（DNS/HTTP 自动核验），而码签验证“你是谁/你是哪个组织”。CA/B Forum 的码签基准要求（CSBR）强制身份验证（组织/个人），**不存在 DV 级的码签证书**。
- **私钥必须进硬件**：CSBR §6.2.7.4.2 自 2023-06-01 起要求码签私钥在合规硬件加密模块中生成、存储、使用，满足方式实际只有“CA 寄送 Token”或“云签名服务”两种——这是云签名适配绕不开的法源原因，不是脚本挑食。
- **没有免费的自动化 CA**：ACME 只服务域名验证；Let's Encrypt 官方明确不做码签（需真实身份验证，无法机器化）。同时有效期还在收紧（Ballot CSC-31：上限从 39 个月降到 460 天，2026-03-01 起生效），续期更频繁。
- **口径差异**：SSL 无证书是硬门槛（浏览器直接拒绝），码签缺签名只是软门槛（Windows 提示仍可安装）——所以码签是自愿的信任投资，免费替代品因此稀少。

**SmartScreen 信誉的跨版本事实**（决定“先不签、攺够信誉再买证书”是无效策略）：

- SmartScreen 只看两个信号：**发布者信誉**（需要签名证书）与**文件哈希信誉**（这一个确切文件）。未签名文件只剩后者可用。
- 微软原文：未签名时“每个新版本都必须从零开始积累信誉，信誉不能从前一版本转移，除非两者由同一发布者身份签名”。未签名发布 ⇒ 每次发版重新弹窗，即使旧版已被下载百万次。
- 签名不等于立即放行：**每张证书自己攺信誉**（EV 自约 2019 年起不再特殊对待）；换证书按新的发布者身份处理，须保持 Subject 一致（CN/O/L/S/C），并可在 Defender 提交门户用 “Software Developer” 流程提前登记新证书。
- 唯一确定性绕过：**Microsoft Store（MSIX）**，商店重签，永不触发下载警告。Windows 11 的 Smart App Control 更严：未签名文件直接阻止（非提示）。
- 结论：未签名发布是“能用但每版重新面对提示”；**信誉跨版本只能靠签名积累**，不能靠未签名阶段预存。

来源：[微软签名选项](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)、[Azure Artifact Signing 地区条款](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)、[SSL.com IV](https://www.ssl.com/products/software-integrity/code-signing/iv/)、[Certum 开源证书](https://shop.certum.eu/open-source-code-signing-on-simplysign.html)、[CSBR 硬件密钥条款](https://cabforum.org/working-groups/code-signing/requirements/)、[Ballot CSC-31](https://cabforum.org/2025/11/17/ballot-csc-31-maximum-validity-reduction/)、[Let's Encrypt 不做码签](https://community.letsencrypt.org/t/do-you-support-code-signing/370)、[SmartScreen 信誉规则](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)、[SmartScreen 实操建议（Eric Lawrence）](https://textslashplain.com/2024/11/15/best-practices-for-smartscreen-apprep/)。

### 生态现状与 Tauri 官方口径（2026-09-27 核实）

- **抽样 12 个知名开源项目的 Windows 安装器**（GitHub latest release，读 PE 证书表与签发链）：公开信任签名 6 个（Notepad++/GlobalSign OV、OBS、HandBrake、VSCodium、Jan、Yaak）；无证书表 4 个（**7-Zip x64、qBittorrent、ShareX、Spotube**）；MSI 未判定 2 个（KeePassXC、Spacedrive）。头部项目约六成签名；往长尾走 unsigned 是常态——文件名直接带 `-unsigned` 的项目不少（SMPlayer、Block 公司的 Buzz 等），也有项目 signed/unsigned 双发。
- 背景事实：Notepad++ 的 DigiCert 捐赠证书 2025-05 到期后续签被拒（开源项目无注册商业实体），一度只能自签（SmartScreen 视同未签名），后获 GlobalSign OV——顶级项目也会掉进拿不到证书的坑；ShareX 明言“几十年没签名，经费不足”；GitHub 官方社区存在请愿帖要求为开源项目提供签名支持。
- **Tauri 官方文档口径**（v2 sign/windows）：签名“**不是 Windows 上运行的必要条件**，只要最终用户愿意忽略 SmartScreen 警告”；官方路径 = OV+signtool 证书存储（本仓当前这条）、Azure Key Vault（relic）、Azure Artifact Signing（artifact-signing-cli）、自定义 signCommand；**未提 SignPath**。
- **Tauri 维护者补充口径**（官方讨论 #8046）：EV/OV 证书“蠢贵”；EV 自 2024 年起无特殊待遇；实操建议是“每版用同一张证书并长期持有、预期每个新版早期下载仍提示、为此别买 EV”；唯一彻底消除提示的是 Store 分发。

### 6. MSIX 商店路线（本轮后移）

崔总 2026-09-27 明确：登记 FUTURE 后续独立启动，本轮先完成直下发行链。正式落点为 memory/FUTURE.md 的 Windows MSIX 分发条目；没有把它列成本轮发布的前置。同日下午拍板（memory/REVIEW.md R3-01）：**官网直下走通后即启动 MSIX**，定位是「零现金成本解决 Windows 安装体验」的主攻方向——它与直下是并行的两条分发通道，不是证书的替代品；发布顺序见 memory/REVIEW.md §8。

仍需您开户、预留产品名并取得 Partner Center 的 Package Identity Name/Publisher；目前没有这些实际身份，不能提交有效 MSIX。Tauri 官方当前教程覆盖 EXE/MSI，MSIX 需要另用 Windows SDK MakeAppx/打包工具包装，不能复用 NSIS 直接上传冒充 MSIX。

**商店签名与费用（2026-09-27 再次确认，比预期更优）**：微软官方原文——“MSIX/AppX 包不必用受信任 CA 的证书签名；认证通过后商店会自动用微软证书重签”，即**无需购买任何商业证书**。开发者账号费用已归零：个人 $19 于 2025-09 豁免（身份证+自拍验证，近 200 市场），公司 $99 于 2026-05 豁免。两个边界：**只有 MSIX/AppX 免签**，用 MSI/EXE 走商店仍需自己签；Tauri 原生只出 NSIS(.exe)/MSI，所以吃到这条红利的前提是先做 MSIX 包装（即本节描述的额外工程），不是现有产物换个后缀。

包装前先准备编译时 RAYBEND_DISTRIBUTION=store 的构建（前端与 Rust 两端一致），内置更新由编译身份禁用。现有 CLI 直下包固定 direct；商店构建和包装流水线在拿到身份后另接，不能拿 direct 包换个后缀上架。

验收须覆盖 MSIX 数据位置与直下版迁移、卸载数据保留、自重启 RAW worker、导入目标/导出目标权限、WebView2、外部编辑器启动和未来更新方式。商店版本交由商店更新，不能同时用内置 NSIS updater 改写程序。产品中尚不显示“商店认证”或虚假商店链接。

---

## 附录 C · crates.io 核心库同步发布

发布对象只有 `crates/raybend`（核心库）；`src-tauri` 带 `publish = false` 守门，防误发桌面外壳。
**发布本体由崔总执行**（AGENTS.md §2.1）。规格与判定细则见 `specs/m5-crates-publish.md`。

### 日常：跟着发版走，不用单独操作

正式版（`--channel release`）走既有那条发布指令时，核心库会在 **GitHub Release 公开之后**自动同步到
crates.io：

```bash
pnpm release patch --win-nsis          # 准备（Agent 可跑；不产生外部副作用）
pnpm release:publish release-out/v0.1.2 --execute   # 发布（崔总；提交/tag/推送/Release + crates.io）
```

* **`--no-crates`** 跳过 crates.io（只发安装包）；**beta / test 版不发**（预发布版本不可撤回，不值当）。
* **核心库没变动就跳过**（崔总 2026-10-14 定）：发版前按「crates.io 上最新已发布版本」的 tag 比对
  `crates/raybend/`，没有改动就不发这一版，**等下个核心库有改动的版本再发**。没有可比的 tag（首次发布、
  历史版本没打 tag）或拿不到构建来源 commit 时按「有改动」处理 —— 宁可多发一次，也不静默漏发；
  本次发布（首个带 tag 的版本）之后，后续版本即可精确比对。
* **重跑安全**：同一条指令重跑时，若该版本已在 crates.io，本地会重新 `cargo package` 并与索引里的
  sha256 比对——一致就跳过，不一致就报错（版本号在 crates.io 不可覆盖，只能升版）。
* **登录凭据是前提**：脚本只检查 `~/.cargo/credentials.toml` 或 `CARGO_REGISTRY_TOKEN` 是否存在，
  缺了会在动手前报错（不会发一半）。
* 不带 `--execute` 的预览**不联网、不写 git**；它只把「执行时会做什么」打出来。

### 发布的是什么

真实核心库源码（当前约 8 万行 Rust + 单测），不是 `0.0.0-reserved` 空壳——crates.io 的
[使用政策](https://crates.io/policies)把「只占名、无真实功能」列为可回收行为。

### 首次发布（一次性，2026-10-09 已完成）

1. 用 GitHub 账号登录 https://crates.io —— **crates.io 没有独立注册**，GitHub OAuth 即账号。
2. **在 https://crates.io/settings/profile 设置并验证邮箱** —— crates.io 有自己的邮箱字段，
   GitHub 侧验证过**不能代替**。未验证时 `cargo publish` 会在上传那一步（打包与本地编译都已过）返回
   `400 Bad Request: A verified email address is required to publish crates to crates.io`。
   **此失败无副作用**：没有落盘任何版本，验证完重跑同一条命令即可，token 不受影响（当天实测踩到）。
3. 在 https://crates.io/settings/tokens 创建 API Token：作用域 `publish-new`（首次需要）+
   `publish-update`，crate 范围写 `raybend`，有效期取最短。**token 只在创建时显示一次**，
   自管保存，不要写进仓库或脚本。
4. 本机 `cargo login`（不带参数，token 从 stdin 读；写成 `cargo login <token>` 会警告已弃用，
   且把密钥暴露在命令行参数与 shell 历史里）。写入 `~/.cargo/credentials.toml`，不进仓库。

### 手工兜底（脱离发版流程单独发核心库时）

```bash
cargo publish -p raybend --dry-run   # 打包 + 真编译校验，不上传
cargo publish -p raybend             # 上传（崔总执行）
```

* **必须在干净工作树上发布**；有未提交改动时 `cargo publish` 会拒绝，加 `--allow-dirty` 会把未提交源码
  打进包里（不可撤回），不要用。手工发时不比对索引——重复版本会被 crates.io 直接拒绝（无副作用）。
* **发出去不可覆盖、不可删除**，只能 `yank`；要改就升版本号（`Cargo.toml` 里 `version.workspace = true`，
  `pnpm release` 会自动同步 package.json / Cargo.toml / Cargo.lock 三处）。
* 体积红线：crates.io 单包上限 **10 MiB**（本包约 1.1 MiB / 236 个文件）。`crates/raybend/assets/ai/` 下的
  ONNX 模型与 onnxruntime DLL 已被 `.gitignore` 排除，不会进包。
* docs.rs 文档为异步自动构建（可能因缺系统库失败），**失败不影响发布本体**，只是页面上没有 API 文档。
* 将来可选配置 Trusted Publishing（GitHub Actions 免长期 token），但**首次发布不适用**（crate 必须先存在）；
  是否启用属发布方式变更，需崔总拍板。

---

## 附录 D · AI 外挂编译（`--ai`）

AI 能力是**编译期外挂**：模型与 CPU 推理库不进本仓 Git，构建时按校验过的字节装进安装包。
功能规格见 `specs/ai-model-library-build.md` 与 `memory/FUNCTION-IMAGING.md`；这里只讲发布侧怎么用。

### 输入从哪来

| 输入 | 位置 | 获取方式 |
| --- | --- | --- |
| 模型（`image_encoder.onnx` 152 MB + 词表/许可） | **独立的 `model-registry` 仓库**（git LFS，云上另存） | `pnpm ai:use` 显式登记，或构建时自动探测同级 `../model-registry` |
| CPU 推理库（onnxruntime 1.28.0 DLL，约 16 MB） | `crates/raybend/assets/ai/ort-win-x64/`（已 gitignore） | `pnpm ai:prepare`：从 onnxruntime 官方 Release 固定 URL 下载 + SHA-256 校验后解出 |

模型绑定的是 `profiles/raybend-photo-tags/v1`（消费侧契约）：`manifest.json` 的摘要必须与
`crates/raybend/assets/ai/tinyclip-v1/manifest.json` 一致，否则拒绝使用 —— 不是“换个模型就上”。

### 模型库在别处：怎么指定路径

构建**只认路径、不认裸 ONNX 文件**：模型必须先是一个**已初始化的 model-registry**（含 `.model-registry.json`
与 `profiles/`），因为构建要按 artifact/profile 的摘要与契约校验字节；随便指一个 `.onnx` 文件会被拒绝。指定顺序：

```bash
# 1) 已在别处有模型库（如 clone 下来的 model-registry）：登记结对路径，一次即可
pnpm ai:use /结对路径/model-registry
#    要求该目录已初始化；登记写入 ai-model-source.local.json（Git 忽略）；
#    已登记优先于「同级自动探测」—— 登记了就按登记的走

# 2) 在新位置建库（空库，随后导出/导入模型）
pnpm ai:library:init /新位置/model-registry
pnpm ai:export /新位置/model-registry --import /已导出的模型目录

# 3) 不想要登记（回到零配置自动探测）
rm ai-model-source.local.json
```

- **`--ai` 本身不带路径参数**（`auto|required|off` 只决定“找不到时怎么办”）：模型路径只通过
  `ai-model-source.local.json`、同级探测提供；`RAYBEND_AI_RUNTIME_DIR` 只服务 CPU 运行库 DLL，**不是模型**。
- 换了模型 = 换了摘要：raybend 侧的准入 manifest（`crates/raybend/assets/ai/tinyclip-v1/manifest.json`）要同步更新，
  属**待审阅事件**，不会自动跟随（见 `specs/ai-model-library-build.md`）。

### 命令与回退链

```bash
pnpm release patch --win-msi --unsigned --ai=auto      # 默认：找不到就出无 AI 版并打印原因
pnpm release patch --win-msi --unsigned --ai=required  # 找不到就失败，绝不静默降级
pnpm release patch --win-msi --unsigned --ai=off       # 不碰模型源、不联网，明确要无 AI 版
pnpm debug:win --ai=auto                               # 调试包同一套口径（release 与 debug 一致）
```

- **默认 `auto` 的回退链**：`ai-model-source.local.json` 登记 → 同级 `../model-registry` 自动探测（不写登记文件，同样过全部摘要与契约校验）→ 都没有就构建基础版，并在构建日志打印 `⚠ AI 未启用：…`。
- `--ai` 与 `--publish` 可同用：一键发版照样按上面的规则决定带不带 AI。
- 产物有身份记录：前端标志、`features=photo-ai-runtime`、`raybend-ai-build.json`、二进制内的 `on`/`off` 标记与模型摘要；`pnpm check:win` 会核对它们是否一致（无 AI 包残留旧模型目录也会报错）。
- 带 AI 的安装器比基础版大约 160 MB；`--with-updater` 与 `--ai` 互不干扰。

---

## 附录 E · 手工收口：finalize 与验证清单

把最终签名后的 NSIS/MSI 与对应 .sig 放在一个只含发行安装器的目录；不要混入裸 exe 或旧包。日常两指令已自动完成此步。以下仅供签名服务返回最终包后手工收口，输出完整待上传目录，**不会上传**；--out 必须不存在。

```bash
pnpm release:finalize '<日志中的 WSL 构建目录>/target/release/bundle' \
  --manifest dist/raybend-build.json --out release-out/所选版本 \
  --base-url https://github.com/C-Thun/raybend/releases/download/v所选版本/
# 明确无 Authenticode 签名时另加 --allow-unsigned
# 不发布更新时省去 --base-url；不会生成更新 JSON
```

输出安装器及 .sig 的字节副本、构建 manifest、发布说明、SHA256SUMS、release-index.json 和可选 latest.json/beta.json；有更新清单时每个安装器必须存在 .sig。signtool /pa /all 验证失败会停止；不把校验和当成发布者签名。发布索引保留来源/脏树/签名状态。installer 已有许可证与声明资源，public/legal 清单嵌进前端；发布前自动重生成。

上传前在全新 Windows 中核对有效签名及时间戳，确认 installer 的产品版本/通道与构建 manifest 一致。Signtool 验证并不证明 M4 功能、安装恢复或 SmartScreen 信誉已通过。

完成全部验收后由您执行第二条 release:publish --execute；它提交精确版本文件、tag/push、上传最终包/.sig/清单，并等待正式 Release 触发官网更新。没有发行包时保持未发布状态。第三方签名服务改变包字节后须重新 finalize，不能沿用旧 SHA256SUMS。

---

## 附录 F · 打包细节：安装器、源码镜像、路径与锁

### 安装器类型与构建目录

日常以 `--win-msi` 为主；`--win-nsis` 生成 NSIS 的 `-setup.exe`；两开关同时指定时只编译一次并打两种安装器。旧 `--windows` 已移除，会提示新参数。Beta 及带预发布标签的 test 只能用 `--win-nsis`，脚本不会静默改目标。

```bash
# 首次验证，不升版；产物进 release-out/test-时间/
pnpm release test --win-msi --unsigned
# 同时准备两种安装器
pnpm release patch --win-msi --win-nsis --unsigned
# 只读核对计划和真实路径映射
pnpm release test --win-msi --unsigned --dry-run
# 可选：指定其他本地盘（Windows 路径或实际 WSL 挂载路径均可）
pnpm release test --win-msi --unsigned --win-dir 'D:\raybend-build'
pnpm release test --win-msi --unsigned --win-dir '/windows/d/raybend-build'
```

最后两条是**同一个目录的两种写法示例**，应使用自己机器实际的盘和挂载点。默认不需要填路径，也不假定 C 盘、`/mnt/c`、用户名或 WSL 发行版。`--win-dir` 优先于环境变量 `RAYBEND_WIN_BUILD_DIR`；没有覆盖时读取 Windows 的 LocalApplicationData 已知目录。`wslpath` 负责正反转换，脚本检查盘可用且为本地盘、挂载点存在、回译一致，打印 Windows/WSL 两侧路径；未挂载盘、网络盘、Linux/UNC 构建目录、`D:relative` 直接拒绝。

本地镜像只含 Cargo 清单、Rust 工具链文件、`crates/`、`src-tauri/`、`src/`（Rust 编译引用 DTO）、`public/`、许可与本轮前端，含工作区实际未提交文件；不带 Git、node_modules、target、官网或历史安装器。未变化文件保留时间戳，已删除输入从镜像同步移除。专用镜像必须有脚本归属标记，拒绝覆盖同名的其他目录；不使用符号链接/junction 指回 WSL。首次使用新构建目录会冷编译，随后复用 `target/`，不会迁移或删除旧的 `C:\rb-target` 产物。

工作区 `.release/build.lock` 和镜像同级 `source.lock` 阻止并发发行。正常成功/失败均释放；强制终止后若报残留锁，确认 WSL 与 Windows 的构建进程均已结束，再手动删除报错指出的锁。每次打包清空本构建目录的 `target/release/bundle` 暂存后重新生成，最终 `release-out/` 保留，不能覆盖已有版本目录。

路径统一转换也覆盖文件型 `TAURI_SIGNING_PRIVATE_KEY` 和自定义 `RAYBEND_DAV1D_WIN_DIR`（接受 Windows 路径或 WSL 挂载路径；内联签名私钥保持原值）。dav1d 仍须按 `scripts/build-dav1d-win.cmd` 预先安装，默认依赖位置沿用原模块。Windows 批处理在本地镜像目录启动，配置与 `CARGO_TARGET_DIR` 使用盘符绝对路径；WSLENV 不再对这些已转换路径重复应用 `/p`。

**验证边界**：2026-09-29 已通过脚本单测、真实路径探测和 dry-run；尚未用新版流水线生成 MSI，也未验证安装、升级、卸载或 NSIS → MSI 切换。已有 NSIS 安装是否能平滑转 MSI 必须真机验收，不能把固定 MSI upgradeCode 当作跨安装器升级保证。MSI 由 Windows WiX 生成的限制见 [Tauri 官方说明](https://v2.tauri.app/distribute/windows-installer/)；如 Windows 缺 VBScript 可选功能，按该文档配置，不将 `light.exe` 失败一律归咎于路径。

> **验证边界**（更新于 2026-10-15）：本附录描述的这一套已在 v0.1.2 实跑（2026-09-29 的「尚未用新版流水线生成 MSI」已作废）；
> 跨安装器升级（NSIS → MSI）与真机安装/卸载验收仍归人类，见附录 G。

### 版本与计划的核对（dry-run）

```bash
pnpm release patch --dry-run
pnpm release minor --channel beta --dry-run
pnpm release 0.2.0 --win-msi --unsigned --dry-run
pnpm release test --win-msi --unsigned --dry-run
```

package.json 为产品版本来源，Cargo.toml 的 workspace 版本与 Cargo.lock 里**全部 workspace 成员**（当前：raybend / raybend-desktop / raybend-crashpad —— 由 `[workspace] members` 动态解析，新增成员自动跟上）同步，Tauri 直接读取 package.json。test 不升版，前端落 dist/test-build；beta/release 落 dist。`pnpm release` 自身不 commit/tag/push/上传；只有 `--publish`（或第二条 `release:publish --execute`）才会做这些外部动作。指定版本号时必须严格大于当前版本，降级会被拒绝；beta 通道的目标版本必须自带预发布标签（如 `0.2.0-beta.1`），test 不接受指定版本号。manifest 的 gitHash 记录升版前已提交代码的 commit，版本改动随后由发布步骤提交；它不是最终版本提交的 hash。源代码 tag 必须保留该次升版与生成许可资源。工作树非干净时，beta/release 默认阻断；--allow-dirty 是明确接受脏树来源的选项，不建议用于公开版，而且 `--publish` 会因为发布提交里混入了版本文件以外的改动而失败。

注意：正式/预览准备成功会修改版本文件；构建失败恢复本次版本写入。发生并行修改时保留外部编辑并报冲突。失败生成的前端不是可发布文件，修复后重跑，不要沿用旧 dist 或旧 SHA。

### Windows 打包（WSL 入口）

Windows 本机需要 Rust/MSVC、Windows SDK signtool（签名路线需要，加入 Windows PATH），以及 cargo tauri CLI。前端仍在 WSL 编译。先在 Windows 安装与根 @tauri-apps/cli 一致的 tauri-cli 版本（见 §2.1）；初次机器配置按 §2.1 构建 dav1d 静态库。

```bash
# 无 Windows 发布者签名的自测包：
pnpm release test --win-msi --unsigned

# 有 Windows 证书存储证书的包（指纹为公开标识，不是私钥）：
export RAYBEND_SIGN_CERT_SHA1='<Windows 证书存储的 40 位 SHA-1 指纹>'
pnpm release patch --win-msi

# 明确选择无发布者签名的公开包：
pnpm release patch --win-msi --unsigned
```

发布目录自动从 Windows 获取（默认 `%LOCALAPPDATA%\raybend\build`），`source/` 是本地构建镜像，`target/` 是持久编译缓存。前端在 WSL 只构建一次，再复制必要输入，由 Windows cargo 构建核心 worker 与桌面应用、Tauri bundle。安装包采用同进程自重启 RAW worker，不附带容易过期的另一份 worker；check:win 在自重启模式检查主程序内协议标签。NSIS 每用户安装且支持英/简中；MSI 使用固定 upgradeCode，禁用降级。beta 仅 NSIS（MSI 数值版本不能可靠区分 beta.N）。未添加任何删除用户照片/库的卸载钩子，但仍需要实际验证安装器行为。

证书存储签名是已准备的一条路线。SignPath 等远程服务需要其签名流程适配，不能把 Foundation 私钥导入本机；暂不伪造 CI 工作流和服务账户。
### 依据

GitHub 草稿/资产与 `--verify-tag` 参数依据：https://cli.github.com/manual/gh_release_create 。
Release 触发 Pages 与令牌触发边界依据：https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release 、https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow 。
v0.1.2 的实跑记录：`implementations/2026-10-15_first-release-and-fixes.md`。

---

## 附录 G · 真机验收与失败恢复

在干净 Win10 22H2 / Win11（至少一台干净配置，可用 VM）依次验：初次安装/首启建库与已有库、中文/Unicode/只读路径、浏览编辑保存及真实导出、自重启 worker、vN→vN+1 更新、下载断网、签名不匹配、安装失败/磁盘不足、导入或导出进行中尝试更新、卸载/重装保留照片与库。
下载失败不启动安装；安装阶段系统行为需实测，不宣传“失败一定自动回滚”。更新后还没启动时可恢复安装；一旦数据库升级，旧程序会触发 SchemaTooNew 拒绝打开。迁移前快照用于按版本恢复，不能直接覆盖升级后新增照片/编辑。恢复需先复制当前库，确认要恢复的时间点，保留差异，不静默删库重建。

未通过这些测试就不能勾选 M5 DoD。GUI/色彩/大库性能/E2E 由您确认；Agent 的编译和单测只属于冒烟。

---

## 附录 H · macOS / Linux 的签名要求（远期参考）

三平台的结构完全不同。**成本必须按「平台 × 分发路径」算，不能按平台一刀切**——按平台排序会得出「Windows 最贵」的错误结论，它只在「直下 + 自签」这一条路径上成立。

| 平台 × 路径 | 现金成本 | 代价 |
| --- | --- | --- |
| Linux 直下（AppImage/tar.gz） | $0 | — |
| Windows 直下 unsigned | $0 | 每版流失约 22–38% 安装（加预安装说明可压到约 22%） |
| Windows 直下 + 自签（IV/OV） | $250–400/年 | 硬件 Token 或云订阅；签完仍需攒信誉 |
| **Windows MSIX → 微软商店** | **$0**（开发者账号也已免费） | 需补 MSIX 包装与双通道构建；商店接管更新 |
| macOS 任何路径 | **$99/年** | 唯一入口；直下另需公证，App Store 另加强制沙盒 |

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| 未签名直下体验 | SmartScreen 提示，用户“更多信息 → 仍要运行”（两下） | **分三档**：已公证 → 无提示直开；有 Developer ID 签名未公证 → 阻止后去系统设置点“仍要打开”（Sequoia 15 起不能右键绕过，Apple 文档劝阻）；**无有效 Developer ID 签名（完全未签名、ad-hoc 签名均算）→ 报「已损坏，移到废纸篓」**，解除需终端命令，普通用户不可安装 | 无警告（AppImage/tar.gz）；deb/rpm 由发行版仓库机制负责 |
| 签名颁发者 | 多家 CA（DigiCert/Sectigo/GlobalSign/SSL.com/Certum…），信任库由微软 Trusted Root Program 决定 | **只有 Apple**（Developer ID），无第三方入口 | 开发者自持 GPG 密钥，无 CA |
| 成本与硬件 | 直下自签 $250–400/年（CSBR 强制私钥进硬件，Token 或云签名）；**MSIX 商店路径 $0** | $99/年 Apple Developer Program（唯一入口，含其他服务） | $0 |
| 公证/沙箱 | 无强制公证 | **强制公证**（2019-06 后构建的 Developer ID 软件）；App Store 另强制 App Sandbox | 无 |
| 商店路线 | MSIX 认证后**微软重签，$0**，开发者账号也已免费 | App Store 须**先有 $99 会员并自签**（Mac App Distribution + Installer 证书），Apple 再重签 | Flathub/发行版仓库各自审核 |

要点：

- **macOS 的 unsigned 直下不是「弹警告」而是「判损坏」**：Gatekeeper 对**没有有效 Developer ID 签名**的下载应用（完全未签名、ad-hoc 签名均算）直接报「XXX.app 已损坏，无法打开，应移到废纸篓」（虚假报错；MarkText 是 ad-hoc 签名照样中招，LocalAI、OVO 等开源项目均有记录），唯一解除方法是终端执行 `xattr -r -d com.apple.quarantine`，普通用户不可能完成。「系统设置 → 仍要打开」的逃生门只在**有 Developer ID 签名但未公证**这一档存在（Sequoia 15 起右键绕过已移除，流程多步且 Apple 文档劝阻）。远期做 macOS 版时，$99/年 是**必选项**而非可选项——这一点比 Windows 更硬：Windows 至少有 unsigned 直下与 MSIX 商店两条 $0 路径，macOS 连 unsigned 直下都不可用；好处是入口唯一、公证全自动。
- **Apple 是签名权的唯一来源**：会员资格是“请求、下载和使用 Apple 颁发证书”的前提。Windows 至少是多 CA 竞争 + 微软守信任库；macOS 是单一厂商——三平台里最集中的结构。
- **App Store 不是“免签名通道”**（与 Windows Store 不同）：微软 Store 的 MSIX 可不买商业证书（认证后重签）；Apple 必须先有 $99 会员并自行签名上传。
- **沙箱是 macOS 的产品层门槛（比证书更重要）**：App Store 强制 App Sandbox，访问沙箱外目录需用户授权的 entitlement；照片管理软件若走 App Store，库目录、导入源、外部编辑器都要在授权模型内重新设计。直下分发（Developer ID + 公证）没有这条限制。
- **Linux 的信任模型就是“密钥=身份”**：deb/rpm 走仓库级 GPG（apt-secure 签 Release 文件），Flatpak 走 OSTree commit 的 GPG 签名（`flatpak build-sign`），AppImage/tar.gz 无需签名。没有 CA、没有身份验证、没有硬件要求，用户自行决定信任谁的密钥。
- **Sigstore 是同一思路的现实实现**：Fulcio 依 OIDC 身份签发约 10 分钟短期证书，Rekor 用 append-only 透明日志（Merkle 树）记录签名，免费、无 CA、无硬件。**落差点在平台侧：Windows/macOS 不查 Sigstore**，只认自家信任根——这类方案能解决“可审计”，不解决“被放行”。

来源：[Apple Developer ID](https://developer.apple.com/developer-id/)、[macOS 公证](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)、[Apple 证书与会员要求](https://developer.apple.com/support/certificates)、[macOS 15 绕过变更（Ars Technica）](https://arstechnica.com/gadgets/2024/08/macos-15-sequoia-makes-you-jump-through-more-hoops-to-disable-gatekeeper-app-checks/)、[Debian 包签名（apt-secure）](https://www.debian.org/doc/manuals/securing-debian-manual/deb-pack-sign.en.html)、[Flatpak build-sign](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)、[Sigstore 安全模型](https://docs.sigstore.dev/about/security/)。

---

## 附录 I · 为什么暂不做 CI 打包

**结论：技术上可行，但对现状不划算，暂缓；登记到 `memory/FUTURE.md` G24，触发条件是「模型公开分发方式定下来」或「有了云签名」。**

评估依据（已核对的代码与事实）：

- 现有流水线是 **WSL 前端 + Windows 原生 cargo** 的两端结构，`release.mjs` 里大量逻辑（镜像同步、`wslpath` 路径转换、本地盘校验、并发锁）是为这两台“机器”服务的；CI 里这些既用不上也不能照搬。
- 全部构建输入在 CI 上**基本都拿得到**：Rust 工具链由 `rust-toolchain.toml` 固定；MSVC/WiX 由 GitHub windows runner 提供；dav1d 1.5.0 能按 `scripts/build-dav1d-win.cmd` 的逻辑（meson+ninja+nasm）重建并用 actions cache；onnxruntime DLL 已有固定 URL + SHA-256 的自动下载。
- **真正的卡点是 AI 模型**：152 MB 的 ONNX 在独立仓库 `model-registry`（git LFS），不在本仓。CI 要出 AI 版，得先解决模型怎么公开分发（LFS 流量 / Release 资产 / 私有仓库令牌）；否则 CI 产物只能是不带 AI 的版本 —— 与本地主力包不同质，等于维护两套发布口径与两套校验。
- 其余成本：Secrets 管理（更新私钥、将来 Authenticode 云签名）、缓存键维护、`check:win` 要在 CI 里同套复跑；而且 CI 直接执行发布动作（提交/tag/上传）与“发布由人类执行”的纪律边界需要重新划定（可行形态是 CI 出草稿、人类点 publish，但又回到了人的动作）。

因此本轮先把**本地一键链路**（本文件）与**启动自动检查更新**做顺；CI 打包留到模型分发与签名方案落地后再启动。

---

## 附录 J · 磁盘占用与清理

构建缓存分布在四处，各自只由对应命令清；**发版流程本身不删任何缓存**（Windows `target/` 是刻意保留的：
下次构建不冷启，省十几分钟）。看总账：

```bash
pnpm clean:report
```

| 位置 | 实测 | 性质 | 清理 |
| --- | --- | --- | --- |
| WSL `target/` | **58 G** | 日常 `cargo check/test` 的缓存，只进不出 | `pnpm clean:wsl`（按构建图清失效产物）；`--keep-days 0` 深清，代价是 examples 重编 |
| Windows `…\raybend\build\target` | 4.6 G | 发版 / 调试构建缓存，**刻意保留** | `pnpm clean:win`（只清失效产物） |
| 旧 `C:\rb-target` | **17.9 G** | 2026-09-29 前的默认位置，已弃用 | `pnpm clean:win --target-dir 'C:\rb-target\raybend' --keep-days 0`（或直接删） |
| `release-out/` | 130 M | 已发布产物的字节副本，保留 | 旧 `test-*` 目录可手动删 |
| `.release/`（AI 输入快照） | 168 M | 模型 + CPU 运行库冻结副本，构建复用 | 保留 |
| pnpm store | 4.0 G | 全局包仓库（所有 pnpm 项目共享） | `pnpm store prune` |
| cargo registry | 1.4 G | crate 下载缓存 | 保留（删了要重新下载） |

口径：`clean:win` / `clean:wsl` 只清「不在当前构建图里、且已凉 ≥ N 天（默认 3）」的产物 ——
正在用的不动、不付冷构建代价；依赖升级 / feature 组合变化 / 换构建参数留下的旧产物由它回收。
`clean:win` 的默认目标与发版构建**同一套探测**（`RAYBEND_WIN_BUILD_DIR` / `--win-dir` 可覆盖），
旧目录还在时它会打印一行提示。
