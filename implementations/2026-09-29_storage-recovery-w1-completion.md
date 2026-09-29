完成时间：2026-09-29 01:30:26 CST（UTC+08:00）。

# Storage Recovery W1：库连接、永久设置入口与同库备用位置收口

本次接续 `2026-09-29_storage-recovery-w1-checkpoint.md`。崔总已授权继续完成当前首波，并明确回复“按修订稿实施”。W1 开发、单元测试和 Agent 合成冒烟收口；不把画稿确认、Windows 编译或合成数据测试当成真实拔插/照片/GUI 验收。W2–W4 尚未开工，NAS 协议仍未选定。

## 改动范围与文件

- 核心 `store/{availability,session,db,pool,writer,repository,location}.rs` 与 `error.rs`：共用身份探测、运行连接状态、有界探测预算、固定 catalog 租约与单写者退场。位置验证和移除登记新增在既有 repository 模块；移除登记的本机事务与同库获取/退场互斥，后端保护使用中根位置。
- `import/{fsops,runner,sink}.rs`、`store/delete.rs`、`export.rs`、`export/output.rs`、`external_editor/mod.rs`：复用会话闸门，失效后拒绝后续写与文件发布；保留已成功项，失败如实传达，旧任务不自动换根续跑。
- Tauri `browse.rs`、`db.rs`、`repo.rs`、`import.rs`、`develop.rs`、`export.rs`、`external_editor.rs`、`lib.rs`、`contract.rs`：生产 catalog 入口使用统一租约；转发结构化连接与位置结果。新增 `repository_add_location`、`repository_remove_location`，类型、命令注册与共享 DTO 键契约同时补齐。唯一生产 `CatalogDb::create` 留在明确的新建库分支，定位不走该分支。
- 前端 `App.tsx`、`api/{db.ts,types.ts,dto-contract.json,dto-contract.test.ts}`、`features/repositories/{state,monitor,location-controller,RepositoryList,LibrarySettingsDialog}` 及测试、`features/browse/BrowsePanels.tsx`、三个 workspace：App 唯一库状态和设置弹窗，统一逐库合并/单飞/迟到结果丢弃，离线保留展示资料、选择与草稿；恢复刷新当前内容和预览。
- `components/ui/RepositoryCard.tsx`、`i18n/{repository-feedback,zh-CN,en-US}`、`features/commands/catalog.ts` 及测试：卡片永久齿轮、独立状态图标，普通未找到不增加红字行。共用双语原因映射，原始异常不直接进入位置提示。卡片只接呈现所需字段，避免 UI 反向依赖 IPC DTO；文案映射归 i18n，不放在纯逻辑 lib 里。
- `scripts/{ui-smoke,check-import-boot,check-browse-boot,check-export-boot}.mjs`：复用现有 CDP 工装和冒烟，补离线设置/定位恢复接线；既有合成 PNG 输入合法化，导入超时诊断更完整。
- `design/{main,browse,export}.pen` 与配对 `.md`，导出的预览；`specs/storage-recovery-w1.md`、`memory/{PLAN,FINISHED,REVIEW,ARCHITECTURE,DESIGN,FUNCTION-REPOSITORY}`：同步批准方案、开发验收证据和真机边界。editor.pen 保持原有工作。

共享工作区另有定稿序号、schema 13、导出后缀、toolsbar、导入确认、对比等并行改动，本次保留，未将它们计作 W1 成果，未整体重置或合并提交。

## 行为与关键决策

库的“多源”是同库可替代位置，用于盘符/挂载点变化，同时使用一个验证位置。所有状态的齿轮均可打开根层同一设置页；顶部“库位置”来自本机 app.db，不以 catalog 读成功为前提。已知离线直接显示登记和最后成功资料，模版输入/保存和重建禁用，原草稿保留。

“定位此库…”与在线“添加位置…”共用选择器和控制器。Rust 先执行现有位置策略、只读身份核对，再幂等登记新位置；旧位置保留，健康会话不切换。离线恢复经统一重连实际打开，不以登记成功冒充 online。空值、取消、身份不符、非目录/非库与权限/策略失败不改目标；超时工作者仅验证，不会迟到后自行登记。迟到选择不再发送新增，迟到 IPC 不写旧弹窗；全局登记成功仍归中央 store。

移除只撤销本机登记，不删除照片、catalog 或库身份；沿用禁行图标、默认二级确认和 Shift 快通道。使用中位置前端禁用并解释，后端在会话锁内再次拒绝，避免移除与 acquire/退场竞争。

探测复用 4 个在途预算、5 秒软等待；超时尚未结束的系统调用继续占位。App 每 2 秒枚举卷，触发合并 200ms，失败后仅按 1/3/10 秒有限重试。同端点暂忙返回 checking，预算释放后按同一策略重试；损坏/权限等真实故障不当拔盘无限重试。没有在线 TTL 或全盘搜库。

新增 availability/session 模块分别承接跨命令的探测调度和跨业务的 catalog 生命周期；继续复用既有 CatalogDb、writer actor、读池、PathForms、目录扫描和身份读取，而非复制一个工作流专用版本。位置异步控制器只管理选择器/弹窗上下文，不保存第二份库事实。没有新顶层依赖、schema 或迁移通道；NAS 留分类/端点提示、状态与租约边界，未写空 provider trait 或引远程 SDK。

命令评估：`repository.reconnect` 与 `repository.locate` 注册到命令面板和文件菜单，默认键均明确为空（低频维护、不占照片操作键），冲突/保留键测试通过；库设置命令按当前工作流目标开放，离线不封死。指定位置的添加/移除和自动探测属于模态/运行内部动作，不另注册全局命令。

