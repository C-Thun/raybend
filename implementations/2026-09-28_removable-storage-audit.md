完成时间：2026-09-28 22:05:25 CST

# 可卸载导入源与可卸载库：实现摸排

本轮是代码审查、既有单测与合成状态探针，未改运行时代码、数据库、画稿或产品规格。下列方案均为建议，未视为崔总已采纳。审查对象包含会话开始时工作区已有的未提交改动，尤其是当天新增的来源卷轮询；未覆盖或回退这些改动。

## 1. 结论与现有基础

库身份与路径分离的底座已成立；主要缺口在**设备变化如何传播到全应用、旧会话如何退场、任务怎样等待并恢复**。优先补这些，再收敛提示表现。仅把红字改成灰字不足以解决重插后需要反复点击的问题。

- `store/repository.rs:225–248` 每次解析都读取真实 `catalog.db` 身份，按最近见过的登记路径查找；同路径不同库、同库多路径已有单测。
- `store/rebuild.rs:172、200–208、293–295` 已拒绝不完整扫描，并在读盘前后复核库身份。不能把“拔盘必然把资产全部标成 missing”当成现有事实。
- `media/watch.rs` 已有非递归、数量受限、去抖的照片目录监听，以及目录身份变化后重装监听的处理；它不是全应用的设备连接状态管理器。
- 导入已有 `Scanner` / `FileOps` / `ImportSink` 注入边界、暂停与取消、临时 `.part` 文件、持久化导入记录，可扩展，不应重写一套导入器。
- 导入源在 import 左列挂载期间每 2 秒枚举卷；展开目录重读，窗口获焦重读已展开分支。这是已实现的基本恢复手段。

## 2. 库：优先封口的位置

### A. 旧数据库连接没有完整退场路径（高优先级，代码风险）

`src-tauri/src/browse.rs:58–85` 的 `with_catalog` 先拿旧连接锁，再 `resolve_root(...)?`。解析为离线时直接返回，**没有清除 `open` 或 `live_watch`**。同库同路径恢复时，失效判据只有库 ID、路径、目录存在性，可能继续使用拔盘前的读连接池与写线程；执行闭包报 I/O 错也不会清掉连接。

`repository_remount` 只更新登记状态和返回视图，不重置这个会话。目录移动的现有单测只验证无长期连接的 resolver，不能替代 Windows 真拔盘后句柄的验证。

已有缓解：`store/pool.rs:142–151、199` 在读连接归还时执行 `PRAGMA user_version`，不健康的连接会丢弃。但这不是整个库会话的失效通知，也不覆盖专属写连接、监听或尚未借出的旧连接，因此不能将它当成重挂载闭环。

建议：把会话管理收进核心库，外壳只接线。每次连接有 generation；确认断开、底层连接故障、主动释放时使旧 generation 作废，停止新工作并释放读池、写者和监听。恢复时重新打开并核对期望库 ID，再发布新状态。不能只比较根路径字符串。长任务也必须受同一会话约束。

### B. “中央状态”实际只覆盖 import（已确认缺口）

- `src/workspaces/import/store.ts:229` 创建自己的 `createRepositoryState`。
- `src/workspaces/browse/BrowseWorkspace.tsx:163、536、578` 另有列表信号、加载与重连。
- `src/workspaces/export/ExportWorkspace.tsx:83、182、504` 又有独立列表与重连。
- `src/App.tsx:651` 的全局库设置弹窗未传 `repository` / `onStale`，读库失败不能同步降级卡片；import 那份接了。
- 浏览重连只更新列表，未显式刷新当前查询；`browse/store.ts` 同库 `setRepository` 又直接返回。目录树可能因 root 改变触发一部分刷新，但不构成完整的恢复契约。

建议：App 持有唯一 `RepositoryStateStore`，三个工作流、设置、命令可用性共同订阅。由后端发布带 generation 的可用性事件；前端只丢弃过期结果，不自行猜“某句报错是否代表拔盘”。重连完成刷新当前目录与必要读模型，保留选中、筛选和滚动语义。

