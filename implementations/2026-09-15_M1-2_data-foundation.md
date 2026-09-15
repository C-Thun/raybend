# M1-2 数据底座：app.db / catalog.db / 迁移备份 / 单写者

完成时间：2026-09-15 23:31:22 CST

计划：`plans/M1-2.md`（已通过 plannotator 评审，四条评审意见见 §6）
规格依据：`LIBRARY.md`（用户口述的库与导入规格）、`AGENTS.md` §6.4 / §7.1–§7.3

---

## 1. 本次交付了什么

**一句话**：把「打开库 → 迁移 → 备份 → 读写」这条链路做成**能安全长期使用**的基础设施，
并把 M1 真正要用的第一版 schema 落地。

| # | 交付 | 位置 |
| --- | --- | --- |
| 1 | 路径语义（NFC + 折叠 + 分隔符统一，全库唯一的「路径相等」判据） | `crates/raybend/src/store/path_semantics.rs` |
| 2 | 文件身份 `(卷序列号, 文件 ID)`，Windows 走 Win32、Unix 走 dev+ino | `store/file_id.rs` |
| 3 | Unix 毫秒时间工具（自带 civil 算法，不引 `chrono`） | `store/time.rs` |
| 4 | 连接统一 PRAGMA（WAL / NORMAL / busy_timeout / 外键） | `store/pragma.rs` |
| 5 | 迁移执行器：版本闸门 + `VACUUM INTO` 快照 + 逐条事务 + 完整性检查 | `store/migration.rs` + `store/migrations/*.sql` |
| 6 | 读连接池（自写，连接数上限 + `Drop` 归还 + 坏连接丢弃） | `store/pool.rs` |
| 7 | 单写者 actor（所有写串行到一条连接，退出时跑干队列） | `store/writer.rs` |
| 8 | 库身份：**可排序的定长 base62 ID**（时间 + 随机） | `store/ids.rs` |
| 9 | 库元信息、多路径登记、在线/离线解析（按**文件里的 ID**判，不按路径） | `store/library.rs` |
| 10 | 门面：`AppDb` / `CatalogDb`（打开、设置、注册、事务、flush） | `store/db.rs` |
| 11 | 云同步 / 网络盘位置判定（含 Linux 挂载表与 Windows 驱动器类型） | `store/location.rs` |
| 12 | 外壳侧接线：启动即打开 `app.db`，四个可验证命令 | `src-tauri/src/db.rs`、`src-tauri/src/lib.rs` |

**schema v1**（`store/migrations/`）：

- `app_0001_init.sql`：`libraries`（库注册表）/ `library_paths`（库 → 多路径 + 状态）/
  `settings`（KV）/ `jobs`（持久化任务队列，M1-3 与 M1-6 用）/ `app_meta`
- `catalog_0001_init.sql`：`library_meta`（**库身份**、导入模版、建库时间）/ `assets` /
  `asset_files`（位图与 RAW 分角色）/ `assets_fts`（`trigram` 全文索引）/
  `seq_counters`（按目录 + 位数计数）/ `import_runs` + `import_items`（导入批次与明细）

---

## 2. 关键决策与理由

### 2.1 `repo.json` 取消了（按用户 2026-09-15 的口述）

原设计是 `<仓>/.raybend/{repo.json, catalog.db, index.db}`。用户明确改为：

```text
<库根>/catalog.db      ← 库身份写在文件里
<库根>/photos/         ← 导入落地目录
```

好处是**库可以整体搬走**（身份跟着文件走，不跟着路径走），
也让「同路径不同库 / 同库多路径」有了实现基础。`AGENTS.md` §6.4 已同步更新，
完整业务规格写进 `LIBRARY.md`。

### 2.2 库 ID：用户 `Gid` 方案的裁剪版，不是 UUID

用户给的 `Gid`（Luclin）实测：`randomSize=6/lengthLimit=18` → 18 位；
12 位与 16 位是更短配置。它的本质是「**时间有序 + 随机补足 + 定长压缩**」。

本项目取它的形制，去掉 `shard`（那是分布式分库用的），定长 **16 位**：

```text
[ 时间刻度（10µs，自 SINGULARITY） | 随机 40 bit ] → base62 → 左补 '0' 到 16 位
```

