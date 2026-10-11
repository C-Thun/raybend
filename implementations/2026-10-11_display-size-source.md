# 显示尺寸口径统一（Display Size Single Source）

2026-10-11 05:47:12 完成。规格：`specs/display-size-source.md`。

**一句话**：把「这张照片现在显示的到底是多大」从三个互不同源的口径
（catalog 基础尺寸 / 变体的全尺寸渲染 / 渲染出来的字节）收敛成**一个** ——
尺寸与方向一样**记在库**里，一个解析函数按「当前显示哪个变体」给出尺寸，
flow info、tiles 比例、看图布局、右栏预览与对比全部读同一份。

---

## 一、改动范围

### 1. 数据层（catalog v19）

* 新迁移 `crates/raybend/src/store/migrations/catalog_0019_display_size.sql`（**只有 ALTER**）：

| 表 | 新列 | 语义 |
| --- | --- | --- |
| `asset_files` | `display_width` / `display_height` / `orientation` | 每个文件的**展示尺寸 + 自己的方向**（位图与 RAW 可以不同） |
| `develop_stacks` | `output_width` / `output_height` | latest 的**输出尺寸**（与栈同一笔事务） |
| `issues` | `output_width` / `output_height` | 每条定稿的**输出尺寸** |

* 全部 NULL-able：NULL = 未知，消费方回退旧行为（老库零回归）。

### 2. 解析层（唯一读口）

* 新增 `crates/raybend/src/store/sizes.rs`：
  * `SizeVariant::{Sooc,Raw,Latest,Issue(i64)}` + `parse`（`sooc` / `raw` / `latest` / `issue:N` 的唯一字符串解析处）；
  * `display_size(conn, asset_id, variant)` —— 判定矩阵与回退链写在模块文档注释里；
  * `stack_output_size`（latest 与定稿共用）、`output_of`、`source_size`；
  * `ensure_file_sizes(catalog, asset)` —— **唯一**会读文件头的入口（懒回填，幂等、只补空值）；
  * `backfill_dir_display_sizes(catalog, dir, files)` —— 目录级懒回填（`dir_meta_ensure` 用）。
* `crates/raybend/src/lib.rs` 不需改；`store/mod.rs` 注册模块与索引表补一行。

### 3. 写侧

| 位置 | 行为 |
| --- | --- |
| `import/sink.rs::register` | 每个非 sidecar 文件读**一次** `media::meta::read_photo_meta`，随 `Op::Register` 落到 `insert_file`（三列） |
| `store/assets.rs::insert_file` | 新入参 `display: Option<PhotoMeta>`（类型上强制每个新写入方想到尺寸这件事） |
| `store/develop.rs::save` | `output_size(源展示尺寸)`，**同一条 UPDATE / 同一笔事务**；`set_setting(SourceBase/Geometry)` 也重算（撤销走路） |
| `store/issues.rs::create` / `import_issue` | 定稿行同事务写输出尺寸；新增 `backfill_output_sizes` |
| `store/backfill.rs` | `Scope::{MissingTime, AllOnline, MissingSizes}`：三种范围都**逐文件**写 `display_*`（写前比较、不一致才改）；新增公开入口 `backfill_display_sizes`（taken_at 一个字不碰） |
| `src-tauri/develop.rs::develop_commit` | 落库前 `ensure_file_sizes`（源尺寸未知的老库顺手补，失败只记日志） |
| `src-tauri/source.rs::dir_meta_ensure` | 目录落在库根里时把刚读到的尺寸写回文件行（浏览网格给老库补比例的那条路） |
| `src-tauri/issues.rs::library` | 读定稿列表时懒回填 NULL 输出尺寸（失败只记日志） |
| `examples/backfill-metadata.rs` | 新增 `--sizes`（只补展示尺寸，不碰时间）；`--dry-run` 两种口径都报数 |

### 4. 暴露层

* `store/query.rs::AssetRow` 增加 `display_*` / `raw_*` / `latest_*`（展示文件的 `display_*` 直接取，
  RAW 行两个关联子查询，latest 走 `LEFT JOIN develop_stacks`——主键关联）；
* `src-tauri/browse.rs::AssetItem`：`width/height` 改为**优先读 `asset_files.display_*`**（NULL 才退回旧算式），
  新增 `rawWidth/rawHeight`、`latestWidth/latestHeight`；
