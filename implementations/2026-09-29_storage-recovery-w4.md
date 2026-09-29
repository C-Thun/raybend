完成时间：2026-09-29 03:11:21 CST（UTC+08:00）。

# 可卸载存储 W4：明确副本选择、主动释放与四波交付

崔总已批准 W3/W4 补充稿“按补充稿实施”，并授权四波连续完成、最后一次统一真机验收。本轮开发、单元、合成冒烟与 Windows 启动/产物冒烟收口；不把这些证据当真机拔插、照片和视觉验收。

## 改动范围

核心 `crates/raybend/src/store/{availability,session,file_id,location,path_semantics}.rs`、`error.rs`、`export/jobs.rs`；Tauri `repo.rs`、`browse.rs`、`import.rs`、`export.rs`、`editor.rs`、`develop.rs`、`external_editor.rs`、`thumbs.rs`、`issues.rs`、`lib.rs`。

前端 `App.tsx`、`api/{db,types,dto-contract}`、`components/ui/RepositoryCard`、`features/repositories/{state,monitor,LibrarySettingsDialog,RepositoryList}`、`features/commands/catalog`、`workspaces/import/store`、`lib/export-model` 与双语文案/对应单测；保存的导入/浏览/导出冒烟脚本。文档为 W3/W4 specs、main.pen 配对说明、验收指南和相关 memory。共享工作区其它改动原样保留，未整体重置或打包提交。

## 选择与释放

解析所有登记位置，以 catalog ID 判断库、FileId 判断物理实体；同一实体的路径为别名，同 ID 不同实体同时发现时返回 multiple_locations。位置行明确确认“使用此位置…”后将路径与实体记入 app.db 现有 settings 的版本化 key。选定实体缺席不能自动换其它副本；添加位置不切走健康会话。本库仍有未完成导入/导出或活动任务时拒绝切换，避免旧任务转写新根。

释放复用 CatalogSessions 的每库屏障。begin_release 的单一所有者先作废原会话、拒绝新活动许可及发布，取消本库导入、终止本库导出待执行意图，退出 watcher，等正在运行的许可归零，再关闭读池/写者；登记、照片与已完成结果保留，其它库继续。重复释放不能第二次结束生命周期；释放中手动重连返回忙，完成后明确重连才解除屏障。释放状态仅本次应用会话有效。

短数据库操作沿用原排干机制；解码/预览/定稿缓存/导出/外部编辑等后台工作在同一协调器持有 TaskPermit。编辑缓存仅保存 TaskAccess，新工作再申请短期许可，避免空闲缓存永久占位。路径读图入口按登记库识别，释放后不能作为普通来源绕过屏障；定稿/缓存写前再次检查捕获租约和根。旧任务失败不能覆盖新的 released/releasing 状态。

不另造数据库生命周期或迁移；本轮没有新增 schema、顶层依赖、NAS SDK 或 OS 弹出实现。Win32 FileId 读取目录复用原读取接口，只加目录句柄所需标志。

## 界面与命令

按 main.pen RQzd5 批准稿接入原设置弹窗：位置行明确选择、释放确认、正在释放/已释放中性说明。全部状态保留齿轮，释放中重连禁用；已释放复用位置区唯一重新连接入口，避免再写第二个相同动作。确认按钮仍取消左、确认右。

`repository.release` 注册 Ctrl+K 命令面板和文件菜单，defaultKey 明确留空（低频且需要确认，避免误触）。重新连接沿用原命令、无默认键；指定位置选择是带参数的模态操作，不加全局热键。自动探测与等待不是新增命令。

## 最终验证

