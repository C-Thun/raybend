# M5 独立分发工程准备
完成时间：2026-09-26 21:25:41 CST

## 范围与边界

崔总授权「按计划先推进 M5 中能推进的部分」。在 M4 大量未提交改动之上仅局部接入，不清理/回滚/提交并行修改。W1/W2/W3/W4 各在开工时写当前规格，没有新造升版、Windows dav1d、库创建或弹窗体系。当前产品版本仍为 0.1.0；只做源码、配置、文档、前端编译、原生 check/单测与合成工装，没有生成 Windows 发布安装器、签名、tag、push、上传或 GUI E2E。

M5 独立工程准备完成；阶段 DoD **未完成**，真实安装/升级/卸载及信任仍待您执行，公开发行仍等 M4 收口。MSIX 包装与商店身份、免费签名服务适配没有伪造为已完成。

## 改动与复用

- `src/lib/release-plan.ts` / tests、`scripts/release.mjs`：严格版本/参数、两种 channel 写法、未知/重复参数拒绝、Windows 数值范围、test 独立输出、不产生 test tag 提示、正式来源预检；package/Cargo workspace/两个本地锁条目事务同步，失败恢复时不覆盖外部编辑。Tauri version 读取 ../package.json，当前版本没升。
- `scripts/lib/release-files.mjs` / `release-windows.mjs`：复用 dav1d/WSLENV，C: target、locked/custom-protocol；先检 Windows cargo-tauri 和签名工具，前端只构建一次；显式有/无 Authenticode 选项，HTTPS 时间戳；更新公私钥独立于证书。Windows 可读的密钥文件路径自动转换，日志不打印私钥/密码。
- `scripts/check-win-artifact.mjs`：增加独立 frontend 路径与 self worker 模式，保持已有协议/内容/时间核对；合成回归证明旧协议/旧前端仍拒绝，默认 debug 仍要求独立 worker。发行包使用既有主程序自重启入口，不安装第二份可能过期的 worker。
- `src-tauri/tauri.conf.json`：NSIS 每用户、English/SimpChinese、稳定 MSI upgradeCode、禁降级、许可证/声明资源、真实官网链接；没有卸载删库钩子。许可与 user-data 不混放。文件关联为可选项，未扩围。
- `vite.config.ts`、`src/index.tsx`、`src-tauri/src/distribution.rs`、spike/lib/build.rs：dev/test 保留 GPU 诊断，beta/release 前端摇除，Rust 建窗入口也按编译通道拒绝；构建身份显式值优先，避免预期升版写入被误判成代码脏树。manifest 的 gitHash 是升版前代码基点，最终版本提交由您完成，文档已解释。
- `src/shell/WelcomeDialog.tsx`、`src/lib/welcome.ts` / test、`src/App.tsx`：只在原生、成功读到零库且设备偏好未确认时欢迎；读失败不当作空库。创建/登记已有库复用现有 CreateRepositoryDialog、newRepository 命令与目录探测，没有第二个库向导。localStorage 为 raybend.welcome.v1，无旧 key 迁移。
- 官方 `tauri-plugin-updater` 2.12.0（MIT OR Apache-2.0）新增至 src-tauri，Cargo.lock 局部更新；没有新增顶层框架。`updates.rs` + `src/api/updates.ts` + `UpdateDialog.tsx` / `update-source.ts`：关闭/稳定/预览/自定义 HTTPS 源+公钥、只主动联网、检查/下载验证/安装分离、稳定拒绝 prerelease、源绑定、并发拒绝；安装前复用 ImportBatches/ExportState、前端编辑保存/迁移保护。原生 Store 编译身份拒绝内置更新；现有直下 release 管道明确为 direct，Store 管道待真实身份后接。
- 下载失败不留可安装 bytes，配置更换/检查失败不继承旧候选；没有把 OS 安装失败、进程重启或 DB 回退宣称已自动恢复。缺官方公钥时默认关闭；第三方可换源+公钥或关闭，验证不可跳过。
- `scripts/generate-licenses.mjs` / `license-inventory.mjs`、`legal/`、`public/legal/third-party.json`：从锁定 Windows normal/build 图与 pnpm production 生成 593 项保守清单，含版本/标识/作者/来源/原文、Cargo/pnpm 锁摘要，不含本仓绝对路径。额外收录 dav1d、lensfun 校准数据、native libwebp COPYING/PATENTS 与源码复用声明。上游遗漏原文按精确 commit 补取；libwebp-sys/selectors/simd_helpers 标记标准 SPDX 补足，保留作者，不编造年份，详 legal/README.md。
- 关于窗口、TitleBar 与命令组装复用为离线帮助/隐私/许可检索、诊断摘要。中/英使用与隐私说明；诊断只复制构建/runtime，不含照片路径、用户名或库记录，不上传。纠正「照片与目录永不改动」和 main/master 许可链接。
- `scripts/finalize-release.mjs` / `release-artifacts.mjs`：最终 installer 的 signtool /pa /all 验证、SHA256SUMS、字节/来源/签名状态索引、带 .sig 的最新/预览清单；NSIS/MSI 平台键分开，不设会跨安装器的裸 windows-x86_64 fallback。显式无签名只能如实标 unsigned，缺更新 .sig 不生成更新清单；本轮只测纯函数。
- `docs/release.md` / user-guide / privacy、README 双语、THIRD-PARTY-NOTICES、PLAN/REVIEW：实际操作、免费路线、源代码取得、恢复边界与进度。官网只有信任文案校正，中/英不再保证总有「仍要运行」；没有伪造公开下载版本/商店认证/捐赠账户。官网发布未触发。