* `src-tauri/issues.rs::IssueDto`：`width/height`（`store::issues::Issue` 带上 `output_*`）；
* `crates/raybend/export.rs::VariantSnapshot`：`width/height`（`snapshot()` 里算好，`#[serde(default)]`）；
* 契约三处同步：`src/api/dto-contract.json`（AssetItem 增 4 键）、`src/api/types.ts`、
  `src/api/dto-contract.test.ts` 的 `ASSET_ITEM_KEYS`；Issue / VariantSnapshot 的 TS 镜像同步（不在契约 JSON 里，无 Rust 侧键表）。

### 5. 前端消费方

* 新增 `src/lib/display-size.ts`：`displaySizeOf` / `issueOf` / `issueIdOf`（矩阵与 Rust 逐条一致）；
  入参用**结构形状**而不是 import `api/` 的类型 —— `lib` 层只允许依赖 `assets`（`lint:arch` 的规则）。
* `features/browse/store.ts::naturalOf`：库里显示尺寸优先（默认变体 = 编辑过就是 latest）；
* `features/browse/display-variants.ts`：`natural` 直接取 `VariantSnapshot.width/height`（毫秒级），
  `details` 退化为**只补直方图**；
* `features/browse/grid-source.ts` / `organization-source.ts`：经 store 的 `naturalOf` 与 `displaySizeOf`（tiles 比例）；
* `features/exif-strip/selected.ts`：`select(path, fallback, sizeOverride)` —— 覆盖优先级最高；
* `workspaces/browse/BrowseWorkspace.tsx`：flowinfo 的 effect 带 `displaySizeOf(锚点, 当前变体, 定稿)`；
* `features/browse/BrowsePanels.tsx`：右栏「文件信息 → 尺寸」同源（同一事实不允许两种表达）；
* `components/ui/viewer/store.ts`：新增 `naturalKnown`；`Viewer.tsx` 的 `<img>` **布局尺寸 = natural**，
  `onLoad` 只在已知尺寸缺失时兜底；
* `features/fullscreen/FullscreenViewer.tsx`：payload 的 `captured.width/height` 喂 `natural`（同一条规矩）；
* `src-tauri/export.rs::export_variant_details`：`width/height` 改**读库**（渲染只剩直方图职责）。

---

## 二、关键决策与理由

1. **尺寸挂 `develop_stacks` 而不是 `assets`**（规格 §3.4 的决策记录）：`develop::save` 是栈的唯一写入口，
   「与栈同一笔事务」= 不可能漂移；`assets` 另有导入 / 回填 / 元数据刷新等写者，派生值会出现第二写者。
2. **尺寸不进 `DevelopStack` / profile JSON / 哈希 / 签名 / XMP**（身份红线）。有**金标测试**钉住：
   `store::issues::tests::profile_hash_and_signature_stay_byte_identical_with_display_sizes_in_the_catalog`
   （固定样例栈 → `58428d1d076da744` / `16719530885303448403`，与改动前一致；这两个函数本次一个字没改）。
3. **`missing` 的语义边界**：`backfill_metadata` 的候选集**没有扩大**（仍是 `taken_at IS NULL`）——
   施工中发现扩大会破坏既有语义（`apply_exif` 是覆盖式写，会把已有 `taken_at` 冲成 NULL，
   老测试「已经有时间的资产不进候选」当场红）。尺寸回填走**独立范围** `MissingSizes`（例子的 `--sizes`）
   与 `AllOnline`（重建），两者都逐文件写、写前比较。
4. **`dir_meta_ensure` 加 `AppHandle`（不加 `repositoryId`）**：库根前缀匹配已经在
   `develop::resolve_asset` 里做过（`catalog_for_dir` 是它的目录版本，两者互为注记）；
   让前端多传一个库 id 会把「哪个库」的事实复制到调用方。
   `assets.width/height` 的「修正偏差」只做在 refresh / 重建那条路 —— 那里有「位图优先」的判据；
   `dir_meta_ensure` 是**单文件**读数，用它去改资产级尺寸会在「位图与 RAW 尺寸不同」时写错归属。
5. **前端布局尺寸**：`naturalKnown` 是新状态字段而不是「natural 非零」判据 ——
   后者在「上一张的尺寸被沿用」时会把新照片按旧比例布局（老代码本来就有这个沿用行为）。
6. **`lib` 层的结构形状入参**：`display-size.ts` 不 import `api/types.ts`（分层规则），
   声明 `DisplaySizeAssetRow` / `DisplaySizeIssueRow` 等**最小契约**，`AssetItem` / `Issue` 天然满足。

---

## 三、验证方式与结果

### 3.1 Rust

```text
cargo test --workspace
  raybend: 1489 passed; 0 failed; 11 ignored
  raybend-desktop: 133 passed; 1 ignored
```

新增/覆盖的测试（对应规格 §9）：

