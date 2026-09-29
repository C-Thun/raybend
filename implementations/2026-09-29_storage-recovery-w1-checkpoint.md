完成时间：2026-09-29 00:36:34 CST

# 可卸载库 W1：阶段实施证据与离线位置管理计划修订

## 范围与授权

崔总已授权“按首波计划开干”，随后要求暂停以讨论同库多位置。2026-09-29 明确多位置是同库的备用挂载点，并采纳永久设置入口、库位置列表与“定位此库”，要求纳入当前计划并估算完成度。

本轮只修改 `specs/storage-recovery-w1.md`、`memory/PLAN.md`、`memory/REVIEW.md` 和本记录，核对此前实施与测试证据，没有恢复代码实施、启动构建或修改 Pencil 稿。此记录同时补齐暂停前 W1 的阶段实施事实，不表示首波已交付。当前没有可调用的 pi todo 工具，未伪报登记，也未另建执行进度表；spec 勾选项仅记已证实的验收。

## 暂停前的实施范围

- 核心 `store/availability.rs`、`session.rs`、`db.rs`、`pool.rs`、`writer.rs`、`repository.rs`、`location.rs` 与 `error.rs`：结构化连接状态/原因、generation/revision、同库共享写者、实际打开后核对 ID、旧会话作废、排队写执行点闸门、锁外退场、探测单飞与软超时预算、失败保留最后成功计数、catalog 位置策略。原有路径解析已改用同一探测实现。
- 核心 `import/{fsops,runner,sink}.rs`、`export.rs`、`external_editor/mod.rs`、`store/delete.rs`：操作持有原会话/根路径，失效后阻止后续写或文件发布；不自动切到同 ID 副本继续任务。普通输入/单图失败不作整库掉线。
- Tauri `browse.rs`、`db.rs`、`repo.rs`、`import.rs`、`develop.rs`、`export.rs`、`external_editor.rs`、`contract.rs`：生产 catalog 入口使用共享核心会话，根路径来自同一会话；重连/长扫描保持短全局锁与代次对账；队列持有原库租约。
- 前端 `App.tsx`、`features/repositories/{state,monitor}.ts`、browse/import/export store 与 workspace、`BrowsePanels.tsx`、`LibrarySettingsDialog.tsx`、`api/{types,db,dto-contract}.ts/json`：App 唯一库事实、共享卷观察、有限重试、过期快照丢弃、掉线保留展示资料和选择、写能力闸门、恢复时刷新当前预览。设置预览错误不再直接判整库离线。
- `features/commands/catalog.ts`：已接 `repository.reconnect`，默认键明确为空，低频维护通过命令面板与文件菜单可达；相应测试已加入。新增的 `repository.locate` 当前仅规划，尚未实现。
- `design/main.pen`/`browse.pen`/`export.pen` 及同名说明：曾准备 W1 五态草稿与浅色变体。**这些稿尚未定案，仍将离线图标替换齿轮，已被新交互要求取代，恢复时必须修订。** RepositoryCard 视觉反馈尚未完成，正常未找到的旧红字仍未消除。
- `memory/FUNCTION-REPOSITORY.md`、`memory/ARCHITECTURE.md`：已记录实际连接状态、会话与调度契约，并如实保留界面未完成边界。未引新依赖或另造迁移通道；并行聊天的 schema 13/定稿序号/导出后缀与其它工作不算作 W1 成果，保留其现有修改。

新增模块用于承接跨命令的统一探测和跨操作的 catalog 会话生命周期；复用既有 CatalogDb、单写者 actor、读池、PathForms、身份读取与目录扫描。没有为不同工作流创建第二套存储事实或另一个写者。

## 本轮计划修订

手动定位与同库备用位置管理由 W4 提前到 W1。正式行为与新验收归 W1 spec §3.5.1/§5.3；路线只记录波次变化，REVIEW 只登记已采纳方向，不复制第二份规格。W4 保留复杂副本冲突与释放库，W2/W3 范围不变，NAS 协议未选。