## 画稿

main.pen：卡片 `ZYUxs`（深色紧凑）、`o7xAe5`（浅色宽松）；设置在线/离线 `ZhVcs`、浅色宽松 `LuuTs`。browse.pen 的 `a2Oo5C`、export.pen 的 `qEDiW` 同源同步。均通过 Pencil MCP 修改、布局问题遍历和导出图目视检查；未文本解析加密 pen 文件。崔总对修订卡片和设置预览明确回复“按修订稿实施”。预览在 `design/exports/storage-recovery-w1/`。这属于设计定案，不代替 Windows 视觉验收。

## 已验证：单元与 Agent 冒烟

- 最终 `cargo test --workspace`：核心 1204 通过/6 忽略；worker 集成 6 通过；Tauri 库 95 通过/1 忽略；桌面入口 2 通过；Rust 文档 2 通过。`cargo check --workspace` 通过，无本波 unused helper 警告。日志 `/tmp/raybend-storage-location-workspace-tests-final.log`、`/tmp/raybend-storage-location-cargo-check.log`。
- 新真实 catalog 测试模拟中文库目录移动：原路径不存在，新位置核对 ID、保留旧登记、重复添加去重、重开并真实读写；空/相对/非目录/非库/身份错配不创建 catalog。移除测试覆盖大小写/分隔符、使用中拒绝、跨 ID 隔离和幂等。复用 PathForms 的 Unicode/NFC、空/非法/超长路径边界，以及现有元信息、版本、位置策略测试。
- session/db/writer 故障注入覆盖同路径重连、resolve/open 间换库、代次溢出、写线程/关闭故障、旧队列写拒绝、运行中事务如实提交、同库退场等待和其它库继续工作；新增位置事务持有当前会话的竞争测试。
- 导入测试在首个成功项后作废会话，剩余项/来源停止并保留成功结果；新增 `expired_catalog_blocks_export_publication_after_render_and_external_tiff` 使用 8×6 合成图和真实 catalog，渲染后作废原会话，导出与外部 TIFF 均返回 SessionExpired，输出目录无文件、源字节不变（定向用例 0.10s，已纳入最终全套）。
- 最终 `pnpm typecheck`、`pnpm test` 通过：112 个文件级子测试，0 失败；覆盖中央状态合并/事件先于快照/u64 字符串/逐库并发/销毁、位置取消与迟到结果、原因双语和 DTO 文案完整性、命令能力与热键决定。新增 monitor 用例证明同端点 checking/busy 按预算恢复后停止重试。日志 `/tmp/raybend-storage-location-pnpm-tests-final.log`。
- `pnpm lint:colors`、`lint:arch`、`lint:i18n`、`pnpm build` 均通过；构建仍有既有 500 kB chunk 提示。日志 `/tmp/raybend-storage-location-build.log`。首次架构 lint 拦住 UI→api/lib→i18n 依赖，已改为呈现字段适配和 i18n 统一映射，未放宽检查规则。
- `pnpm smoke:ui`：`problems: []`；在线/离线两个卡片均有齿轮，离线可打开位置区、定位可用、模版禁用。`check:import`、`check:browse`、`check:export` 均通过。日志分别为 `/tmp/raybend-storage-location-{ui,import,browse,export}-smoke-final.log`。
- 导出 mock 覆盖：掉线保留图片 DOM；离线齿轮打开设置不读 catalog；取消不登记，选错身份保留路径并本地化提示，关闭后选择器返回不再新增；正确新根恢复同 ID/计数并刷新预览，旧路径保留，当前位置禁移除，非使用中位置确认后只撤销登记，未调用创建库。后台通知目录位置与节点身份断言保持 `shifts: 0`。
- `pnpm debug:win` 最终成套构建及内置 `check:win` 通过：主程序 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`，2026-09-29 01:27:59 CST；worker 01:26:21，协议 raybend-worker-proto-v3；dist 4 个引用资源全部命中。日志 `/tmp/raybend-storage-location-debug-win.log`。未跑 release、未顶版本、未发布/推送/tag。
- `git diff --check` 通过。当前没有 callable pi todo 工具，未另造进度文件；spec 勾选仅表示上述开发/单元/合成冒烟证据。

## 排障与实际限制

原导出目录跳动来自已有数据刷新时插入加载文字行，现改为仅空列表初次加载显示，未放宽零跳动断言。导出队列冒烟的“全选未完成”写死数量因并行定稿数量变化失败，改为核对实际未完成项及已完成项未选中；空预设断言等待表单实际挂载，业务断言保持。少量浏览器启动空白页等待超时发生在业务调用前，保留超时诊断并重跑后通过；未以这些失败冒称通过。

2 秒采样不能保证看到两次采样之间的完整拔插；同步 OS I/O 超时不等于已取消；路径/ID 检查不等于所有文件系统发布都持有原子设备身份句柄。网络/云同步位置分类包含启发式，不宣称识别所有 provider。真实 Windows 内核行为、突然拔盘/读写中断、睡眠、慢盘、双密度/DPI 与视觉、真实照片/色彩均未经人类验证；此轮未以 native GUI 交互代替人类 E2E。

崔总可用最新 debug exe 验证：原盘符重插；改盘符后从离线齿轮进入“定位此库…”选择新库根；选错另一个库必须拒绝；恢复后照片、选择、计数与位置登记正确；正常未找到无卡片红字。人类结果后续记回本记录。来源展开树/最近/勾选恢复属于 W2，任务等待续跑属 W3，副本冲突与主动释放属 W4；首波通过不表示这些已完成。