- **可排序**：字母表保持 ASCII 升序 + 定宽 ⇒ 字典序 = 时间序（有测试守着）；
- **抗碰撞**：40 bit 随机，同毫秒并发生成也不怕（有 1.6 万次并发生成的测试）；
- **实现成本**：只要 `u128` 算术，不需要大数库（`Gid` 的 base62/base36 转换要 bigint）。

UUID v7 也能满足这两点，但长度 36、且与用户现有体系的「味道」不同。
若将来要对接 Luclin 服务端，已在 `FUTURE.md` **G15** 登记。

### 2.3 迁移的四道保险（`AGENTS.md` §8 第 5 条）

1. **版本闸门**：库比程序新 → 拒绝打开（`SchemaTooNew`，提示升级程序）；
2. **迁移前快照**：`VACUUM INTO`（原子、不阻塞读），**按库分组轮转、各留 7 份**；
3. **逐条事务**：每条迁移单独事务，失败整条回滚且 `user_version` 不前进；
4. **完整性检查**：迁移后跑 `foreign_key_check`，把「迁移写坏数据」暴露出来。

快照放在**应用数据目录的 `backups/`**（不在库根 —— 库根只该有 `catalog.db` 与 `photos/`）。
因为所有库共用一个备份目录，轮转必须**按库分组**：否则一个频繁升级的库会把别的库
唯一的备份挤掉。这一点有专门的测试。

### 2.4 读连接池自己写（评审意见 1）

不引 `r2d2`：连的是本机文件，没有网络抖动/认证过期/半开连接。真正要防的只有两件事 ——
**借出的连接要还回去**（`Drop` 保证，出错也还）、**连接数要有上限**（`Condvar` 等待）。
连接坏了直接丢弃、下次重开，这就是「基础防护」的全部。

**额外加了一条防线**：读池的连接都带 `query_only=ON`，写入只可能走写线程 ——
单写者设计在运行时被兜住了，而不是只靠约定。

**读池绝不创建文件**（刻意不带 `SQLITE_OPEN_CREATE`）：路径打错时必须在错地方**报错**，
而不是安静地生出一个空 `catalog.db`。

### 2.5 FTS5 用 trigram，但短词搜不到（已写进 SQL 注释）

`unicode61` 对中文等于没分（`AGENTS.md` §7.2），必须 `tokenize='trigram'`。
**实测约束**：trigram 按 3 个字符切分 ⇒ 「合影」「R5」这种短查询搜不到，
搜索功能实现时必须回退 `LIKE '%…%'`。这一点有测试钉住（`婚礼现场` 能搜、`合影` 不能）。

### 2.6 路径三种表示：raw / normalized / folded

| 表示 | 内容 | 用途 |
| --- | --- | --- |
| `raw` | 用户原样输入 | 展示、原样还原 |
| `normalized` | NFC + 分隔符统一（UNC 前缀保留 `\\`） | 展示、哈希 |
| `folded` | normalized + 小写 | **唯一索引与相等判断** |

不解析 `..`（没有文件系统无法可靠解析）；不改变大小写。
需要 `unicode-normalization`（std 不提供 NFC）；为了少一个依赖而自己实现 NFC 不现实。

### 2.7 位置判定只警告不拒绝

