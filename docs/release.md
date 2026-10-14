# RayBend 发布操作

本文的打包、签名、tag、push、上传与真机验收由您执行。M4 最新开发与冒烟已收口，M5 发行 DoD 仍需真机验收。本轮聚焦 NSIS/MSI → GitHub Release → 官网，MSIX 已登记 FUTURE，后续独立启动。

## 日常发行：一条命令或两条指令

前提：先审阅并**自己提交**开发改动（脚本只提交版本文件与许可清单，其余改动一律阻断 —— 见 §2）；不要把并行未完成的改动混入发行。执行者是崔总。

**一条命令（推荐，`--publish`）** —— 准备与发布一次做完：

```bash
# 自增 patch：0.1.1 → 0.1.2，构建 MSI 并直接发布（提交/tag/推送/上传/公开/等官网）
pnpm release patch --win-msi --unsigned --publish

# 指定版本号：位置参数或 --version 均可，接受 v 前缀与预发布标签
pnpm release 0.2.0 --win-msi --unsigned --publish
pnpm release --version v0.2.0 --win-msi --unsigned --publish

# 正式版跳过 crates.io 同步（默认跟着发）
pnpm release patch --win-msi --unsigned --publish --no-crates
```

`--publish` 的硬规则：必须同时指定 `--win-msi` / `--win-nsis`；不能与 `--dry-run`、`--skip-build` 或 `test` 通道同用；发布动作与下面第二条指令**完全一致**（同一个脚本函数，不是第二条通路）。

**两条指令（需要验收窗口时）** —— 与上面等价，只是中间停下来让您验收安装器、核对/编辑 `release-out/v版本/RELEASE-NOTES.md`：

```bash
pnpm release patch --win-msi --unsigned
# 在 Windows 验收本版安装器，并核对/编辑 release-out/v0.1.2/RELEASE-NOTES.md 后：
pnpm release:publish release-out/v0.1.2 --execute
```

版本决定两种方式：`patch` / `minor` / `major` 自增（含 beta 线递增规则），或直接指定 `x.y.z`（必须**严格大于**当前版本；beta 通道的目标版本要自带预发布标签）。指定版本号是给“跳级、补齐、早就定好的版本”用的，不是绕过降级保护的后门。

第一条同步版本、重生成许可、构建一次前端、构建 Windows 主程序/worker、自动镜像到 Windows 本地盘并打所选的 MSI、核对嵌入前端和 worker 协议、验证签名（或显式 unsigned）、生成完整 `release-out/v版本/`。输出目录含安装器、可选 .sig、SHA256SUMS、release-index.json、raybend-build.json、可选 latest.json 和发布说明；只收本次版本，缺目标安装器直接失败，旧包不混入。输出目录必须全新，不覆盖旧产物或并行文件。第一条成功后不再另跑 finalize。官网优先选择同版 MSI，没有 MSI 时选择 NSIS。test 为单独带时间的输出目录且不能公开上传；beta 仅 NSIS。

第二条核对安装器哈希、更新签名/地址、版本事务、许可锁摘要和构建时源码快照；只自动提交 package.json / Cargo.toml / Cargo.lock / public/legal/third-party.json，创建本版 tag，原子推送 master + 本版 tag，然后创建草稿 Release、上传全部资产、核对远程 SHA-256，最后公开 Release。正式版本发布事件触发既有官网 Actions：从完整事件资产注入版本和安装包直链、测试/构建静态站、发布 Pages；脚本等待该工作流成功并输出发布页和下载地址。没有修改官网源码或另做一次 website commit。GitHub 的对应源码归档来自该 tag，完整构建步骤保留在同一 tag 中。

`release:publish` **默认只读预览**，只有显式 `--execute` 才联网并执行提交/tag/push/上传。发现其他修改、未跟踪文件、暂存区非空、错误 origin、错误分支或来源不一致时阻断，保留现场。只支持本仓 master → C-Thun/raybend，不自动切分支/合并/强推，不使用 --follow-tags 推送其他 tag。

