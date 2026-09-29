完成时间：2026-09-29 03:09:32 CST（UTC+08:00）。

# 可卸载存储 W3：长任务等待与安全续跑

按崔总“一口气做完所有波次”的授权实施，W3/W4 补充稿已明确批准“按补充稿实施”。本记录表示开发、单元和 Agent 合成冒烟；真实 Windows 拔插、照片与观感依约四波一起验。

## 范围与复用

核心改动在 `crates/raybend/src/import/{runner,progress,sink,fsops}.rs`、`store/session.rs`、`export/jobs.rs`、`fs_atomic.rs`；外壳接入 `src-tauri/src/{import,export}.rs`。前端为 `features/import/{store,ImportProgressDialog}`、`lib/export-model`、`api/{types,export}`、`workspaces/export` 和双语文案，测试随模块交付。示例中的 Deps 同步新可选恢复适配参数。

沿用现有 runner、CatalogSink、RepoFs、导出 Engine、单写者/读池与 W1 的会话闸门。恢复需要双方设备状态时新增 StorageRecovery 适配入口；旧单一租约不足以表达“旧连接作废后核对原实体再取得新连接”，因此同一 session 模块提供 TaskCatalog，没有另造调度、迁移或依赖框架。

## 关键行为

批次开始即捕获所有来源目录的 FileId 与目标 catalog ID/实体 FileId。未知身份在失联后不能自动冒认设备；TaskCatalog 安装新健康会话时要求原实体相同，同 ID 副本不接替写入。

来源等待仅挂起该来源，队列继续其它来源；目标等待暂停向该目标写入。每个来源保留扫描/规划结果、序号和文件游标，已成功计数不重复增加；等待期间取消可退出，尚未开始的来源不虚造失败 run。中断或错误扫描不规划半份清单，进度阶段取尚未结束的来源，不因另一来源完成而显示整批完成。

CatalogSink 失败提交保留操作，成功事务后才清空；规划重放幂等，序号合并不回退。运行中批次的成功项同步实际 import_items 痕迹，重启同源重发可跳过已登记成功，即使“避免重复导入”关闭；正常已完成批次仍允许按既有开关再导入。

pending 成品核对完整字节内容和来源事实，不能仅凭相同大小接纳。复制检查来源文件大小/修改时间/实体，先写 part、同步后复用共享不覆盖发布函数；目标被别人创建时不覆盖。恢复时根来自新健康会话。

导出 waiting 为非终态并让出执行槽；恢复核对原 catalog、输出设备锚点与捕获定稿。停止预设不自动重新开启，移除/重置等待项不复活。已成功发布如实保留，不因紧接着离线重做。队列仍只在进程内，未声称跨重启恢复导出队列。

批准的 main.pen bJD0w 接入既有进度模态：中性等待说明、成功数/阶段保留、取消在左、重新检查在右。重新检查仍走身份验证。模态继续/重查/取消为上下文操作，不新增全局命令或默认热键；导出等待自动适配原队列开关。NAS 继续复用 Scanner/FileOps/StorageRecovery、状态与探测预算，实际协议和远程 catalog 未进入本波。

## 已验证（单元与冒烟）

- 最新 `cargo test --workspace`：核心 1219 通过/6 忽略，worker 集成 6 通过，外壳 95 通过/1 忽略，桌面入口 2 通过，文档 2 通过；零失败。核心执行 22.12 秒。日志 `/tmp/raybend-storage-delivery-rust-fixed.log`。
- 真实临时 catalog 测试模拟目标改名失联、buffer 留存、恢复原实体、继续写入；覆盖同 ID 副本拒绝、来源轮转/安全游标、等待取消、失败提交保留、规划幂等/序号不回退、重启成功项筛选，以及同大小不同内容、扫描后来源改变。释放与队列竞争测试见 W4。
- 最新 `pnpm typecheck`、`pnpm test`：112 个文件级测试通过/0 失败，2.30 秒；等待非终态、终态后迟到进度拒绝和 DTO 契约通过。日志 `/tmp/raybend-storage-delivery-front-check.log`。
- colors/arch/i18n lint、build、`cargo check --workspace` 通过；现有大于 500 kB chunk 提示保留。最终 Windows 主程序 03:07:01、worker 03:05:06（2026-09-29 CST），dist 四个引用资源全部命中，worker 协议 v3。日志 `/tmp/raybend-storage-delivery-win-fixed.log`、`/tmp/raybend-storage-delivery-check-fixed.log`。
- 保存的生产预览合成 `check:import`、`check:export` 再次通过，导入包括来源拔插、等待文案/按钮、成功数、完成与释放确认；日志 `/tmp/raybend-storage-delivery-{import,export}-fixed.log`。这些是合成数据接线检查。

## 排障与限制

冒烟替身曾将 import_status 返回 null，违背接口完整快照契约；已改真实 DTO 并保存等待/完成事件夹具。导出位置行新动作使原“首个 button”断言选错按钮，已改按移除 aria-label 定位；空预设第二页面的启动恢复与点击流程竞争，改为等待目标工作区实际挂载，未放宽业务断言。Windows 首轮资源核对拦住开发期间旧资源，已基于稳定源码重新成套构建。最终 Windows 启动发现的扩展路径与探测回路修补详见 W4 记录。

原设备无法证明、Windows 内核 I/O 未返回时不能承诺立即续跑/取消；可靠身份缺失可取消后重新发起。路径/身份检查仍不等于每次文件发布都持有原子设备身份句柄。NAS 协议、认证、远程 catalog、多机写入未实现。真机四波验收统一见 `docs/removable-storage-acceptance.md`；崔总结果后续记回本记录。