UNC → 网络；云同步目录（OneDrive / Dropbox / 坚果云 / Google Drive / 百度网盘 / Nextcloud…）
按**整段**匹配（`OneDrive - Contoso` 认，`我的OneDrive备份` 不认 —— 宁可漏报不误报）；
Linux 读 `/proc/mounts` 看文件系统类型（`nfs`/`cifs`/`sshfs`/**`9p`**）；
Windows 用 `GetDriveTypeW`。白名单之外的文件系统不报警。

`DRIVE_REMOTE` 用**本地常量**而不是 `windows-sys` 的导出：0.61 里它不在
`Win32::Storage::FileSystem`（版本间会挪位置），而数值是 Win32 的稳定契约 ——
这是 Windows 侧编译报错后查出来的。

### 2.8 外壳保持薄

`src-tauri/src/db.rs` 只有「解析路径 + 转发命令 + 启动日志」，业务全在 `raybend::store`。
启动时**打开失败不阻止窗口出现**（记日志，前端需要时再报错）。
四个命令：`app_paths` / `db_status` / `setting_get` / `setting_set`。

---

## 3. 验证方式（Agent 已做的）

| 验证 | 结果 |
| --- | --- |
| `cargo test -p raybend` | **160 个单测 + 2 个文档测试全绿**，用例本身 **0.47 秒**（计划要求「秒级」） |
| `cargo test -p raybend-desktop` | 1 个测试通过 |
| `cargo clippy --workspace --all-targets` | **0 警告** |
| `cargo check --workspace` | 通过 |
| **Windows 侧** `cargo check --workspace`（`CARGO_TARGET_DIR=C:\rb-target\raybend`） | 通过（1m06s）—— 覆盖了 WSL 上被 cfg 掉的 Win32 分支 |
| 覆盖的边界 | 空/空白路径、中文与 emoji 路径、全角、组合字符（NFC）、UNC、盘符根、超长路径、控制字符、时钟早于纪元、i64 极端值、并发写 400 条、迁移失败回滚、快照同秒撞名、外键违规、损坏 JSON、非 SQLite 文件冒充库、库比程序新、同路径不同库、同库多路径、离线库换挂载点、trigram 短词 |

**测试里几处「故意踩坑」的记录**：

- 内存库**不支持 WAL**（会回落 `memory`）→ WAL 的断言必须用文件库，否则测试假绿；
- 迁移期间外键必须**关掉**（允许重建表），结束后再打开并 `foreign_key_check`；
- 快照文件名里带库 ID 前缀，`rotate_backups` 必须按前缀分组，否则会互相挤掉。

---

## 4. 尚未做的（有意留给后续单元）

| 事项 | 归属 |
| --- | --- |
| 导入模版解析（变量、`FILENAME` 前缀匹配）、序号分配、重名 `_XX`、位图/RAW 分流、目录透传 | **M1-6**（表结构已就位：`seq_counters`、`import_runs`、`import_items`） |
| 库列表界面上的在线状态徽标、齿轮弹窗、离线库重新探测的交互 | **M1-5**（数据层 `resolve_library` / `set_path_status` 已可用） |
| 扫描、EXIF 抽取、缩略图 worker（消费 `jobs` 表） | **M1-3** |
| `index.db`（派生索引） | 真需要可重建索引时再建（`LIBRARY.md` §1） |
| 库文件加密 | `FUTURE.md` **G13**（按用户要求写明预想目的） |
| `photos/` 目录名可配置 | `FUTURE.md` **G14** |
| 与 Luclin `Gid` 的互操作 | `FUTURE.md` **G15** |
| 读池上限目前是常量 8（未做成设置项） | 有实际需要时再说 |

---

## 5. 顺手解决的两件事

1. **编辑器自动缩进噪声**：pi-lens 的自动修复会反复重排已提交文件的缩进，
   把 git diff 污染成几百行。做法：提交前用
   `for f in $(git diff --name-only); do [ -z "$(git diff -w --ignore-blank-lines -- "$f")" ] && echo "$f"; done`
   找出纯空白差异的文件，`git checkout --` 回退（真实改动在提交里，不会丢）。
2. **`ASSISTANCE.md` A7（Pencil）**：MCP 服务端可连、编辑器侧不可达，
   按用户指示继续推进，画布配色同步仍待用户打开 `design/main.pen` 时进行。

---

## 6. 评审意见的落实（对照 `plans/M1-2.md` §11）

| 评审意见 | 落实 |
| --- | --- |
| 读池自己写，简单防护即可 | ✅ `store/pool.rs`，无新依赖 |
| 整套库与导入业务（模版、序号、重名、RAW 分流、目录透传、多路径、在线离线、齿轮设置） | ✅ 整理为 `LIBRARY.md`（含 5 个待确认项），schema 已按它设计；实现分派到 M1-5/M1-6 |
| FTS5 trigram：建 | ✅ 已建 + 短词约束写进 SQL 注释与测试 |
| 加密不做，留 future 并写明目的 | ✅ `FUTURE.md` G13 |
| （追加）M2 前要先做浏览界面设计，等用户出方案 | ✅ 已写进 `PLAN.md` 的 M2 章节开头 |