网络中断时保留草稿和已上传资产，修复后重跑同一条第二指令：已上传资产须与本地字节一致，只补缺项；冲突不覆盖。草稿中断上传留下的 starter 零字节占位可由重跑指令删除重传，已完成资产不删除。公开后 Pages 失败会报错，Release 已公开的事实不会回滚，排障后重跑不会重传安装器。第二条重跑会对匹配的已失败官网工作流自动重跑一次并等待；仍失败按日志修复，不反复重传或循环重跑。尚无工作流时检查 Actions/Pages 配置和发布令牌，不把上传成功冒充网站成功。

### 一次性准备（不属于每版重复操作）

- 需要 Windows Rust/MSVC、dav1d 静态库，以及和 WSL 前端 CLI 匹配的 Windows cargo-tauri。**2026-09-29 本机只读核查 cargo-tauri 2.11.4 已就绪**。换机器须自行配置这些工具，WSL 的 Windows 互操作须启用，且能调用 `powershell.exe`、`cmd.exe` 与 `wslpath`；Windows 不需要 Node/pnpm。第一条指令缺工具时会在改版本之前提示；首次可多执行这条（一次配置后每版仍只用上面两条）：

  ```bash
  cmd.exe /d /c "cargo install tauri-cli --version 2.11.4 --locked"
  ```

- **同日 WSL 未找到 GitHub CLI `gh`**。在 WSL 安装并登录官方 GitHub CLI https://cli.github.com/ ，确认账号可向本仓推送并管理 Release/读取 Actions。SSH origin 的 Git 认证和 GitHub API 登录是两件事。脚本不收集或存储密码/私钥；不要把密钥写进仓库。使用能触发后续工作流的个人登录令牌；不要用仓库 GITHUB_TOKEN 代替个人发行登录后假定它会触发新的 Actions。
- 仓库 Pages 来源选 GitHub Actions，自定义域名/HTTPS 按 website/AGENTS.md §5 配置，开启 Actions。官网已有 .github/workflows/website.yml，不需要新增网站发布服务器。
- 如需签名更新，先自己生成/备份私钥并设置 §4 的公私钥环境变量；第一条加 `--with-updater`，第二条不变。没有该参数时新构建默认关闭内置更新，手动下载升级可先发行。有 Authenticode 证书时配置指纹并去掉 `--unsigned`；二者与发行指令数量无关。

GitHub 草稿/资产与 --verify-tag 参数依据：https://cli.github.com/manual/gh_release_create 。Release 触发 Pages 与令牌触发边界依据：https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release 、https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow 。脚本已用模拟后端验证；真实安装器/账户/线上链路仍需首次执行验收。

## Windows 安装器与目录选择

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

## 1. 不花钱也能开始

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

## 2. 先核对计划

```bash
pnpm release patch --dry-run
pnpm release minor --channel beta --dry-run
pnpm release 0.2.0 --win-msi --unsigned --dry-run
pnpm release test --win-msi --unsigned --dry-run
```

package.json 为产品版本来源，Cargo.toml 与 Cargo.lock 的两个本地包同步，Tauri 直接读取 package.json。test 不升版，前端落 dist/test-build；beta/release 落 dist。`pnpm release` 自身不 commit/tag/push/上传；只有 `--publish`（或第二条 `release:publish --execute`）才会做这些外部动作。指定版本号时必须严格大于当前版本，降级会被拒绝；beta 通道的目标版本必须自带预发布标签（如 `0.2.0-beta.1`），test 不接受指定版本号。manifest 的 gitHash 记录升版前已提交代码的 commit，版本改动随后由发布步骤提交；它不是最终版本提交的 hash。源代码 tag 必须保留该次升版与生成许可资源。工作树非干净时，beta/release 默认阻断；--allow-dirty 是明确接受脏树来源的选项，不建议用于公开版，而且 `--publish` 会因为发布提交里混入了版本文件以外的改动而失败。

注意：正式/预览准备成功会修改版本文件；构建失败恢复本次版本写入。发生并行修改时保留外部编辑并报冲突。失败生成的前端不是可发布文件，修复后重跑，不要沿用旧 dist 或旧 SHA。