### C. 红字、残留提示与乱序返回（两项合成探针已复现）

`src/components/ui/RepositoryCard.tsx:173–179` 把 `not_found` 和真实异常都渲染为 `text-danger`，并新增一行撑高卡片。`repositories/state.ts:116–163` 存在以下问题：

1. `load()` / `upsert()` 使库在线时不会清掉旧重连提示，只有 `remount()` 自己清理。
2. 旧 `load()` 的离线结果可以覆盖刚完成的重连在线结果，没有 generation/ticket 防护。
3. `remountingId` 是全局单值，一个慢库探测会挡住其它库；发起新一次探测时旧失败提示仍在。
4. browse/export 未把 `remounting` / `remountError` 传入共享卡片；三处反馈不一致。

建议交互：离线保持既有灰化图标；点击时原位转圈；没找到用中性短反馈“设备尚未连接”，原因和尝试位置放悬停/详情，避免红字撑高卡片；成功清提示。红色留给损坏、迁移失败等需要处置的问题。权限、只读、超时需要明确区分，不能都写“设备尚未连接”。界面实现前按项目纪律更新 Pencil 稿。

### D. 单库探测会占用全局数据库锁，且夹带全库工作（已确认结构）

`src-tauri/src/db.rs:47–57` 的 `DbState::with` 在整个闭包内持有互斥锁。`repository_remount` 在该锁内通过 app.db 写事务逐路径做真实磁盘探测（`repo.rs:277` → `repository.rs:413`），随后调用 `views` 再查**所有库**。`views:116` 还会为计数未知的库递归扫盘，并在 app.db 写线程里做计数。

因此一个休眠盘／慢路径就可能推迟其它 app.db 操作；即使目标库已经找到，别的库的探测也可能延迟按钮结束。当前没有后端时限或按存储端点隔离。前端 `withTimeout` 只停止等待，不能终止已经进入阻塞 I/O 的线程。

建议：先短锁取登记数据，锁外做有界探测，短事务写结果；重连只返回目标库。状态探测与照片计数分离，计数复用现有有界扫描基础。每个端点限制同时在途请求，慢端点不挤满全局线程；真实同步系统调用的取消能力需要单独验证，不能把 Promise 超时写成“已取消 I/O”。

### E. 错误被压成布尔，位置安全检查尚未接到入口（已确认缺口）

`path_holds_repository` 用 `is_file` + `is_ok_and` 将不存在、权限不足、数据库损坏、I/O 失败、ID 不符全部压成 false；`RepositoryView.online` 也不代表可写或 schema 可用。设置弹窗则把任意设置读取/预览失败都调用 `onStale`。

另：`store/location.rs:328` 已有 `catalog_suitability`，但本轮搜索只有定义和测试，`repository_create` / `CatalogDb::open` / `create` 未调用。项目已规定 catalog 不放网络盘／云同步目录，实际命令入口尚未形成这道闸门。

建议：保留每条位置的结构化探测结果，区分 `offline`、`timeout`、`access_denied`、`identity_mismatch`、`catalog_invalid`、`schema_too_new`；“身份存在”与 `canRead/canWrite` 分开。对网络来源允许读，对本期本地 catalog 的创建/登记/打开执行统一位置策略，复用 `location`，不复制分类器。

### F. 新盘符与同 ID 副本仍缺明确恢复策略（已确认范围限制）

resolver 只查已登记的路径。D: 变 E: 且 E: 从未登记过时，重复点击不会自己发现；可以通过现有添加库入口选择新目录，它会按 catalog ID 登记新路径，但入口不直观。

建议：离线详情提供“重新定位”，只验所选目录，不全盘搜索。来源定位可增补可选的设备标识 + 卷内相对路径，设备标识只缩小候选范围，最终仍核对 catalog ID。多个可写副本持同一 ID 同时在线时，不宜静默切换写入位置；需要固定本次活动位置或提示选择，不能声称 ID 相同就等于内容始终一致。该冲突策略为待决产品语义。

### G. 正常释放与意外掉盘应各有闭环（建议）

