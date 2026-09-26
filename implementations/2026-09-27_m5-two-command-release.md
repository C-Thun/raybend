# M5 两指令直下发行与官网接线收口
完成时间：2026-09-27 02:14:04 CST

## 范围与授权

承接 specs/HANDOFF-2026-09-27-m5.md。先读项目/全局约束、M5 实施与规格、M4 最新 finalization/RAW metadata 修复记录及源码；未重复实现 M4。崔总追加要求日常发行不超过三条指令，已收敛为两条。保留当前 master 工作树与全部原有未跟踪文件，未 reset/clean/restore、全仓格式化或提交并行修改。

**本次没有生成发布安装器、签名、tag、push、上传或部署网站。** 发布副作用只存在于崔总以后显式执行的第二条指令中；测试用模拟 git/gh，不调用真实账户或创建测试 tag。

## 本次完成

1. `scripts/release.mjs` 复用原版本事务、dav1d、Windows build/check：第一条 `pnpm release patch --windows --unsigned` 自动完成升版、许可、一次前端构建、Windows NSIS/MSI、嵌入内容/worker 检查及 finalize。只有 --with-updater 才把构建公钥启用并生成签名更新文件；手动更新包不混入旧 .sig。Windows CLI 版本须与 WSL @tauri-apps/cli 一致，缺工具在改版本前停止。
2. `scripts/finalize-release.mjs` 改为可注入执行器的控制器，自动收口只收本版文件、要求本波所有目标安装器齐备。输出全新且独占创建的 release-out/v版本，含安装器、可选 .sig、构建 manifest、SHA256SUMS、release-index、可选 latest/beta JSON、发布说明。拒绝旧目录、符号链接、重复 basename、缺签名/目标，不覆盖并行文件。Standalone finalize 仍保留给远程签名服务返回后的人工收口，不属日常额外步骤。
3. `scripts/release-publish.mjs` + `scripts/lib/release-publish.mjs` 为第二条：`pnpm release:publish release-out/v版本 --execute`。默认只读预览；显式执行后核对哈希/源快照/版本事务/许可锁、拒绝其他修改或暂存内容，仅提交版本与生成许可，创建精确 tag、原子推送 master + 该 tag、创建草稿/上传/核对远程 SHA-256/公开，再等待官网 Actions 成功。origin 固定核对本仓、不强推、不推其他 tags。
4. 上传中断保留现场，重跑只补缺失资产；仅清除草稿中 starter 零字节失败占位，已完成资产不删除/覆盖。远程字节冲突拒绝；官网失败如实报错，重跑同一指令会重跑匹配失败 workflow 一次，无重复上传。脚本不声称已公开 Release 可回滚。大许可文件用 64MiB 命令缓冲，缺远程 digest 的资产回读按本地实际文件尺寸设缓冲，避免默认 1MiB 截断（现有许可 JSON 实测 4,742,546 字节）。
5. `scripts/lib/release-upload.mjs` 复用 releaseMetadata 为本地上传计划核对；源文件名单放在 release-files 单一契约，构建/发布共用，版本规则也复用原实现。新增/扩展 release-run/upload/publish/artifacts 单测，package.json 仅新增 release:publish，不手改 packageManager。
6. 官网复用 `src/data/release.ts` 与原 `.github/workflows/website.yml`。`website/scripts/release-info.ts` 把正式 published 事件的完整资产注入真实下载，其它 CI 查询指定 tag/latest；无版本 404 为 pending，网络/认证/指定 tag 错误停止部署保留旧网站。过滤草稿、预发布、非法 SemVer、其他版本/平台/裸 exe/不合法下载源，NSIS 优先 MSI。无需另做 website commit/push，也无浏览器 GitHub API。实际静态编译验证过事件样本的直链注入，之后重新本地构建恢复 pending，不留模拟版本产物。
7. 补 docs/user-guide.md/.en：当前显示定稿不改 latest、四格式、预设保存门槛与内存队列、四预设并行/停止/重置/失效、单向 RGB16 TIFF、外部应用发现/后台阶段与失败路径。核对 App UpdateDialog 与原生 updates_install 已共用 M4 外部任务保护、导入/导出及编辑/迁移保护，没有再造任务状态。
8. docs/release.md 改为日常两指令和一次性准备；PLAN 修正陈旧 M4/Pencil 状态；FUTURE 正式登记 Windows MSIX/Store 后续另开，本轮不要求商店身份；website/AGENTS 和 M5-W1/closeout 规格、design/main.md 同步真实接线与验证边界。

## Pencil 与许可核查