## 3. Windows 打包（WSL 入口）

Windows 本机需要 Rust/MSVC、Windows SDK signtool（签名路线需要，加入 Windows PATH），以及 cargo tauri CLI。前端仍在 WSL 编译。先在 Windows 安装与根 @tauri-apps/cli 一致的 tauri-cli 版本（见上方一次性准备）；初次机器配置按 AGENTS.md §5.3 构建 dav1d 静态库。

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

## 4. 更新密钥与发布源

更新密钥免费自持，独立于 Authenticode。请自己生成并备份加密私钥和密码，不要放进仓库、对话、日志或自动化脚本。

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

**启动后静默检查一次更新**（默认开，可在更新对话框里关；测试/开发通道默认不自动检查，避免调试环境被拉到正式版），发现新版本只给出提示，下载与安装仍由用户确认并验证签名。没有构建公钥时默认关闭，不发无效网络请求。官方稳定源为 GitHub 最新正式 release 的 latest.json；预览源为官网 /updates/beta.json，发布前必须实际部署该文件。源码可替换源/公钥或关闭更新，仍保留签名校验；稳定源拒绝预发布版本。NSIS 和 MSI 使用各自 windows-x86_64-nsis/msi 更新目标，未知/裸 exe 不跨用另一种安装器。启动检查会联网 —— 隐私说明与 `updates.privacy` 文案必须与之一致（已同步）。

## 5. 验证与生成清单

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

## 6. MSIX 商店路线（本轮后移）

崔总 2026-09-27 明确：登记 FUTURE 后续独立启动，本轮先完成直下发行链。正式落点为 memory/FUTURE.md 的 Windows MSIX 分发条目；没有把它列成本轮发布的前置。同日下午拍板（memory/REVIEW.md R3-01）：**官网直下走通后即启动 MSIX**，定位是「零现金成本解决 Windows 安装体验」的主攻方向——它与直下是并行的两条分发通道，不是证书的替代品；发布顺序见 memory/REVIEW.md §8。

仍需您开户、预留产品名并取得 Partner Center 的 Package Identity Name/Publisher；目前没有这些实际身份，不能提交有效 MSIX。Tauri 官方当前教程覆盖 EXE/MSI，MSIX 需要另用 Windows SDK MakeAppx/打包工具包装，不能复用 NSIS 直接上传冒充 MSIX。

**商店签名与费用（2026-09-27 再次确认，比预期更优）**：微软官方原文——“MSIX/AppX 包不必用受信任 CA 的证书签名；认证通过后商店会自动用微软证书重签”，即**无需购买任何商业证书**。开发者账号费用已归零：个人 $19 于 2025-09 豁免（身份证+自拍验证，近 200 市场），公司 $99 于 2026-05 豁免。两个边界：**只有 MSIX/AppX 免签**，用 MSI/EXE 走商店仍需自己签；Tauri 原生只出 NSIS(.exe)/MSI，所以吃到这条红利的前提是先做 MSIX 包装（即本节描述的额外工程），不是现有产物换个后缀。

包装前先准备编译时 RAYBEND_DISTRIBUTION=store 的构建（前端与 Rust 两端一致），内置更新由编译身份禁用。现有 CLI 直下包固定 direct；商店构建和包装流水线在拿到身份后另接，不能拿 direct 包换个后缀上架。

验收须覆盖 MSIX 数据位置与直下版迁移、卸载数据保留、自重启 RAW worker、导入目标/导出目标权限、WebView2、外部编辑器启动和未来更新方式。商店版本交由商店更新，不能同时用内置 NSIS updater 改写程序。产品中尚不显示“商店认证”或虚假商店链接。

## 7. 真机验收与失败恢复