位置登记在本机 app.db，离线不能封死设置入口；操作通过 catalog ID 验证同库，不能简单替换盘符或借“添加库”新建 catalog。添加位置与真正恢复在线区分，健康会话不被添加备用位置切换，旧长任务不自动换根续跑。视觉定案仍遵循 AGENTS §5.1；此次方案采纳不被扩大为旧 Pencil 稿获批。

## 已验证（此前 Agent 单元/冒烟证据）

本轮只读取下列已有日志，没有重跑这些命令；结论对应暂停前被测试的代码，不代表其它聊天此后修改已重新验证。

- `cargo test --workspace`：core 1200 通过、6 忽略；worker 集成 6 通过；Tauri 95 通过、1 忽略；桌面 2 通过；文档 2 通过。`cargo check --workspace` 通过，尚有 repo.rs 的 unused online_root 警告。日志 `/tmp/raybend-storage-workspace-tests.log`、`/tmp/raybend-storage-check.log`。
- `pnpm typecheck` 与 `pnpm test` 通过（110 个文件级测试）；定向 Node 无隔离运行覆盖库状态、monitor、命令、browse/export store，112 个测试通过。日志 `/tmp/raybend-storage-typecheck.log`、`/tmp/raybend-storage-pnpm-tests.log`、`/tmp/raybend-storage-frontend-tests.log`。
- 三个 lint 与 `pnpm build` 曾通过；其后仍修改过 ExportWorkspace 的恢复预览逻辑，最终版本须再跑 lint/build。
- `pnpm smoke:ui`：开发 gallery 冒烟 `problems: []`；`pnpm check:import`：合成后端下确认窗取消/确认调用检查通过。日志 `/tmp/raybend-storage-ui-smoke.log`、`/tmp/raybend-storage-import-smoke.log`。
- `pnpm debug:win` 构建与成套产物检查通过：exe 为 2026-09-29 00:22:13 CST，worker 为 00:21:11，协议 v3，嵌入 dist 4 个引用资源命中。该构建早于最后的前端恢复刷新修改，仍需最终重建校验；未据此声明 Windows GUI 功能正确。日志 `/tmp/raybend-storage-win-build.log`。

已勾验收的具体依据：session 的真实 catalog 同路径恢复、打开前后换库/工厂故障/代次溢出/写者故障测试；db 的已排队写拒绝与运行中事务真实提交测试；writer 的关闭故障与并发 join 测试；repository 的不完整计数保留与既有 migration/rebuild 测试；state 的各入口清旧提示、旧 load 与新事件对账测试；命令注册表与默认键测试。未全面完成的组合验收保持未勾。

## 遗留与验证限制

- `pnpm check:export` 未通过：后台 catalog 通知后目录位置断言期望 `shifts: 0`，实际 `shifts: 21`，行/卡片 DOM 身份仍保留。新的离线/恢复预览检查已运行到此断言之前；不能据此宣称导出回归全部通过。日志 `/tmp/raybend-storage-export-smoke.log`。保持该断言，恢复实施后定位原因，不以放宽断言绕过。
- 三个启动脚本的 1×1 PNG mock 曾含无效数据，已换成有效静态合成输入；这是测试输入修复，不是前端图像算法。`check:browse` 尚未在这次修复后完整重跑，最终 mock/契约与恢复保留行为仍需一起收口。
- 新增位置管理、定位 ID 校验的指定库 IPC、移除登记入口、离线设置分区、永久齿轮及 `repository.locate` 均待实现和测试；旧卡片红字反馈仍待修订稿后实施。
- 2 秒观察不能保证捕获两次轮询之间的完整拔插；路径检查也不等于所有文件系统 I/O 都具有原子身份保护。真实 Windows 拔插/换盘符/睡眠/任务中断、DPI/两密度、真实照片与色彩均未经崔总验收。
- W1 尚未整体验收，W2 导入源恢复、W3 等待续跑、W4 副本冲突/释放均未开工。没有发布、推送、tag 或跨聊天派发任务。

本轮文档验证采用限定路径的 `git diff --check` 与文本核对，未因计划修改重复执行程序测试。继续实施须先收到恢复指令；恢复时读取最新工作区，保留并行修改。