本轮未找到统一的“释放库占用”动作。可以先提供应用级释放：阻止新任务 → 结清可结清的写入／明确未完成工作 → 关闭连接与监听 → 库保留在登记表。它不同于忘记库，也不同于请求操作系统弹出设备；不要把应用已经释放误报成磁盘已安全弹出。真掉盘时不承诺无法完成的 flush 成功。

若新增“重新定位／释放库”入口，须同步接统一命令表。建议默认键明确为空（低频操作，命令面板可达，避免占常用键）；自动探测不是用户命令。本轮没有新增运行时功能，因此不修改注册表。

## 3. 导入源与执行任务

### H. 卷列表刷新尚未贯通目录、最近、勾选集合与图片区

- `import/store.ts:425–460` 轮询只换卷列表，不刷新已展开目录、已勾选来源或当前图片区。
- `DirTree.tsx:69–80` 的树另存 children/expanded；同盘符回来时可能先显示旧内容，只有重新展开／窗口获焦才重读。卷枚举只有路径和类型，没有连接代次。
- `photo-grid/store.ts:439` 对同一路径直接返回。插回后再选已选目录虽然会检查 `is_dir`，不会因此重新载图；这与“点几下才回来”的现象相符，但未用真机复现，不能据此断言就是该次故障的唯一原因。
- `ImportWorkspace.tsx:200–206` 的 `reloadRecent()` 与 `hydratePreferences()` 并行；后者立刻检查尚可能为空的最近列表，没有在最近列表返回后补查。`LeftColumn` 的 focus 只调用 `reloadVolumes`，没有调用最近可用性刷新。当天旧实施记录写“最近在获焦时识别”的口径与当前接线不一致，历史记录保留，本轮指出差异。
- 已勾选来源没有持续在线状态；导入按钮检查来源数量与库 online，源盘掉线后仍可开始，随后才失败。

建议：卷变化输出 added/removed/reconnected 差量，只重新探测受影响的最近项、勾选项和当前来源；保留意图与勾选，标为暂不可用。当前目录恢复时有明确 reload 入口，同时避免自动清空已有选择；未知/慢来源不应默认为确认在线。同一请求合并与过期结果防护应覆盖 focus、定时器、展开三个入口。

### I. 导入中断还不是“等待连接后继续”

`src-tauri/src/import.rs:430–459` 在后台线程持有一个 CatalogDb 和固定 RepoFs.root 贯穿整批；没有任务执行期的连接 generation 或期望库 ID 复核。手动暂停后继续也只是改变 Control 原子状态。

`import/runner.rs:376、542–558` 复制失败计为单项失败后继续；sink 失败则整来源失败后尝试其它来源。设备断开没有单独分类，不能自动等待设备。`import/fsops.rs:139–163` 的 FsScanner 只传回 files/skipped/cancelled，底层 ScanOutcome.problems 丢失，扫描期间失联或目录读取失败可能只留下部分扫描结果而缺少完整性提示。

建议：来源掉线只挂起受影响来源，目标库掉线则挂起该库整个写批次；恢复时重新校验源身份与目标 catalog ID，重新获取会话，校验已复制文件再续。目标改变不能因为盘符相同就继续写。已有续跑仅按源绝对路径与文件大小认领 pending，面对换盘符或同尺寸替换文件还不足，需要结合身份/指纹及重新规划。持久化结构变化必须走现有迁移。

另有较小的数据质量缺口：`count_dir_on_disk` 把读取失败算 0；`count_library_on_disk` 无完整性结果。导入结束无论成功失败都会重算计数，掉盘可能把部分旧计数写成零。应保留最后成功计数并带 observedAt，失联与真空目录分开。它是派生计数失真风险，不等于照片丢失。

## 4. NAS 预留：尽量小的接口边界（候选，不是已实现 API）

NAS 作导入源与 NAS 作库分开。现有 `VolumeKind::Network` 可显示映射网络盘，目录扫描可接本地可访问路径；这不等于已经提供未映射 UNC 的来源管理、认证、网络发现、连接超时或重连体验。现行“catalog 禁放网络盘”的约束继续有效。