在干净 Win10 22H2 / Win11（至少一台干净配置，可用 VM）依次验：初次安装/首启建库与已有库、中文/Unicode/只读路径、浏览编辑保存及真实导出、自重启 worker、vN→vN+1 更新、下载断网、签名不匹配、安装失败/磁盘不足、导入或导出进行中尝试更新、卸载/重装保留照片与库。
下载失败不启动安装；安装阶段系统行为需实测，不宣传“失败一定自动回滚”。更新后还没启动时可恢复安装；一旦数据库升级，旧程序会触发 SchemaTooNew 拒绝打开。迁移前快照用于按版本恢复，不能直接覆盖升级后新增照片/编辑。恢复需先复制当前库，确认要恢复的时间点，保留差异，不静默删库重建。

未通过这些测试就不能勾选 M5 DoD。GUI/色彩/大库性能/E2E 由您确认；Agent 的编译和单测只属于冒烟。

## 8. macOS / Linux 的签名要求（远期参考，2026-09-27 核实）

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

## 9. crates.io 核心库同步发布（已接入发版流程）

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

## 10. AI 外挂编译（`--ai`）：默认零配置，找不到就出基础版

AI 能力是**编译期外挂**：模型与 CPU 推理库不进本仓 Git，构建时按校验过的字节装进安装包。
功能规格见 `specs/ai-model-library-build.md` 与 `memory/FUNCTION-IMAGING.md`；这里只讲发布侧怎么用。

### 输入从哪来

| 输入 | 位置 | 获取方式 |
| --- | --- | --- |
| 模型（`image_encoder.onnx` 152 MB + 词表/许可） | **独立的 `model-registry` 仓库**（git LFS，云上另存） | `pnpm ai:use` 显式登记，或构建时自动探测同级 `../model-registry` |
| CPU 推理库（onnxruntime 1.28.0 DLL，约 16 MB） | `crates/raybend/assets/ai/ort-win-x64/`（已 gitignore） | `pnpm ai:prepare`：从 onnxruntime 官方 Release 固定 URL 下载 + SHA-256 校验后解出 |

模型绑定的是 `profiles/raybend-photo-tags/v1`（消费侧契约）：`manifest.json` 的摘要必须与
`crates/raybend/assets/ai/tinyclip-v1/manifest.json` 一致，否则拒绝使用 —— 不是“换个模型就上”。

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

## 11. 为什么暂不做 CI 打包（2026-10-14 评估）

**结论：技术上可行，但对现状不划算，暂缓；登记到 `memory/FUTURE.md` G24，触发条件是「模型公开分发方式定下来」或「有了云签名」。**

评估依据（已核对的代码与事实）：

- 现有流水线是 **WSL 前端 + Windows 原生 cargo** 的两端结构，`release.mjs` 里大量逻辑（镜像同步、`wslpath` 路径转换、本地盘校验、并发锁）是为这两台“机器”服务的；CI 里这些既用不上也不能照搬。
- 全部构建输入在 CI 上**基本都拿得到**：Rust 工具链由 `rust-toolchain.toml` 固定；MSVC/WiX 由 GitHub windows runner 提供；dav1d 1.5.0 能按 `scripts/build-dav1d-win.cmd` 的逻辑（meson+ninja+nasm）重建并用 actions cache；onnxruntime DLL 已有固定 URL + SHA-256 的自动下载。
- **真正的卡点是 AI 模型**：152 MB 的 ONNX 在独立仓库 `model-registry`（git LFS），不在本仓。CI 要出 AI 版，得先解决模型怎么公开分发（LFS 流量 / Release 资产 / 私有仓库令牌）；否则 CI 产物只能是不带 AI 的版本 —— 与本地主力包不同质，等于维护两套发布口径与两套校验。
- 其余成本：Secrets 管理（更新私钥、将来 Authenticode 云签名）、缓存键维护、`check:win` 要在 CI 里同套复跑；而且 CI 直接执行发布动作（提交/tag/上传）与“发布由人类执行”的纪律边界需要重新划定（可行形态是 CI 出草稿、人类点 publish，但又回到了人的动作）。

因此本轮先把**本地一键链路**（本文件）与**启动自动检查更新**做顺；CI 打包留到模型分发与签名方案落地后再启动。