* `store::sizes`：判定矩阵覆盖（sooc/raw × 有/无文件 × 恒等/裁切 × 有/无方向 × NULL 回退）、
  `ensure_file_sizes`（真临时目录 + 裸 TIFF，幂等、坏文件不写垃圾）；
* `store::migration::display_size_migration_adds_nullable_columns_without_rewriting_rows`（v18 → v19：列在、旧行 NULL、数据不丢、`user_version=19`、重复应用空转）；
* `store::develop::stack_changes_rewrite_the_latest_output_size_in_the_same_transaction`
  （save 裁一半 → 2000×1500；`set_setting(Geometry)` 清掉 → 回到源；切基准 → 换尺寸；源未知 → NULL）；
* `store::issues::profile_hash_and_signature_stay_byte_identical_...`（身份金标）；
* `store::backfill::display_sizes_are_written_per_file_and_corrected_when_wrong`（每文件一份、只补空值、刷新时改正错值且不重写正确的行、幂等）；
* `import::sink::importing_records_each_files_own_display_size_and_orientation`（JPEG 40×20 + 竖拍裸 TIFF → bitmap(40,20,1) / raw(4000,6000,6)）；
* `export::tests::snapshot_carries_the_variant_display_size_from_the_catalog`（sooc / latest 裁切 / issue / 老库回退）；
* `browse::tests::asset_item_reads_the_stored_display_sizes_and_falls_back_for_legacy_rows`
  （`width/height` = 基准展示尺寸、`rawWidth/Height`、`latestWidth/Height`、老库退回旧算式）。

### 3.2 前端

```text
pnpm typecheck && pnpm test
  1278 passed; 0 failed
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n   → 全绿
pnpm build                                             → 成功（3.0s）
pnpm smoke:ui                                          → problems: []
```

新增：`src/lib/display-size.test.ts`（矩阵：default/latest/sooc/raw/issue × 缺失回退 × 坏值 × 认不出的串）、
`display-variants.test.ts`（snapshot 一到尺寸就有；无尺寸才回退基准）、
`selected.test.ts`（sizeOverride 优先级：文件回读也不许盖）、
`viewer/store.test.ts`（`naturalKnown` 语义：自带尺寸=已知、沿用上一张=未知、量到=已知）、
`browse/store.test.ts`（编辑过的照片 `naturalOf` = latest 输出尺寸）。

### 3.3 迁移 / 回填实测（v18 真库副本）

在**副本**上做（`C:\src\tmp\testrespos` 拷到 WSL 临时目录；原库未动）。
§「库不能放在网络盘」的检查拒绝从 WSL 直接打开 `/mnt/c` 下的库，所以副本落在 WSL 侧：

```text
$ cargo run -q -p raybend --example backfill-metadata -- <副本> --dry-run
（dry-run）这个库里有 0 张照片没有拍摄时间、22 个文件缺展示尺寸

$ cargo run -q -p raybend --example backfill-metadata -- <副本> --sizes
库：…
候选资产：      12
这次补上：      10
读完仍缺失：    0
文件不在磁盘上：2          # 两个文件确实不在副本里（离线）

$ 再跑一次 --sizes
候选资产：      2（还是那两个离线文件）· 这次补上：0 · 读完仍缺失：0     # 幂等

# 结果核对（sqlite3）
user_version = 19
assets / asset_files / develop_stacks / issues 行数 = 12 / 22 / 12 / 12（与升级前一致）
```

每文件独立的展示尺寸与方向（真库数据）：

| 资产 | `assets.width/height/orientation`（原始轴） | 位图文件 | RAW 文件 | 说明 |
| --- | --- | --- | --- | --- |
| 1 | 3712×2784 / 1 | 3712×2784 / o=1 | 5264×3904 | 位图与 RAW 尺寸不同，各记各的 |
| 8 / 10 | 3712×2784 / 8 | **2784×3712** / o=8 | **3904×5264** | 竖拍：方向已应用；两个文件方向一致 |
| 2 / 9 | — | NULL（离线） | NULL | 读不到就保持 NULL，不写垃圾 |

**迁移四件事的演练（`scripts/migration-drill.sh`，本单元重新跑一遍）**：

```text
$ bash scripts/migration-drill.sh /tmp/rb-migrate     # 默认工作目录 /mnt/c 的库被「网络盘」检查拒，故用 WSL 侧目录
…
⑥ 打开旧版库 → 迁移结果：from 1 to 19，applied [2..19]，
   snapshot: backups/catalog_00Mu4YBu_v1_20261010-214704Z.db（迁移前快照真存在）
   ✓ 标记行还在 / 身份行还在 / 分组认得出
⑦ 快照轮转：11 → 7 份（BACKUP_KEEP=7），别的库不被牵连
⑧ 未来版本的库（user_version=99）→ 拒绝打开，提示里说清「升级 raybend」
③ 头部写坏 → 拒绝打开、不静默重建；④ 快照恢复 → 账与损坏前一致
✅ 迁移 / 备份 / 损坏演练全部通过
```

