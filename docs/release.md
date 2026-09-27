# RayBend 发布操作

本文的打包、签名、tag、push、上传与真机验收由您执行。M4 最新开发与冒烟已收口，M5 发行 DoD 仍需真机验收。本轮聚焦 NSIS/MSI → GitHub Release → 官网，MSIX 已登记 FUTURE，后续独立启动。

## 日常发行只用两条指令

前提：先审阅并提交当前开发工作树；不要把并行未完成的改动混入发行。下面以当前 0.1.0 升到 0.1.1、暂不配置 Windows 发布者证书/内置更新为例。执行者是崔总：

```bash
pnpm release patch --windows --unsigned
# 在 Windows 验收本版安装器，并核对/编辑 release-out/v0.1.1/RELEASE-NOTES.md 后：
pnpm release:publish release-out/v0.1.1 --execute
```

第一条同步版本、重生成许可、构建一次前端、构建 Windows 主程序/worker、打全部稳定版 NSIS/MSI、核对嵌入前端和 worker 协议、验证签名（或显式 unsigned）、生成完整 `release-out/v版本/`。输出目录含安装器、可选 .sig、SHA256SUMS、release-index.json、raybend-build.json、可选 latest.json 和发布说明；只收本次版本，缺目标安装器直接失败，旧包不混入。输出目录必须全新，不覆盖旧产物或并行文件。第一条成功后不再另跑 finalize。test 为单独带时间的输出目录且不能公开上传；beta 仅 NSIS。

第二条核对安装器哈希、更新签名/地址、版本事务、许可锁摘要和构建时源码快照；只自动提交 package.json / Cargo.toml / Cargo.lock / public/legal/third-party.json，创建本版 tag，原子推送 master + 本版 tag，然后创建草稿 Release、上传全部资产、核对远程 SHA-256，最后公开 Release。正式版本发布事件触发既有官网 Actions：从完整事件资产注入版本和安装包直链、测试/构建静态站、发布 Pages；脚本等待该工作流成功并输出发布页和下载地址。没有修改官网源码或另做一次 website commit。GitHub 的对应源码归档来自该 tag，完整构建步骤保留在同一 tag 中。

`release:publish` **默认只读预览**，只有显式 `--execute` 才联网并执行提交/tag/push/上传。发现其他修改、未跟踪文件、暂存区非空、错误 origin、错误分支或来源不一致时阻断，保留现场。只支持本仓 master → C-Thun/raybend，不自动切分支/合并/强推，不使用 --follow-tags 推送其他 tag。

网络中断时保留草稿和已上传资产，修复后重跑同一条第二指令：已上传资产须与本地字节一致，只补缺项；冲突不覆盖。草稿中断上传留下的 starter 零字节占位可由重跑指令删除重传，已完成资产不删除。公开后 Pages 失败会报错，Release 已公开的事实不会回滚，排障后重跑不会重传安装器。第二条重跑会对匹配的已失败官网工作流自动重跑一次并等待；仍失败按日志修复，不反复重传或循环重跑。尚无工作流时检查 Actions/Pages 配置和发布令牌，不把上传成功冒充网站成功。

### 一次性准备（不属于每版重复操作）

- 当前 Windows Rust/MSVC 与 dav1d 静态库已存在；**2026-09-27 核查未找到 Windows cargo-tauri**。WSL 前端 CLI 为 2.11.4，Windows 需匹配。第一条指令缺工具时会在改版本之前提示；首次可多执行这条（一次配置后每版仍只用上面两条）：

  ```bash
  cmd.exe /d /c "cargo install tauri-cli --version 2.11.4 --locked"
  ```

