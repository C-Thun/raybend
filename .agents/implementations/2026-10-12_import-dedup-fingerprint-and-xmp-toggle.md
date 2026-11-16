# 导入判重（内容指纹 + 整组消解）与「导入 xmp 配置」开关

完成时间：2026-10-12 14:49:04
规格：`specs/import-dedup-fingerprint.md`（新）、`specs/xmp-sidecar.md` §7/§9/§12（修订）

## 一、起因

崔总做「重复导入」测试，导入第 32 张时**报错并中断**：
`数据库错误：UNIQUE constraint failed: asset_files.rel_path_folded`。

代码级根因（两条，都在同一条链上）：

1. **判重只认「源是谁」**：`plan::KnownSources` 比的是**源文件身份**（`(卷序列号, FileId128)`）
   与「源绝对路径 + 大小 + mtime」。同一批照片换个目录再导（拷贝 = 新路径 + 新 FileId）
   一条也匹配不上 → 全部重导。
2. **冲突判定只看盘**：`resolve_conflict` 只问 `FsProbe`（盘），不问库。库里有一行、
   盘上没有（标了 missing / 被手动删 / **文件监听差分刚登记**）时规划当成空位 → 登记时撞唯一索引。
   而 `Op::Register` 的 INSERT 在**一个批事务**里 → **一条撞库 = 整批回滚**、run 停在 `running`。

实测印证：库里那几条 `asset_files` 行**没有对应 `import_items` 行**、时间戳落在导入开始后 2 秒内
—— 是 `store/rebuild.rs::rescan_scope`（文件监听 → 差分同步）抢在导入前面登记的。

## 二、改了什么

### A. 判重（`specs/import-dedup-fingerprint.md`）

* **范围**：指纹只用于「导入时 × **同一目标目录层**」的防重确认；不做跨目录判重、
  不作库内相片身份、不加唯一索引（崔总 2026-10-12 口径）。
* **判定（逐成员，命中即停）**：目标路径 Free → 直接导入（**不读指纹**）；
  被占 → 大小先筛 → 大小相同才比指纹 → 相同 `Skip`（原因写明库内路径）/ 不同或无核对 `Conflict`。
* **改名（整组）**：组内任一要导入的成员 Conflict → **整组共用一个后缀**
  （`_01`，复用全项目唯一口径 `collision_stem`），保住「位图与同组 RAW 同名」这条配对不变量
  —— 改动前是**逐成员**消解，会出现「jpg 变 `_01`、raw 不变」→ 库侧按主体配对会把同一张照片拆成两个格子。
* **指纹口径**（`media::fingerprint`，新模块）：位图 `sha256-full`；RAW `sha256-sample`
  = `sha256(size ‖ head 1 MiB ‖ mid 1 MiB ‖ tail 1 MiB)`（≤3 MiB 取整文件）。
  不做 RAW 全文哈希（单张 10–25 MB，采样只读 3 MiB；也不需要过 raw worker 解码）。
  判等需**口径 + hex + 大小**三条同时成立。
* **建立与回填**：导入复制后对**库内副本**算一次（落 `asset_files.content_hash(_kind)`）；
  库内老行没有指纹时**在命中占用时**现算并随批回填（`Op::SetContentHash`，幂等：已有不覆盖）。
* **跳过不消耗序号**：整组都被内容判重跳过时把取走的号还回去（`Sequences::restore`），
  免得一次重复导入把库里的编号往后推。
* **续跑整组一致**（§11.3）：上次崩溃留下的「已复制未登记」痕迹（`adopt`）现在按**组**读 ——
  组里任一成员有上次的名字就用它，全组沿用同一个名字与组目录（RAW 的 `_RAW/` 折回上一层）；
  不然「只落了 RAW、计数器又往后走了」时，RAW 会被重新编号、与位图配对断掉。
* **关闭开关**：`avoid_duplicates = false` 时整条内容判重不生效（不读指纹），只保留重名让位。

