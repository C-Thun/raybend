# XMP sidecar W1 实施·第一批（模块文件与规格落账）

完成时间：2026-09-30 23:05:00 CST（第一检查点；实施进行中，按崔总指示暂停供提交）

## 本批落地内容

1. **规格与决策账**（无代码影响）：
   - `specs/xmp-w1.md`（新规格；崔总已口头批准实施）；
   - `specs/issue-xmp-contract.md` 小幅修订（`rb:ordinal`、`profileJson` 剔除 `auto_adjust`）；
   - `AGENTS.md` §2 新增第 19 条：新功能必须同步评估 XMP 侧影响，不得延后欠账（崔总 2026-09-30 定）；
   - `docs/user-requirements.md`（2026-09-30 两节原文照录）、`memory/{PLAN,REVIEW,FUTURE,ARCHITECTURE}.md`、
     `todos/2026-09-30-xmp-export-first.md`（转材料存档）。
2. **核心模块文件**（`crates/raybend/src/xmp/`，**尚未接进编译树**——`lib.rs` 没有 `pub mod xmp;`）：
   - `path.rs`：sidecar 路径规则（位图优先、`_RAW/` 折算、`<主体名>.xmp`）、`probe_sidecar`（大小写折叠探测）；含测试。
   - `mapping.rs`：`crs:` 映射（9 个直映标量 + 曲线 0..1→0..255 规范化 + 色标换算）；含测试。
   - `packet.rs`：模型（`SidecarContent/SidecarProfile/SidecarMetadata/ParsedSidecar`）、`compose`（含
     「仅 raw 基 latest 写 `crs:`」「空 `rb:profiles` 作为『我们的文件』标记」）、`parse`（roxmltree、
     哈希校验、版本闸门、宽容解析）、共享 `escape`/`rdf_list`/`rdf_alt`；含测试。
   - `mod.rs`：`publish`（归属检查 / 外来 `.bak` 备份接管 / 未知版本保留 / 原子写 / Unchanged 短路）、
     `remove_if_ours`、`is_ours`；含测试。

## 当前项目状态（可提交）

- **已编译代码零改动**：`lib.rs` 未声明 `xmp` 模块 ⇒ 四个新文件不参与编译，
  `cargo check/test` 结果与本批之前完全一致（未在本检查点重跑，依据是编译树未变）。
- ⚠️ 四个新文件的代码**未经编译与测试验证**（按崔总指示跳过检查直接收尾）；
  若提交前想确认，跑 `cargo check -p raybend` 即可（不接线则必然通过）。

## 待办（下一批，见 pi todo 工具任务 13–19）

1. `lib.rs` 加 `pub mod xmp;`，修编译错、跑测试到全绿（任务 13 收口）；
2. `export/metadata.rs` 的 `escape`/dc: 构造改用 `xmp::packet` 共享件（任务 14）；
3. `src-tauri` 侧同步服务 + 四个写触发点（develop_commit / issue_create·delete / browse_apply·undo·redo）（任务 15）；
4. 清空删除 + `store/delete.rs` 回收站带走 sidecar（任务 16）；
5. 导入与重建的新资产自动采纳（sink 记录新资产 → 命令层后置采纳，含 app.db 关键词 ensure）（任务 17）；
6. 全量测试与质量门（任务 18）；更新本记录或另开（任务 19）。

## 关键决策备忘（实现时容易忘的）

- 「我们的文件」判据 = 含 `rb:` 域（`rb:profiles` 元素**永远写**，即使为空——否则纯元数据 sidecar 会被误判外来）。
- `profileJson` 剔除 `auto_adjust`，与 `profile_hash` 同口径；导入时 `auto_adjust` 置空。
- 曲线写入前规范化：0–255 整数、x 严格递增、首末补 `(0,0)/(255,255)`、>32 点降采样；恒等曲线不写。
- 触发点必须含 `browse_undo/redo`（撤销补丁直接改库，不经过 `develop_commit`）。
- 写触发在 DB 事务提交后；计划用单工作线程队列（`Box<dyn FnOnce + Send>`）保证同资产 FIFO、后写覆盖前写。
- 导入采纳只对**新建资产**（`find_or_create_asset` 的 create 分支要记录），文件 IO 不进写事务。