建议先围绕本地可卸载盘封口三组数据与调用：

```text
StorageLocation
  locator               # 本地路径 / 后续 provider 专有定位符
  kind                  # 沿用现有 VolumeKind，类型不等于可用状态
  deviceHint?           # 可选、仅用于发现候选，不替代库身份
  generation            # 此次连接的代次

Availability
  state                 # unknown / checking / online / offline / unavailable
  reason?               # 稳定原因码，不用中文异常串做判断
  observedAt
  capabilities          # canRead / canWrite / canWatch / canResume 等按事实报告

RepositorySession
  repositoryId + location + generation
  acquire(expectedId, purpose)
  invalidate(reason)
  release()
```

- probe 在 Rust 核心层，接受时限/取消上下文；返回状态事实和身份，不携带界面文案。前端订阅快照与变化事件。
- 在既有 `Scanner` / `FileOps` 后扩展网络来源。暂不引 SMB/SFTP/云 SDK，不造包揽所有文件系统操作的巨型抽象。需要本地路径的 raw worker 将来可经 provider 的本地暂存接口取得内容，不让前端接管图像 I/O。
- **远程库**必须有独立的 catalog 访问边界。候选是本地 catalog + 远程媒体，或服务端拥有 catalog、桌面走服务 API；两者的身份、冲突、多机写入语义都需另定，不能把 NAS 路径直接塞给现有 CatalogDb 当作支持完成。此处只留接缝，不改变当前库随盘迁移的布局。
- 设备变化可先复用现有轻量卷枚举，之后再替换为平台事件源；无变化不递归查库。探测频率、退避与最长等待需在实施单元中给出具体数值供崔总定案，不在本轮擅设网络时限。

建议首个工作单元只处理“统一会话生命周期 + 统一状态 + 重连反馈”，让本地可卸载库真正形成恢复闭环；任务断线续跑与远程协议实现后续单独成单元。这里是范围建议，不是第二份任务进度表。

## 5. 验证、落点与遗留

已验证（冒烟/单测）：

- `cargo test -p raybend --lib store::repository::tests -- --test-threads=2`：26 条通过；测试执行 0.33s，首次包含增量编译共约 1m26s。
- `cargo test -p raybend --lib store::rebuild::tests -- --test-threads=2`：13 条通过，测试执行 0.35s。
- `node --test src/features/repositories/state.test.ts src/features/dir-tree/store.test.ts src/workspaces/import/store.test.ts`：3 个测试文件通过，报告约 0.45s。使用项目 `pnpm test` 中同一 Node test runner；pnpm 启动未在有界等待内返回，绕过包管理器启动，仅直接执行已有测试，无安装或锁文件改动。不能将此写成 `pnpm test` 全量通过。
- 合成状态探针检查 `load` 恢复不清旧提示、旧列表覆盖新重连结果，见本轮工具输出；仅访问内存假 API。
- 本轮修改的 `memory/REVIEW.md` / `memory/FUTURE.md` 定向 `git diff --check` 通过；全工作区检查另报已有 `design/export.md:406` 尾部空行，未改动该文件。

未经真机验证：Windows 安全弹出/突然拔盘/同盘符重插/改盘符、读写期间断开、睡眠唤醒、慢网络与权限变化、卡片视觉及恢复后的选中/滚动。现有单测通过不能宣称这些 E2E 已通过，也未据代码风险宣称发生过数据库损坏。

本轮涉及文件：本报告、`memory/REVIEW.md`（R5 结论索引）、`memory/FUTURE.md`（NAS 候选接口登记）、`todos/2026-09-28-removable-storage-hardening.md`（原话与后续实施输入）。本会话未提供 pi `todo` 工具，未另造任务状态表或伪报已登记。后续开工从需求原文整理单工作单元 spec。

未进行全量构建：本轮未改运行时代码，既有工作区还有其它工作的未提交改动。库会话风险需在实施时补真实 CatalogDb 生命周期的合成故障测试；现有 resolver 改名测试不能充当这一覆盖。