### B. 冲突兜底（不中断整批）

* 规划期：`KnownTargets` 升级为「路径 → 库内那一行（大小 + 已有指纹）」，冲突判定**同时吃盘与库**。
* 登记期（`apply(Op::Register)`）：
  * 路径空 → 真插入（整段包在 **savepoint** 里，撞 `UNIQUE` 就 `ROLLBACK TO` 再走兜底）；
  * 已被登记且**角色 + 大小一致** → **认领**（不插新行，补来源列/身份/时间，清缺失标记，补指纹）；
  * 已被登记但**不一致** → **只把这一条标 failed**（原因「库内已有同名路径、内容不同」），不碰库、不中断整批。
* `CatalogSink::register` 推 op 前先查一次（索引查询）→ 「大小不同」这种真冲突在推 op 时就拒掉。
* `Op::Register` 一步把条目写成 `imported`，执行器不再补 `Mark`（`specs/M1-6.md` §3.3 已写明
  「`copied` 与 `imported` 之间没有需要区分的时刻」；两步写会让兜底标的 `failed` 被后一条 `Mark` 盖掉）。

### C. 「导入 xmp 配置」开关 + 完整性门禁（`specs/xmp-sidecar.md` §7.1）

* 现状核对：**导入侧的 sidecar 采纳本来就已接好**（`src-tauri/src/import.rs` 在 `run_batch` 之后
  取 `sink.take_new_assets()`，用**源文件绝对路径**探 sidecar 并采纳）—— 缺的是
  ① 没有开关；② 没有「完整导入」门禁（原来**逐文件**采纳，拆开的情况也会采纳）。
* 新增：右列底部「避免重复导入」**下方**的勾选 **「导入 xmp 配置」**
  （设备级 `app.db` 键 `import.import_xmp`，默认**开**；`SETTING_KEYS.importXmp`）。
* 门禁：`import/plan.rs` 给每个条目算出**组级**标志 `PlannedItem.complete`
  = 组内每个成员都真的导入了（没跳过、没失败）**且没改名**；经 `NewAsset.complete` 传到外壳，
  只有 `import_xmp && complete` 的资产才采纳 sidecar。重扫/重建路径不受这两道门影响。

