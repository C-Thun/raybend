完成时间：2026-09-29 21:48:44 CST

# app.db 启动等待与 catalog 升级遮罩

## 改动范围与原因

- 产品启动先显示既有 splash，在后台打开/迁移 app.db；首屏就绪和 4 秒兜底不能跳过尚未返回的数据初始化。打开失败也结束 splash 等待，由原有命令错误/库加载反馈继续呈现。开发模式仍立即显示窗口。
- 设置、数据库状态和最近目录命令复用 source::blocking，等待 app.db 打开锁不再阻塞窗口事件线程。已有 SQL 迁移的版本闸门、VACUUM INTO 快照、逐条事务和完整性检查保持原入口。
- CatalogSessions 自动重连/打开已会触发 migration::apply，并在成功前不交付会话。本轮复用这条链；新增每次迁移的唯一 u64 编号，Start/Done 共享编号，多个 catalog 不再按同一 kind 覆盖。
- 外壳保存活动升级的完整快照与递增 revision，事件与只读查询共用 DTO。前端先订阅后查快照，旧响应不覆盖新事实，前端重载/晚监听可恢复遮罩；订阅失败只降级每秒查询内存，查询单飞、销毁后停止。
- 既有 MigrationGate 标题明确为「库数据升级中」，正文去掉 SQL/schema 版本细节。Portal 和专用 z-migration 层覆盖 titlebar、已有对话框与 message；捕获键盘、滚轮及真实指针输入，保持焦点并在完成后恢复，无 Esc/取消出口。多个活动执行全部结束后才撤罩。
- updater 原生端检查活动升级，补齐现有前端 canInstall 条件。

## 涉及文件与复用

- 核心：crates/raybend/src/store/migration.rs（编号与边界/并发测试）。
- 外壳：src-tauri/src/lib.rs、db.rs、source.rs、migration.rs、updates.rs、contract.rs。
- IPC：src/api/db.ts、types.ts、dto-contract.json、dto-contract.test.ts；事件订阅复用 api/events.ts，取消数据库模块原有重复订阅实现。
- 状态与界面：src/features/migration/*；src/App.tsx；src/dev/migration-gate-demo.tsx；src/i18n/{zh-CN,en-US}.ts；src/styles/tokens.css；scripts/ui-smoke.mjs。
- 通用十进制 revision 从 repositories/state.ts 提取为 src/lib/revision.ts，库状态与迁移共同使用，避免第二套大整数比较逻辑；同时交付边界单测。
- 规格与说明：specs/database-upgrade-gate.md、design/main.md、memory/{ARCHITECTURE,DESIGN,FUNCTION-REPOSITORY}.md。
- 预置图需求原文：todos/2026-09-29-catalog-preset-image-upgrade.md。

## 关键决策

- 全窗阻塞与提示复用已批准的现有布局；只有状态、层级和文案修订。按执行编号区分多个库，完整快照处理事件乱序，编号/revision 以十进制字符串跨 IPC，避免 JS Number 截断。
- 本轮不新增 schema，不另造迁移通道，不增加依赖；不重写库位置探测、会话或图像生成。
- 自动升级不作为手动命令：通过既有打开/自动重连触发；不分配默认热键。repository.reconnect 仍走原命令面板/菜单，默认键留空（低频库维护）。
- 崔总本轮明确选择修正启动和现有遮罩；Pencil get_app_state/read_skill 均 IPC 60 秒超时，无法改 .pen。在配对 main.md 记录既有布局的文案/行为修订，未手工写入 .pen。
- 本会话没有可调用的 pi todo 工具；不另建进度文件，spec 仅为需求/验收规则。

## 已验证（单测 / 冒烟）

- pnpm typecheck：通过。
- pnpm test：1131 项通过，约 1.52 秒；涵盖并发 catalog、晚订阅、事件/查询乱序、大整数精度、重复开始/未知结束、订阅失败降级、查询单飞与销毁清理。
- pnpm lint:colors / lint:arch / lint:i18n：通过。
- pnpm build：通过；保留既有 >500 kB chunk 提示。debug:win 内再次构建最终前端。
- cargo test -p raybend store::migration::tests --lib：36 项通过，执行约 0.63 秒，覆盖既有迁移、快照、数据保留、失败回滚、未来版本拒绝、唯一编号并发与溢出。
- cargo test -p raybend-desktop --lib：97 项通过、1 项既有真实 RAW 测试 ignored；启动等待、活动快照并发/溢出、IPC 契约与状态注册通过。收尾日志修订后启动闸门单测复验通过。
- cargo check --workspace：通过。
- pnpm smoke:ui http://127.0.0.1:1420/dev/kitchen-sink：通过，problems=[]；现有可重复脚本验证标题、遮罩、方向键/数字键/Esc 捕获、滚轮 preventDefault、完成撤罩，无控制台错误。
- pnpm debug:win：完整脚本通过（含既有 target 收尾），Windows cargo 构建通过（约 1 分 50 秒），内置 check:win 通过；主程序时间 2026-09-29 21:45:54.957 CST，worker 时间 21:44:47.343 CST，协议 raybend-worker-proto-v3，dist 的四个引用资源全部命中。程序路径 /mnt/c/rb-target/raybend/debug/raybend-desktop.exe。未进行 Windows GUI 交互验收。
- git diff --check：通过；未修改本轮以外的既有工作区改动，未提交混合工作区、未 push/tag/发行。

## 环境绕法与遗留边界

- 受限沙箱中的 pnpm 无法联网验证自身签名；工具授权后相同命令通过。未更改 packageManager、依赖/锁文件或关闭签名校验。
- Pencil 无法连接，main.pen 同步未完成；现有布局被复用，说明已同步 main.md。
- Windows splash 是否在慢升级期间正常显示与响应、真实旧 catalog 的升级/失败表现、已有弹窗与窗口三键能否被盖住、多库真实并发，仍需崔总在真机确认。本轮的自动冒烟不代表 GUI/视觉/E2E 已验收。
- 当前升级只覆盖 SQL。未来若需生成必需预置图，SQL Done 不能提前结束整体升级；需要 Rust/worker 的分批幂等任务、持久完成状态、重启续做及明确失败策略。新状态字段仍走既有 SQL 迁移框架。
- 现有位置探测等待上限 5 秒未改变；长生成任务不可直接塞进探测等待并误报为离线/损坏。具体接入时应分离位置探测与已识别升级等待；本轮未实现图片生成任务。