- 使用 pencil-design 与 Pencil MCP 读取 design/main.pen 的 eyYHP / uIuzv / OBgSA / FPU9M，节点存在，布局遍历无 clipping。磁盘 main.pen 495615 字节、2026-09-27 00:46:33 CST，未修改 .pen。
- 不能把上项当作磁盘独立重载证明：不解析内容地复制加密文件到 /tmp 后，MCP 未打开新路径而回到当前 export 画布；computer-use 的 node_repl 初始化失败，错误 `sandboxCwd is not a local file URI: file:///home/andares/repos/c-thun/raybend`。没有声称文件坏了或已确定未保存，没有用文本/Python 解析加密 .pen。需崔总保存 main.pen 并重开后再核对；工具问题可绕过，不阻断其它收口。
- 原 WSL shell 沙箱因 /mnt/wslg/distro 挂载错误无法启动，本次仓库命令经 require_escalated 审核执行；未出现拒绝或修改系统挂载的绕法。
- 许可现有 593 项，Cargo/pnpm 锁摘要重新核对一致，无依赖变更，未重复刷新资源。三补足包的精确 VCS commit/元数据/文件头/上游树已核对：selectors MPL-2.0 引用与标准原文相符；libwebp-sys/simd_helpers 两项 MIT 上游精确树仍无原始版权文本，保留 spdx-fallback/包作者，不编造年份或将 vendor BSD 当绑定 MIT。公开发行前仍需确认两项，细节写入 legal/README.md。

## 实际验证（本次 Agent 冒烟）

| 检查 | 本次结果 |
| --- | --- |
| pnpm test:release | **45 passed / 0 failed**，最终约 0.24s；含两指令 controller、来源/暂存/并行保护、中断补传/零字节占位、远程冲突、Pages 失败重跑、旧 bundle 隔离、CLI 前置与大许可缓冲。全为临时合成文件/模拟 git/gh，没有真实安装器或线上发布 |
| pnpm test | **1059 passed / 0 failed**，约 2.31s；与 release-plan 用例有交集，不相加宣称独立总数 |
| pnpm typecheck / lint:colors / lint:arch / lint:i18n | 全通过 |
| pnpm build | 通过；仅既有大 chunk 提示；包含本次帮助文本，未重建 Windows debug exe |
| RAYBEND_CHANNEL=release pnpm build --outDir /tmp/... | 通过；无 SpikeViewport/KitchenSink JS 块或 spike_open 入口，只静态前端 |
| cargo test -p raybend-desktop --lib --locked | **95 passed / 0 failed / 1 ignored**，执行约 0.07s；本轮未改 Rust 源码，未重跑重型核心测试 |
| 官网 pnpm test:run / typecheck / lint / build | **24 tests passed**，类型/构建通过；lint 只有已有 Hero.tsx:90 structured-class warning |
| 官网 published 事件 fixture 的实际构建 | 通过，编译 JS 命中预期 GitHub 安装包直链；未联网/部署，之后无 fixture 本地构建再次通过并确认不含模拟版本 |
| release patch/test --windows --unsigned --dry-run | 只读通过，正式脏树阻断明确；未升版或调用 Windows bundle |
| 许可资源 / git diff --check / CLI syntax | 593 项两锁一致、差异检查通过、release-publish.mjs 语法通过 |
| 一次性工具只读检查 | WSL tauri-cli 2.11.4；Windows cargo-tauri 命令未找到；WSL command -v gh 未找到；C:/rb-deps/dav1d-1.5.0/lib/libdav1d.a 已存在 |

## 命令体系与外部边界

本次新增的是开发者发行脚本，不是摄影师在应用内触发的动作，故不新增 UI 命令或热键。帮助继续 F1 / Ctrl+K；导出 Enter / Ctrl+Enter、外部交接低频无默认键，均复用原统一 catalog，未硬编码新监听。没有 UI 布局变更或第二份帮助组件。

## 未验收及崔总步骤

- 先审阅并提交当前开发树，保留并行工作；一次性装匹配 Windows cargo-tauri、WSL GitHub CLI 并登录，核对 origin/权限、Actions/Pages/DNS/HTTPS。完整说明 docs/release.md；这不计入每版重复动作。
- 保存/重开 Pencil main.pen 后核对三节点；确认两项 MIT 版权补足。签名更新可后配：崔总自行生成/备份密钥，--with-updater 才启用；MSIX 已后移，不阻断本轮直下包。
- 执行第一条生成安装器，在干净 Win10/11 验安装/首启/worker、当前 M4 真照片导出/外部应用打开、DPI/色彩/性能、升级/断网坏签名/安装失败及卸载重装数据保留；核对发布说明。两版实际更新与数据库恢复未验证。
- 验收后执行第二条完成 tag/push/上传/正式 Release/官网 Pages，打开发布页和官网确认下载与访问。实际账户、GitHub API、远程 asset digest、Actions 权限和 Pages 未经本次真实执行；M5 DoD 仍未勾选，不把冒烟当真机 E2E。