## 三、涉及文件

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/media/fingerprint.rs` | **新增**：口径/算法（全文 / 采样）+ 9 条单测 |
| `crates/raybend/src/store/migrations/catalog_0021_content_hash.sql` | **新增**：`content_hash` / `content_hash_kind` 两列 |
| `crates/raybend/src/store/migration.rs` | 登记 v21；两条迁移测试的断言改为对照迁移表（不再逐次改数） |
| `crates/raybend/src/store/assets.rs` | `insert_file` 带指纹；`set_content_hash`（幂等）；`find_file_by_rel_path` 带指纹 |
| `crates/raybend/src/store/rebuild.rs` | `insert_file` 调用补 `None`（重扫不建指纹） |
| `crates/raybend/src/import/plan.rs` | `KnownTargets` 升级、`ContentProbe`、逐成员判定 + 整组消解、`Sequences::restore`、`complete`、`backfilled` |
| `crates/raybend/src/import/fsops.rs` | `FileOps::fingerprint_of`（RepoFs 读盘 / MemoryFs 内存） |
| `crates/raybend/src/import/runner.rs` | `ImportSink::{known_targets, set_content_hash}`、`FsContentProbe`、复制后取指纹、回填、`complete` 透传 |
| `crates/raybend/src/import/sink.rs` | 认领 / 单条失败兜底、savepoint、`Op::SetContentHash`、`NewAsset.complete` |
| `crates/raybend/src/error.rs` | 新 `Error::ContentConflict`（库内同名路径、内容不同） |
| `crates/raybend/src/diagnostics/bundle.rs` | **顺带修**：崩溃记录「取最新一份」的排序加目录名兜底（见 §五） |
| `src-tauri/src/import.rs` | 命令参数 `import_xmp`；采纳加两道门；`run_import_thread` 多一个参数（带理由的 `allow`） |
| `src/api/{db,import}.ts`、`src/features/import/store.ts`、`src/workspaces/import/{store.ts,ImportWorkspace.tsx}` | 偏好键、信号、持久化、请求透传 |
| `src/features/repositories/RepositoryFooter.tsx` | 新勾选项（在「避免重复导入」下面） |
| `src/i18n/{zh-CN,en-US}.ts` | `repo.import_xmp` |
| `design/main.md` | §3.3 记新勾选项（画稿 `.pen` 未补，见 §五） |

## 四、验证

* `cargo test -p raybend --lib`：**1572 通过 / 0 失败**（11 ignored）——含本次新增
  （指纹 9 条、plan 内容判重与整组消解 + 续跑一致 13 条、sink 认领/兜底 4 条）。
* `cargo test --workspace --exclude raybend-crashpad`：全绿（1572 + src-tauri 146）。
* `cargo check --workspace`：通过。
* `cargo clippy -p raybend -p raybend-desktop --all-targets`：我改过的文件**无新增告警**
  （剩下的 `fsops.rs:552`、`sink.rs:1479` 是既有告警，已核对不在本次 diff 里）。
* `pnpm typecheck` / `pnpm test`（1297 通过）/ `pnpm lint:colors` / `pnpm lint:arch` / `pnpm lint:i18n`：全绿。
* 手工复核（代码级）：`src-tauri/src/contract.rs` 的命令契约测试不涉及新增参数，未受影响。

### 未经人类验证（需要真机 E2E）

* 真机上「同一批照片换个目录再导 → 全部跳过且弹窗写明内容一致」这条主场景；
* 大库（万张级）下按需建立指纹的耗时体感（当前设计只在**路径被占**时才读盘）；
* 「导入 xmp 配置」勾选在真机上的复原效果（编辑配置/评级/标签是否如预期回到新资产）。

## 五、遗留与说明

1. **`crates/raybend-crashpad` 的测试在 HEAD 就编不过**（`main.rs:244`
   `tempfile::tempdir().unwrap_or_else(|_| std::env::temp_dir().into())` 类型不匹配）
   —— 与本轮改动无关，**未动**；`cargo test --workspace` 因此需 `--exclude raybend-crashpad`。
   已报告给崔总，是否修由他定。
2. **顺带修的既有缺陷**：`diagnostics::bundle::latest_unprompted` 只按目录 mtime 排序，
   两个崩溃目录 mtime 撞同一刻度时顺序不定（本机稳定选错）。加目录名兜底（名字里就带日期）。
3. **登记期抢占 → 判失败（已定案，不是欠账）**：plan 之后、登记之前别人抢占同一路径时，
   「大小一致 → 认领；否则这一条导入失败」就是**最终口径**（崔总 2026-10-12 定），
   不做事后改名（那要新加 `FileOps::rename` + 组一致性 + 新的失败模式与测试，为一个极端窗口不划算）。
   `specs/import-dedup-fingerprint.md` §11.4 已按此改写，§12 不再列为待办。
4. **库内同 stem 的 RAW 在 `A` 层**（早先单独导过 RAW），本次成对导入要落 `A/_RAW/名`：
   判定只看该成员自己的落点目录 → 会形成同资产两个 RAW 副本。罕见、属库内整理，暂不处理。
5. **`design/main.pen` 已补**（2026-10-12 崔总切 Pencil 后）：右列底部新增 `ImportXmpRow` `bjrMu`
   （拖 `AvoidDupRow` `u7Qutm` 复制后改文案），`design/main.md` §3.3 已记；画面复核无裁切。
6. 命令体系（`AGENTS.md` §2.15）：**新勾选是设置项不是命令**（与「避免重复导入」同口径），
   不进 `features/commands/catalog.ts`、不需要默认热键。
7. `PlanCounts` 未把「内容判重」单独计数（并进 `duplicates`）；界面文案与计数口径不变。