即：**版本闸门 / `VACUUM INTO` 快照（保留 7 份）/ 逐条事务 / `foreign_key_check` 四件事均为框架既有，
本单元只加了一条 ALTER 迁移** —— 演练是在 v19 上线后跑的，覆盖了新增的这条。

**恢复步骤（回退）**：只有向前迁移；要回退用
`%LOCALAPPDATA%\com.cthun.raybend\backups\catalog_<库ID>_*.db` 快照覆盖库根下的 `catalog.db`
（先关程序；`-wal`/`-shm` 一并删掉）。

### 3.4 影响面与兼容边界（核对结果）

* 三张目标表的所有写语句都带列名，无 `SELECT *`——追加可空列不破任何既有语句（全仓 `rg` 核对过）；
* XMP 侧影响：**无**（`xmp/mapping.rs` 没有尺寸字段；`xmp/collect.rs` 的 `SidecarProfile` 逐字段构造，
  不序列化 `Issue`）；sidecar 不进尺寸，目录 / 命名 / `_RAW/` 规则也没动；
* 缩略图 / 大图缓存命名：`latest-<profile_hash 前 12 位>` 与 `issue-<id>-<hash>` 不变（哈希未变，有金标测试）；
* 命令体系：**不接入**——本单元没有新的用户可触发功能（无按钮、无菜单项、无快捷键）；
  「尺寸」是既有界面上的一行数据，只是变得正确了；
* 编辑器（GPU 视口）本轮**不改**尺寸口径（规格 §12 明确排除），后续统一项。

---

## 四、遗留

1. **Windows 真机冒烟（人类）**：RAW+JPG（尺寸不同 + 竖拍）一张，切 SOOC / RAW / 各定稿，
   核对 ①flow info 的尺寸 / MP 逐项变化且与所选一致；②view 的 fit 与 100% 以该尺寸为准；
   ③tiles 比例正确、不再「跳一下」。
2. **`dir_meta_ensure` 的资产级修正**：只补文件行三列，不碰 `assets.width/height`（理由见 §二.4）；
   若真机发现老库比例仍不对，优先用「重建数据」或 `--sizes` 补齐。
3. **编辑器口径统一**：编辑视口仍用自己的尺寸（GPU 侧），与本口径的收敛登记为后续工作
   （规格 §12 已把它排除在本单元之外）。
4. `specs/display-size-source.md` 的勾选项按本记录的实测结果更新（人类真机项保持未勾）。

---

## 追加：崔总真机反馈的两条（2026-10-11 14:56:56 完成）

真机验收报了两条，**同一根因**：

> 1. flow info 的尺寸一直不变，无论切哪个定稿都是 SOOC 的尺寸；刚进去显示 SOOC，
>    但编辑过一次生成 latest 之后就该以 latest 为主，包括文件类型；
> 2. latest 的图片比例不对（裁过的 3:2 被固定拉成原图的 4:3），而 3:2 的定稿切过去比例是对的。

### 一、根因：读侧把「列还没回填」当成了「另一个口径」

v19 才有的 `develop_stacks.output_*` / `issues.output_*` 三列，**v19 之前建的编辑栈与定稿全是 NULL**
（在客户真库上核实：12 个栈、12 条定稿，升级后 0 个栈有尺寸 —— 全库只有 10 条定稿被懒回填过，
因为回填只发生在「打开那张照片的定稿面板」那一刻）。而 DTO 只读列：

* `AssetItem.latestWidth/Height` = 列 → NULL ⇒ 前端 `displaySizeOf` 回退到 **基准展示尺寸**
  = 未裁切的原始画面（4:3）⇒ ①flowinfo 永远显示 SOOC 的尺寸、②把 3:2 的画面硬拉成 4:3
  （`<img>` 的布局盒 = 变体尺寸，比例不符就被 `object-fit` 默认的 `fill` 拉变形）；
* 定稿那边的 3:2 之所以「对」，是因为它那条刚好被懒回填过 → `IssueDto.width/height` 有值。

**回退到基准尺寸是错的**：基准是另一个口径（未裁切的原始画面），不是「未知」。
真未知（源尺寸也拿不到）才该退。

### 二、修法（全部围绕「NULL ≡ 现算」）

