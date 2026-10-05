# XMP sidecar W1 实施完成

完成时间：2026-10-01 00:47:00 CST（第一检查点 2026-09-30 23:05；同夜完成全部实施）

## 改动范围

**核心 crate（`crates/raybend`）**
| 文件 | 内容 |
| --- | --- |
| `src/xmp/`（新） | `mod.rs`（发布：归属检查/外来 `.bak` 备份接管/未知版本保留/原子写/`remove_if_ours`/`is_ours`）；`path.rs`（`<主体名>.xmp` 位置规则 + `_RAW/` 折算 + 大小写折叠探测 `probe_sidecar`）；`packet.rs`（模型 + XML 构建/解析 + 哈希校验 + 版本闸门 + 共享 `escape`/`rdf_list`/`rdf_alt`）；`mapping.rs`（`crs:` 映射 9 标量 + 曲线 0..1→0..255 规范化 + 色标换算） |
| `src/lib.rs` | 声明 `pub mod xmp` |
| `src/export/metadata.rs` | 删除本地 `escape`/列表构造，改用 `xmp::packet` 共享件（§2.12 不写第二套）；既有 33 个导出测试全过 = 回归网 |
| `src/store/issues.rs` | 新增 `import_issue`（保留名称/基准/创建时间；序号沿用·冲突重分配；重复静默跳过；**基准对齐在去重之前**——否则两边基准不一致时哈希对不上） |
| `src/store/assets.rs` | `ApplyOutcome.new_asset_rows`（真创建的资产+相对路径）；`apply_sidecar_metadata`（采纳元数据落列；SQL 住 store 层） |
| `src/store/delete.rs` | 照片进回收站时**我们的**同名 sidecar 一并进回收站（别家的不动；失败只记日志不阻塞记录清理） |
| `src/import/sink.rs` | `find_or_create_asset` 返回 `(id, created)`；`flush` 事务内收集 `NewAsset`；`take_new_assets()` 给命令层 |

**外壳（`src-tauri`）**
| 文件 | 内容 |
| --- | --- |
| `src/sidecar.rs`（新） | 单工作线程 FIFO 队列（`Box<dyn FnOnce + Send>`，同资产后写覆盖前写）；`sync_now/sync_one`（读库 + app.db 取标签名 + compose + publish/清空删除）；`marked_assets`（标记补丁里影响 sidecar 的资产——排除喜欢/锁）；`adopt_new_assets/adopt_one`（读 sidecar → latest/定稿/元数据/关键词词典对齐；未知参数栈跳过编辑部分） |
| `src/develop.rs` | `develop_commit` 成功后 `queue_sync`（触发点 ①） |
| `src/issues.rs` | `issue_create`/`issue_delete` 后 `queue_sync`（②③） |
| `src/browse.rs` | `browse_mark`/`browse_undo`/`browse_redo` 后 `queue_sync`（④⑤⑥；撤销补丁直接改库、不经 develop_commit——spec §6.1 点名的隐藏触发点） |
| `src/import.rs` | `run_import_thread` 在 `run_batch` 后对新资产执行采纳（源目录探测） |
| `src/repo.rs` | `rescan_scope`（进目录同步）与 `rescan_library_with_progress`（重建数据）后对新资产执行采纳（库内探测，灾难恢复语义）；`adopt_sidecars_for_report` 助手 |

## 关键决策（实现层面，spec 之外的）

1. **空 `rb:profiles` 永远写**——它是「我们的文件」的标记；否则纯元数据 sidecar 会被自己判成外来文件、反复备份接管。
2. **`import_issue` 的基准对齐放在去重之前**（`rb:sourceBase` 声明值胜出）。
3. **采纳时栈过 `stack_importable` 校验**（参数 id/值域/曲线通道都是本版本认识的）——serde 反序列化不校验，防「新版本写的旧库」污染。
4. **写入队列单线程 FIFO**：保证同资产「后写覆盖前写」无竞态；命令不等待文件 IO。
5. **照片删除时 sidecar 失败不阻塞**（它是镜像不是本体，`AGENTS.md` §11.3 的「先文件后记录」只对照片本体严格）。

## 验证

- `cargo test -p raybend`：**1294 通过 / 0 失败**（含 xmp 39 + 新增 sink/assets/issues/delete 测试）。
- `cargo test -p raybend-desktop`：**102 + 2 通过 / 0 失败**。
- `cargo check --workspace`：0 错误；`cargo clippy`：**我改动的文件 0 告警**（仓里既有告警未动：`develop.rs` unused_mut 等非本次引入）。
- `pnpm test`：**1189 通过**；`pnpm lint:colors / lint:arch / lint:i18n`：全过。
- ⚠️ `pnpm typecheck` **失败**：`src/dev/KitchenSink.tsx` 缺 `FlowBarProps.onSettings/settingsOpen` —— 来自**另一会话（GPT）进行中的前端工作**（工作区里 App.tsx/FlowBar.tsx 等十余个 TSX 均为其未提交改动），与本次 XMP 实施（纯 Rust）无关；按崔总 2026-09-27 指示不越权代改，待其收口。

## 命令体系接入结论（`AGENTS.md` §2.15）

本波**无任何用户可触发命令**，不进命令注册表、无默认热键。理由（spec §9）：写出是编辑/标记动作的自动镜像、采纳是导入/重扫的自动副产物，均无「用户主动触发」语义；亦无设置开关（避免误关造成数据残缺）。

## 已验证（冒烟）与未经人类验证

- 已验证：全部单元测试、编译、lint 门、命令层接线编译通过。
- **待人类真机验收**（spec §10 的 7 条）：编辑→看 sidecar 内容；定稿/打标→sidecar 更新；sooc 切换→`crs:` 消失；清空→文件删除；**拷库演练**（整目录拷走再导入恢复）；ExifTool/Bridge 读标准层；删除照片→回收站含 `.xmp`。

## 遗留

1. 上述真机验收 7 条归崔总。
2. `pnpm typecheck` 的既有失败归 GPT 会话收口后自愈（KitchenSink 的 props 漏配）。
3. 既有 clippy 告警（复杂类型 / chunks_exact 等，历史代码）未清——非本波引入，留待统一清理。
