完成时间：2026-09-29 21:35:10 +0800

# WSL 一条 pnpm 命令准备 MSI / NSIS

## 改动范围与依据

承接 2026-09-27 MSI/WiX 失败记录与 FUTURE G22。此前实测：同样的 WiX 输入放到 Windows 本地盘能生成 MSI，WSL 映射盘 / UNC 资源则失败。因此保留 WSL 发起入口，由脚本同步构建输入后调用 Windows 工具链，可避免手工复制。Tauri 官方仍要求 MSI 在 Windows 生成：https://v2.tauri.app/distribute/windows-installer/ （2026-09-29 核对）。

崔总本轮追加：脚本不能只适合当前机器，需识别真实 Windows 挂载与传给 Windows 的盘符路径。最终实现不写死 C 盘、`/mnt/c`、用户名或发行版。

## 涉及文件与决策

- `src/lib/release-plan.ts` 及单测：`--windows` 替换为 `--win-msi` / `--win-nsis`，可组合；旧参数报迁移提示。`--win-dir` 接受 Windows 或 WSL 本地盘目录，优先于 `RAYBEND_WIN_BUILD_DIR`。Beta / 带预发布标签的 test 明确拒绝 MSI，沿用数值版本上限、签名和脏树纪律。
- `scripts/lib/windows-paths.mjs` 及单测：通过 Windows 已知目录（默认 LocalApplicationData）选择 `raybend/build`，PowerShell 查询真实 DriveInfo；用 `wslpath -u/-w` 与挂载目录检查确认路径双向一致。支持非 C 盘、自定义挂载前缀、中文、空格、单引号、尾斜线；拒绝网络/离线/未挂载盘、UNC/Linux 构建目录及 `D:relative`。PowerShell 用 UTF-16 EncodedCommand / UTF-8 输出，不在 shell 拼用户路径；批处理用本地 cwd + 固定相对文件名启动。
- `scripts/lib/release-mirror.mjs` 及单测：固定镜像内复制 Cargo 清单、工具链、crates、src-tauri、src（含 Rust 引用的 DTO）、public、许可和本轮前端，可选 `.cargo`；排除 Git、node_modules、target、网站等。读取实际工作区，保留未提交输入；按字节同步，未变化文件保留 mtime，移除过期输入，拒绝无归属目录、符号链接、Windows 重名/保留名和过长路径。使用 Node 内置文件 API，无额外 rsync 或 Windows Node 依赖。
- `scripts/release.mjs` / `scripts/lib/release-windows.mjs` / `scripts/lib/release-run.test.mjs`：沿用原版本事务、一次前端构建、worker 协议清单、签名及核查，加入工作区锁与镜像锁；构建从 Windows 本地 source 目录执行，target 缓存持久复用。`custom-protocol` 传给 build 与 bundle，bundle `-vv` 留出 WiX 错误原文。旧 bundle 暂存先清空，失败回滚版本并释放锁，最终 `release-out` 保留且拒绝覆盖。
- `scripts/lib/dav1d-win.mjs` 及单测：原模块一般化接收环境，复用 dav1d 变量实现；Windows 目标、文件型签名私钥、自定义 dav1d 目录统一转换，WSLENV 去重并移除已转换路径的旧 `/p` 标志，防止二次翻译。内联私钥保持原值，不写入镜像。
- `scripts/finalize-release.mjs` / `scripts/lib/release-upload.test.mjs`：只收明确选择的安装器，防止同版本另一类型及旧签名混入；签名检查复用统一路径转换。
- `website/src/data/release.ts` 及单测：同版本同平台 MSI 优先，NSIS 回退，仍校验正式版本、下载域与文件身份。无页面布局改动，无需新 Pencil 画稿。
- 更新 `docs/release.md`、根与官网 `AGENTS.md`、`specs/release-windows-msi.md`、`specs/website-homepage.md` 及 `memory/{ARCHITECTURE,FUTURE,PLAN,FINISHED}.md`，明确新入口、目录选择、冷编译成本和验收边界。

复用评估：已有 release 计划/版本事务/Windows 命令/环境/finalize 均直接扩展。仓内没有通用 WSL 路径探测与本地构建镜像实现，故仅新增这两项独立职责，不另造第二套发行流水线。属于开发发布脚本，**不接入应用命令注册表，无应用热键**。

## 已验证（冒烟）

- Windows `cargo tauri --version`：`tauri-cli 2.11.4`，与仓内 CLI 一致；`cargo tauri bundle --help` 确认 `-vv`、`--features`、`--config` 与 MSI/NSIS 支持。
- `pnpm test:release`：65 项通过；覆盖目标/通道、边界版本、目录解析、非 C 盘/自定义挂载、未挂载/网络盘、Unicode/空格/单引号、镜像更新/删除、超长/保留/冲突路径、符号链接、锁竞争、版本回滚、Windows 环境与签名文件路径、安装器筛选。
- `pnpm typecheck`、`pnpm test`：通过，应用 1122 项测试；`pnpm lint:colors` / `lint:arch` / `lint:i18n` 全通过；`pnpm build` 成功（现有 >500 kB 分块提示仍在）。
- 官网 `pnpm --dir website test:run`：24 项通过；typecheck / lint / build 成功。lint 仍有 `Hero.tsx:90` 既有 `solid(prefer-structured-class)` warning，本轮未修改该组件。
- 真实只读 `pnpm release test --win-msi --unsigned --dry-run` 与 `patch --win-nsis --win-msi --unsigned --dry-run` 通过；后者如实提示脏树阻断。默认探测到 Windows `C:\Users\andar\AppData\Local\raybend\build` ↔ WSL `/mnt/c/Users/andar/AppData/Local/raybend/build`。
- 自定义 `--win-dir 'C:\rb-target\MSI 中文 space'`（MSI）和 `--win-dir '/mnt/c/rb-target/MSI 中文 space'`（NSIS）真实 dry-run 解析到同一目录，未创建该目录。PowerShell 首次探测的 CLIXML 进度噪声通过设置 ProgressPreference 消除。
- `git diff --check` 通过。package.json、pnpm-lock.yaml、Cargo.toml、Cargo.lock 无变化；没有升版、tag、push、上传或生成安装器。

测试中的 Windows 执行/打包后端为 mock，合成文件仅用于验证流程与筛选，不能算真实 MSI 生成证据。未改 Rust，因此没有额外运行 Cargo 编译/单测。原工作区已有浏览/缩略图变更，本轮未编辑那些文件。

## 问题、绕法与风险

- 首次 pnpm 在受限沙箱内无法联网验证包管理器签名；通过工具授权后执行相同命令成功，未修改 packageManager、锁文件或校验策略。
- 未提供 pi 的 todo 工具；没有伪造工具进度或另建进度表，规格只保留验收项。
- 首次使用新 Windows 构建目录会冷编译；不移动/删除旧 `C:\rb-target` 的缓存或已有发布目录。后续用同一路径复用缓存。dav1d/MSVC/Rust/Tauri 等仍是一次性本机前置，不自动安装；自定义 dav1d 路径支持统一转换，原默认依赖位置保持兼容。

## 未经崔总验证

依据项目 AGENTS.md §2.1「生成/上传发布物……全部由人操作」，本轮没有运行实际发行打包。请在 WSL 执行 `pnpm release test --win-msi --unsigned` 首次生成测试 MSI；test 不升版本，输出 `release-out/test-时间/`。实际 MSI 生成、安装/升级/卸载、权限、照片数据保留、GUI 与 NSIS → MSI 切换仍须真机验收；不能将固定 MSI upgradeCode 当成跨安装器迁移保证，M5 DoD 维持未完成。