- `cargo test --workspace`：核心 1219 通过/6 忽略、worker 集成 6 通过、外壳 95 通过/1 忽略、桌面入口 2 通过、文档 2 通过，零失败；核心 22.12 秒。`cargo check --workspace` 通过。日志 `/tmp/raybend-storage-delivery-rust-fixed.log`、`/tmp/raybend-storage-delivery-check-fixed.log`。
- 单元覆盖多实体冲突/同实体别名/选中实体缺席、同 ID 副本不能接替任务、活动任务拒绝切换、释放排干/拒绝新任务/其它库不受影响/重复释放所有者/明确恢复，以及导出等待让出槽、停止意图不复活。原路径 Unicode/NFC/大小写/超长、代次与竞争边界继续覆盖。
- `pnpm typecheck`、`pnpm test`：112 个文件级测试通过、0 失败（2.30 秒）；包含 released 自动观察保护、双语 DTO/反馈/命令能力和浏览器响应式探测回路回归。colors/arch/i18n lint、build、`git diff --check` 通过；保留既有 chunk 提示。
- 最终 `check:import`、`check:export` 和单独复跑 `check:browse` 合成冒烟通过。日志 `/tmp/raybend-storage-delivery-import-fixed.log`、`/tmp/raybend-storage-delivery-export-fixed.log`、`/tmp/raybend-storage-delivery-browse-fixed-alone.log`。浏览并行复跑曾在滑块拖动后失去后续视图，保留失败日志；相同代码/脚本单独完整复跑通过，未删除断言或放宽质量门。
- 最终 `pnpm debug:win` 与内置 `check:win` 通过：主程序 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`，2026-09-29 03:07:01 CST；同目录 worker 03:05:06，协议 raybend-worker-proto-v3；dist 四个引用资源全部命中。日志 `/tmp/raybend-storage-delivery-win-fixed.log`。未跑 release、未顶版本、未推送/发布/tag。
- 最终 Windows 原生进程启动冒烟：主窗口建立、Responding=true；日志中本地库不再被拒绝，约两分钟后仍为 9 行，仅登记离线库的有限重试，没有 panic 或无限探测。日志 `/tmp/raybend-storage-delivery-startup-fixed.log`；启动测试进程随后由 Agent 结束。这不证明 GUI/照片功能正确。

## 启动冒烟发现与修补

首轮原生启动暴露了 Rust canonicalize 的本地路径 `\\?\C:\…` 被 is_unc 当网络盘的问题。已在共享 path_semantics 中新增仅用于位置分类的命名空间适配；本地盘符/卷命名空间与扩展 UNC 区分，UNC 和云目录禁用保持，原始 I/O 路径及已持久化 normalized/folded 键不变。新增纯路径、Unicode、长路径、扩展 UNC/云目录回归；Windows 临时目录用例补 canonicalize 最终检查，原生启动也证明已登记本地库可连接。

同次启动发现工作流 effect 调用 monitor.request 时读到库状态，探测回写又触发 effect，绕过有限重试。request 现在在共享 monitor 内 untrack；浏览器条件子进程单测证明状态回写不触发请求，而工作流变化仍触发。迟到设置读取错误及导入侧 stale 回调改为自动重连，不解除 released 屏障。修补后重新跑全套单测、质量门、合成冒烟和完整 Windows 构建，并复查原生日志。

## 遗留边界与统一交付

四波真实拔插、换盘符、Windows 句柄退场、睡眠/慢盘、照片/色彩、DPI/观感和性能体感由崔总确认；步骤为 `docs/removable-storage-acceptance.md`，只需同一份最终程序。释放不等于 OS 弹出，不关闭应用外部句柄；阻塞的 OS I/O 无法承诺立即退出。导出队列不跨进程恢复，可靠实体身份未知时等待不冒认。

NAS 接缝已有位置/端点分类、状态、ProbeBudget、CatalogSessions、Scanner/FileOps/StorageRecovery；实际协议/认证、远程 catalog、多机写入仍待决，现行本地 catalog 边界保持。当前无 callable pi todo 工具，未另造进度文件；spec 勾选仅表示以上开发与冒烟。真机结果后续由 Agent 验证并记回本记录。
