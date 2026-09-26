# M5 并行准备与分发信任评审
完成时间：2026-09-26 20:22:16 CST

## 范围与文件

仅更新 REVIEW.md：当前汇总追加 R2-01–04，追加 R2 的判断、去向和官方依据；未修改 PLAN/FUTURE 的排期或 M4/M5 的状态，未改业务与 release 代码。
核对 PLAN 的 M4/M5、最新 W3 实施记录、release 脚本及纯逻辑/单测、Vite 构建信息、Tauri/Cargo 配置、debug:win 与 check:win、RAW worker 兜底入口、既有建库组件、官网发布信息与下载提示、数据库版本闸门。

## 发现与复用决定

- package.json / Cargo workspace / tauri.conf.json 当前都为 0.1.0，但 release.mjs 只有 package.json 写回。今后直接升版会使前端版本与 Rust/Tauri/安装版本分离。
- release 当前只跑 pnpm build，不做 Windows release、NSIS/MSI 或更新产物；配置有 nsis/msi targets 不能视为已验证安装流程。
- release 的 RAYBEND_CHANNEL/BUILD_TIME 等只存在该次子进程；后续 debug:win / Tauri beforeBuildCommand 若重新跑前端构建，须保持同一构建身份，不能丢通道与时间。
- 实测文档示例 `minor --channel beta --dry-run` 退出 1；`minor --channel=beta --dry-run` 成功得到 0.2.0-beta.1。参数目前放过 --channel=bogus 和 --unknown；版本解析也放过前导零、beta..1 与大于安全整数的分段。现有单测未覆盖这些缺口。
- plan 打印 test 产物目录 dist/test-build，但 Vite 实际始终输出 dist。plan 的目录单测只验证计划对象，未验证实际产物隔离。
- 版本写回发生在构建之前；构建失败没有恢复写回。git 状态读取失败当成空状态；git hash 缺失仅警告。须在正式发布策略中明确失败阻断、重试版本与写回一致性。
- test 计划也打印 tag/push 提示；建议只为真正发行计划显示这些指令，避免自测被误作发布。
- Windows 日常构建已有 dav1d 环境封装与产物检查，应提取/扩展共用构建步骤，不直接调用会重建前端且固定 debug 的整段 debug:win。
- RAW worker 已有「主程序 + --raybend-raw-worker」兜底，并在 main 初始化 GUI 前处理；不能误报安装包缺独立 worker 就必然失效。需要安装布局下验证选定的独立 worker / 自重启路径、协议与所有可执行文件签名范围。
- 首启应复用 repositories 的已有创建/打开能力；不自行扩成第二套建库流程。spike IPC 与诊断入口仍编入薄壳，发布策略须明确。
- 现有 migration.rs 在 SchemaTooNew 时拒绝打开，自动更新的回退策略不能只替换 exe。
- M4 最新 W3 记录（19:44:46）已列开发/冒烟证据，早一份 pause 记录被后续工作推进；五格式与真实后台执行、外部编辑和实际 GUI 仍待后续工作。PLAN 的「当前工作单元 W2」是过时引用，进度以最新实施记录核对，本轮没有代改其他正在进行的文档。

## 验证

已验证（单元/冒烟）：
- pnpm exec node --test src/lib/release-plan.test.ts：18/18 通过，约 0.16 秒。
- pnpm release patch --dry-run：退出 0，目标 0.1.1，报告脏树，不写文件、不构建。
- pnpm release minor --channel beta --dry-run：退出 1，复现说明与解析器不一致。
- pnpm release minor --channel=beta --dry-run：退出 0，目标 0.2.0-beta.1，不写文件、不构建。
- 纯函数定点执行复现非法参数与非法版本未拒绝的问题；未运行任何非 dry-run release。
- 文档变更的 git diff --check：见本轮最终检查。

未经验证：安装包生成、安装/升级/卸载、Windows 信任弹窗、MSIX/子进程/外部软件交互、签名申请审核及开发者账户开户。相关真实环境验收归崔总。

## 官方依据与遗留

引用集中在 REVIEW.md §7.2，本记录不复制路线建议。四项均待讨论采纳；没有制定未开工波次的实施细目。
本机默认 exec 沙箱因 /mnt/wslg/distro 的 bubblewrap 挂载错误不能启动，改用经过审批的定点只读调用与本次授权的评审文档写入；未改系统挂载或配置。
联网采用已安装 Firecrawl 搜索和官方网页核实；缓存放 /tmp/raybend-m5-*，避免污染仓库；未安装工具或依赖。
本次没有新增用户动作，命令注册表/默认热键不适用。没有发布物生成、tag、push、上传或自动 commit。