| 位置 | 改动 |
| --- | --- |
| `store::sizes` | `latest_size` / `issue_size` 转公开（**读侧现算**的唯一入口，零 I/O）；新增 `issue_output_size`（已解析的定稿 → 尺寸，供 `list` 与写侧回填共用一份算法）；`ensure_file_sizes` 拆出连接版 `ensure_file_sizes_conn(conn, root, asset)` |
| `store::query::page` | 本页里「有栈行但尺寸列 NULL」的行**按栈现算**再交给 DTO（不新增查询口径，只多几条零 I/O 的读） |
| `store::issues::list` | 同上：栈与基底已经在手上，NULL 的行现算（`get` 走 `list`，自动同修） |
| `store::issues::backfill_output_sizes` | 改调 `issue_output_size`（与读侧同一份算法，不写第二份） |
| `src-tauri/export.rs::export_snapshots` | 切变体时对 ≤8 张顺手回填 `asset_files.display_*`（读一次文件头；导出面板那种批量不做，交 `--sizes`/重建），于是 `raw` 的**绝对值**也不再退到基准 |
| `AssetItem.latestSourceBase`（新 DTO 字段，契约三处同步） | 来自 `develop_stacks.source_base`——flowinfo 的「文件类型」要它 |
| `lib/display-size.ts::sourceBaseOf` | 变体 → 来源基底（`raw` / `sooc` / 说不清），与尺寸矩阵同一个变体映射、同一处 |
| `features/exif-strip/selected.ts` | 第三个参数由「尺寸覆盖」升级为**变体覆盖** `{width, height, base}`：尺寸照旧最后上；`base === "raw"` 时把 `format` 强制成 `RAW`（**崔总 2026-10-11 定：文件类型跟变体来源基底**，`SOOC` 保持位图自己的格式） |
| `BrowseWorkspace` flowinfo 效果 | 尺寸优先取该变体装载时的 snapshot（更"新"），没有才读列表项；同时传 `sourceBaseOf` |
| `Viewer.tsx` / `FullscreenViewer.tsx` / `CompareView.tsx` | `<img>` 加 `object-contain` 安全网（盒子比例若有出入，宁可留边也不拉变形） |

### 三、验证

**真库副本钻取**（`C:\src\tmp\testrespos` 的副本，与客户真库同构：display 列全 NULL、栈尺寸全 NULL）：

```text
# 修前：DTO 只读列 ⇒ 这张裁切过的照片 latest 为空 ⇒ 前端退基准 3712×2784（4:3）
# 修后（query::page 与 browse DTO 同一条读路）：
asset 2 : latest = 3712×2475  latestBase = raw      ← 3:2 裁切，正是真机那张
asset 4 : latest = 2898×2173  latestBase = raw      ← 旋转 13.35° + 方形裁切
asset 8 : latest = 2784×3712  latestBase = raw      ← 竖拍（方向 8 已应用）
asset 7 : 定稿 #11/#13 由 list 现算 = 3712×2784（原先 NULL）
```

**测试**：`cargo test --workspace` 1492 + 134 全绿；`pnpm typecheck / test(1279) / lint:colors / lint:arch / lint:i18n / build` 全绿；
`pnpm smoke:ui` → `problems: []`。新增/加强的单测：

* `store::query` 读侧现算（在 `browse::tests::asset_item_computes_latest_size_when_the_stack_row_predates_v19`
  与 `issues::tests::list_computes_output_size_when_the_issue_row_predates_v19` 里，两条都模拟「v19 之前建的行」）；
* `lib/display-size.test.ts`：`sourceBaseOf` 全矩阵（含脏值 / 找不到定稿 → 说不清）；
* `features/exif-strip/selected.test.ts`：变体覆盖的尺寸与类型都不许被文件回读盖掉；`sooc` 基底不动类型。

### 四、这一轮学到的口径（写进 `memory/FUNCTION-IMAGING.md` §2.1）

**NULL 列 ≠ 未知，更 ≠ 基准尺寸** —— 派生值（输出尺寸）在库里没存时，
读侧要用**产生它的那份输入**现算（栈 + 源尺寸，零 I/O）；只有连输入都没有才是 NULL。
真机症状（裁切被拉成原图比例）就是这条没立住造成的：回退链的每一跳都必须是**同一个口径**。

### 五、边界（未做）

* `raw` / `sooc` 的**绝对值**在旧库仍要先回填文件行才精确；回填发生在「切到那个变体时」（≤8 张一批）
  或 `--sizes` / 重建数据。**比例**从第一眼就是对的（基准与 RAW 同画面比）。
* 编辑器（GPU 视口）与「文件类型」无关（编辑器有自己的基底选择），本轮不动。