- **同日 WSL 未找到 GitHub CLI `gh`**。在 WSL 安装并登录官方 GitHub CLI https://cli.github.com/ ，确认账号可向本仓推送并管理 Release/读取 Actions。SSH origin 的 Git 认证和 GitHub API 登录是两件事。脚本不收集或存储密码/私钥；不要把密钥写进仓库。使用能触发后续工作流的个人登录令牌；不要用仓库 GITHUB_TOKEN 代替个人发行登录后假定它会触发新的 Actions。
- 仓库 Pages 来源选 GitHub Actions，自定义域名/HTTPS 按 website/AGENTS.md §5 配置，开启 Actions。官网已有 .github/workflows/website.yml，不需要新增网站发布服务器。
- 如需签名更新，先自己生成/备份私钥并设置 §4 的公私钥环境变量；第一条加 `--with-updater`，第二条不变。没有该参数时新构建默认关闭内置更新，手动下载升级可先发行。有 Authenticode 证书时配置指纹并去掉 `--unsigned`；二者与发行指令数量无关。

GitHub 草稿/资产与 --verify-tag 参数依据：https://cli.github.com/manual/gh_release_create 。Release 触发 Pages 与令牌触发边界依据：https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release 、https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow 。脚本已用模拟后端验证；真实安装器/账户/线上链路仍需首次执行验收。

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
pnpm release test --windows --unsigned --dry-run
```

package.json 为产品版本来源，Cargo.toml 与 Cargo.lock 的两个本地包同步，Tauri 直接读取 package.json。test 不升版，前端落 dist/test-build；beta/release 落 dist。脚本不 commit/tag/push/上传。manifest 的 gitHash 记录升版前已提交代码的 commit，版本改动随后由您提交；它不是最终版本提交的 hash。源代码 tag 必须保留该次升版与生成许可资源。工作树非干净时，beta/release 默认阻断；--allow-dirty 是明确接受脏树来源的选项，不建议用于公开版。

注意：正式/预览准备成功会修改版本文件；构建失败恢复本次版本写入。发生并行修改时保留外部编辑并报冲突。失败生成的前端不是可发布文件，修复后重跑，不要沿用旧 dist 或旧 SHA。

## 3. Windows 打包（WSL 入口）

Windows 本机需要 Rust/MSVC、Windows SDK signtool（签名路线需要，加入 Windows PATH），以及 cargo tauri CLI。前端仍在 WSL 编译。先在 Windows 安装与根 @tauri-apps/cli 一致的 tauri-cli 版本（见上方一次性准备）；初次机器配置按 AGENTS.md §5.3 构建 dav1d 静态库。

```bash
# 无 Windows 发布者签名的自测包：
pnpm release test --windows --unsigned

# 有 Windows 证书存储证书的包（指纹为公开标识，不是私钥）：
export RAYBEND_SIGN_CERT_SHA1='<Windows 证书存储的 40 位 SHA-1 指纹>'
pnpm release patch --windows

# 明确选择无发布者签名的公开包：
pnpm release patch --windows --unsigned
```

发布 target 使用 C:\rb-target\raybend-release，前端只构建一次，再由 Windows cargo 构建核心 worker 与桌面应用、Tauri bundle。安装包采用同进程自重启 RAW worker，不附带容易过期的另一份 worker；check:win 在自重启模式检查主程序内协议标签。NSIS 每用户安装且支持英/简中；MSI 使用固定 upgradeCode，禁用降级。beta 仅 NSIS（MSI 数值版本不能可靠区分 beta.N）。未添加任何删除用户照片/库的卸载钩子，但仍需要实际验证安装器行为。

证书存储签名是已准备的一条路线。SignPath 等远程服务需要其签名流程适配，不能把 Foundation 私钥导入本机；暂不伪造 CI 工作流和服务账户。

## 4. 更新密钥与发布源

更新密钥免费自持，独立于 Authenticode。请自己生成并备份加密私钥和密码，不要放进仓库、对话、日志或自动化脚本。

```bash
pnpm tauri signer generate -w /您自己选择的仓外安全目录/raybend-updater.key
```

以安全环境变量/密钥管理提供 RAYBEND_UPDATER_PUBLIC_KEY、TAURI_SIGNING_PRIVATE_KEY（Tauri 支持私钥内容或绝对路径）与 TAURI_SIGNING_PRIVATE_KEY_PASSWORD；本机存在的私钥文件路径会经 wslpath 转成 Windows 可访问路径，内容型密钥不转换；密钥不会打印到计划中。公开公钥可进发布配置。私钥不能由 Agent 代您保管。

```bash
pnpm release patch --windows --with-updater --unsigned
# 有发布者证书时去掉 --unsigned
```

包默认仅主动检查更新；没有构建公钥时默认关闭，不发无效网络请求。官方稳定源为 GitHub 最新正式 release 的 latest.json；预览源为官网 /updates/beta.json，发布前必须实际部署该文件。源码可替换源/公钥或关闭更新，仍保留签名校验；稳定源拒绝预发布版本。NSIS 和 MSI 使用各自 windows-x86_64-nsis/msi 更新目标，未知/裸 exe 不跨用另一种安装器。

## 5. 验证与生成清单

把最终签名后的 NSIS/MSI 与对应 .sig 放在一个只含发行安装器的目录；不要混入裸 exe 或旧包。日常两指令已自动完成此步。以下仅供签名服务返回最终包后手工收口，输出完整待上传目录，**不会上传**；--out 必须不存在。

```bash
pnpm release:finalize /mnt/c/rb-target/raybend-release/release/bundle \
  --manifest dist/raybend-build.json --out release-out/所选版本 \
  --base-url https://github.com/C-Thun/raybend/releases/download/v所选版本/