## 设计与命令

使用 Pencil 设计技能与已有 main.pen Dialog/品牌/按钮，编辑器中新增/修改欢迎 eyYHP、离线许可 uIuzv、更新 OBgSA，main.md 说明同步；MCP 布局检查无 clipping，截图目视检查过。三张预览导出到 `/tmp/raybend-m5-design-preview/`。

**未完成持久化**：Pencil MCP 没有保存接口，磁盘 main.pen 的修改时间仍为旧值，编辑器变更需您切到该画稿保存。没有用文本/Python 读写加密 .pen 或冒称已落盘。

help.docs 默认 F1，命令冲突测试通过；help.welcome/licenses/updates 明确 defaultKey undefined，低频动作避免误触。都进入命令面板/设置/标题栏菜单。模态内检索、切页、复制摘要、下载/确认安装属于当前配置下的弹窗内部交互，不另设热键或全局后台联网命令。

## 已验证（冒烟）

- pnpm typecheck：通过。
- pnpm test：1036 通过（约 1.8 秒）；M5 新增 welcome/update-source/support-info/命令回调和 F1 冲突边界。
- pnpm test:release：31 通过（约 0.3 秒）；版本事务、磁盘失败/并行修改、controller dry-run/失败/输出隔离、cmd 路径、签名配置、许可图/缺失、合成二进制、Unicode artifact URL/缺签名/重复/无签名策略。
- pnpm lint:colors / lint:arch / lint:i18n：通过。
- pnpm build：通过。发行通道另编译到 /tmp，断言没有 SpikeViewport/KitchenSink JS 块和 spike_open 入口；不是生成安装包。
- cargo check -p raybend-desktop --locked：WSL 通过。
- cargo test -p raybend-desktop --lib --locked：93 通过、1 个已有手动项 ignore；约 0.04 秒测试执行时间（首次新增插件编译另计）。
- Windows 原生 cargo check -p raybend-desktop --features custom-protocol --locked：MSVC 通过（首次 graph 构建 4m30s），target 位于 C:\rb-target\raybend，仅 check，没有生成/运行 GUI exe。
- pnpm licenses:generate：593 项；锁摘要相符、无本仓路径，SPDX 补足项透明标记。
- release test --windows --unsigned --dry-run 与 patch --windows --unsigned --dry-run：只读计划通过；正式脏树阻断在计划中明确显示。
- website 独立 typecheck/build：通过；oxlint 无错误，已有 Hero.tsx:90 prefer-structured-class warning 仍在（未扩改无关代码）。

## 未验证与需要您执行

1. 保存 Pencil main.pen；pnpm debug:win 运行当前代码，人工确认欢迎/既有库登记、F1/许可检索/诊断/更新关闭设置。GUI 与真实目录权限不据单测标已验证。
2. 如启用内置更新，您生成并安全备份 Tauri 私钥/密码，提供公钥构建配置，部署对应 HTTPS 更新 JSON；没有密钥/服务器时仍可关闭并手动更新。
3. 官网直下可显式无签名；若要免费发布者签名申请 SignPath，若选商店则免费新入口开户并取得真实 Identity/Publisher。MSIX 包装/源仓 CI 签名服务适配在外部输入明确后继续，不购买 EV 为开工前置。
4. M4 收口后由您生成两版包、验证签名/时间戳/源码/校验和，在干净 Win10/11 做安装→升级（含断网、坏签名、忙任务与安装失败）→卸载重装数据保留。DB 已迁移的恢复要保留新增编辑/快照，旧 exe 不等于旧 DB。
5. 验收后您 commit/tag/push/上传最终包/.sig/清单及对应源码，触发既有官网 workflow。完整命令见 docs/release.md。捐赠地址属可选输入，未配不影响发行。

本会话没有可用 plannotator/todo 工具，已在 W1 规格说明，没有虚构评审通过或另开 Agent 会话。检查通过后没有继续扩大测试范围；没有自动 commit。