# 明确无 Authenticode 签名时另加 --allow-unsigned
# 不发布更新时省去 --base-url；不会生成更新 JSON
```

输出安装器及 .sig 的字节副本、构建 manifest、发布说明、SHA256SUMS、release-index.json 和可选 latest.json/beta.json；有更新清单时每个安装器必须存在 .sig。signtool /pa /all 验证失败会停止；不把校验和当成发布者签名。发布索引保留来源/脏树/签名状态。installer 已有许可证与声明资源，public/legal 清单嵌进前端；发布前自动重生成。

上传前在全新 Windows 中核对有效签名及时间戳，确认 installer 的产品版本/通道与构建 manifest 一致。Signtool 验证并不证明 M4 功能、安装恢复或 SmartScreen 信誉已通过。

完成全部验收后由您执行第二条 release:publish --execute；它提交精确版本文件、tag/push、上传最终包/.sig/清单，并等待正式 Release 触发官网更新。没有发行包时保持未发布状态。第三方签名服务改变包字节后须重新 finalize，不能沿用旧 SHA256SUMS。

## 6. MSIX 商店路线（本轮后移）

崔总 2026-09-27 明确：登记 FUTURE 后续独立启动，本轮先完成直下发行链。正式落点为 FUTURE.md 的 Windows MSIX 分发条目；没有把它列成本轮发布的前置。

仍需您开户、预留产品名并取得 Partner Center 的 Package Identity Name/Publisher；目前没有这些实际身份，不能提交有效 MSIX。Tauri 官方当前教程覆盖 EXE/MSI，MSIX 需要另用 Windows SDK MakeAppx/打包工具包装，不能复用 NSIS 直接上传冒充 MSIX。

包装前先准备编译时 RAYBEND_DISTRIBUTION=store 的构建（前端与 Rust 两端一致），内置更新由编译身份禁用。现有 CLI 直下包固定 direct；商店构建和包装流水线在拿到身份后另接，不能拿 direct 包换个后缀上架。

验收须覆盖 MSIX 数据位置与直下版迁移、卸载数据保留、自重启 RAW worker、导入目标/导出目标权限、WebView2、外部编辑器启动和未来更新方式。商店版本交由商店更新，不能同时用内置 NSIS updater 改写程序。产品中尚不显示“商店认证”或虚假商店链接。

## 7. 真机验收与失败恢复

在干净 Win10 22H2 / Win11（至少一台干净配置，可用 VM）依次验：初次安装/首启建库与已有库、中文/Unicode/只读路径、浏览编辑保存及真实导出、自重启 worker、vN→vN+1 更新、下载断网、签名不匹配、安装失败/磁盘不足、导入或导出进行中尝试更新、卸载/重装保留照片与库。
下载失败不启动安装；安装阶段系统行为需实测，不宣传“失败一定自动回滚”。更新后还没启动时可恢复安装；一旦数据库升级，旧程序会触发 SchemaTooNew 拒绝打开。迁移前快照用于按版本恢复，不能直接覆盖升级后新增照片/编辑。恢复需先复制当前库，确认要恢复的时间点，保留差异，不静默删库重建。

未通过这些测试就不能勾选 M5 DoD。GUI/色彩/大库性能/E2E 由您确认；Agent 的编译和单测只属于冒烟。

## 8. macOS / Linux 的签名要求（远期参考，2026-09-27 核实）

三平台的结构完全不同，**按开发成本排序：Linux（$0）< macOS（$99/年）< Windows（$250–400/年）**。

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| 未签名直下体验 | SmartScreen 提示，用户“更多信息 → 仍要运行”（两下） | **Sequoia 15 起不能右键绕过**，须去系统设置点“仍要打开”；Apple 支持文档劝阻（称其为 Mac 中毒最常见方式） | 无警告（AppImage/tar.gz）；deb/rpm 由发行版仓库机制负责 |
| 签名颁发者 | 多家 CA（DigiCert/Sectigo/GlobalSign/SSL.com/Certum…），信任库由微软 Trusted Root Program 决定 | **只有 Apple**（Developer ID），无第三方入口 | 开发者自持 GPG 密钥，无 CA |
| 成本与硬件 | $250–400/年；CSBR 强制私钥进硬件（Token 或云签名） | $99/年 Apple Developer Program（唯一入口，含其他服务） | $0 |
| 公证/沙箱 | 无强制公证 | **强制公证**（2019-06 后构建的 Developer ID 软件）；App Store 另强制 App Sandbox | 无 |
| 商店路线 | MSIX 认证后**微软重签**，可不买商业证书 | App Store 须**先有 $99 会员并自签**（Mac App Distribution + Installer 证书），Apple 再重签 | Flathub/发行版仓库各自审核 |

要点：

- **macOS 的 unsigned 直下基本不可行**：Sequoia 起绕过流程变成系统设置里的多步操作，Apple 还主动劝阻。远期做 macOS 版时，$99/年 是必选项而非可选项；好在它比 Windows 证书便宜且入口唯一、全自动。
- **Apple 是签名权的唯一来源**：会员资格是“请求、下载和使用 Apple 颁发证书”的前提。Windows 至少是多 CA 竞争 + 微软守信任库；macOS 是单一厂商——三平台里最集中的结构。
- **App Store 不是“免签名通道”**（与 Windows Store 不同）：微软 Store 的 MSIX 可不买商业证书（认证后重签）；Apple 必须先有 $99 会员并自行签名上传。
- **沙箱是 macOS 的产品层门槛（比证书更重要）**：App Store 强制 App Sandbox，访问沙箱外目录需用户授权的 entitlement；照片管理软件若走 App Store，库目录、导入源、外部编辑器都要在授权模型内重新设计。直下分发（Developer ID + 公证）没有这条限制。
- **Linux 的信任模型就是“密钥=身份”**：deb/rpm 走仓库级 GPG（apt-secure 签 Release 文件），Flatpak 走 OSTree commit 的 GPG 签名（`flatpak build-sign`），AppImage/tar.gz 无需签名。没有 CA、没有身份验证、没有硬件要求，用户自行决定信任谁的密钥。
- **Sigstore 是同一思路的现实实现**：Fulcio 依 OIDC 身份签发约 10 分钟短期证书，Rekor 用 append-only 透明日志（Merkle 树）记录签名，免费、无 CA、无硬件。**落差点在平台侧：Windows/macOS 不查 Sigstore**，只认自家信任根——这类方案能解决“可审计”，不解决“被放行”。

来源：[Apple Developer ID](https://developer.apple.com/developer-id/)、[macOS 公证](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)、[Apple 证书与会员要求](https://developer.apple.com/support/certificates)、[macOS 15 绕过变更（Ars Technica）](https://arstechnica.com/gadgets/2024/08/macos-15-sequoia-makes-you-jump-through-more-hoops-to-disable-gatekeeper-app-checks/)、[Debian 包签名（apt-secure）](https://www.debian.org/doc/manuals/securing-debian-manual/deb-pack-sign.en.html)、[Flatpak build-sign](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)、[Sigstore 安全模型](https://docs.sigstore.dev/about/security/)。
