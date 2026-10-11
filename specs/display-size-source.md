# 显示尺寸口径统一（Display Size Single Source）— 工作单元规格

**状态**：待崔总审 → 由**新会话执行**（本文档即执行规格，自包含）。
**关联**：`AGENTS.md` §2.10/§2.12/§2.13/§2.16/§2.19；`crates/raybend/src/store/migration.rs`（迁移四件事）；
`crates/raybend/src/store/migrations/catalog_0018_recycle.sql`（上一版）；`crates/raybend/src/store/query.rs`（`AssetRow`）；
`crates/raybend/src/store/assets.rs`（`asset_files` 写入）；`crates/raybend/src/store/backfill.rs`；
`crates/raybend/src/media/meta.rs`（`PhotoMeta` / `oriented_size`）；`crates/raybend/src/develop/geometry.rs`（`output_size`）；
`src-tauri/src/browse.rs`（`AssetItem`）；`src-tauri/src/export.rs`（`VariantSnapshot` / `export_variant_details`）；
`src-tauri/src/issues.rs`（`IssueDto` / `preview_bytes`）；`src/components/ui/viewer/{store.ts,Viewer.tsx}`；
`src/features/browse/display-variants.ts`；`src/features/browse/grid-source.ts`；`src/features/exif-strip/selected.ts`；
`memory/FUNCTION-IMAGING.md` §3/§4；`memory/FUNCTION-BROWSE.md` §5.9/§5.10。

---

## 0. 一句话与验收清单

**一句话**：把「这张照片现在显示的到底是多大」从**三个互不同源的口径**（catalog 基础尺寸 / 变体的全尺寸渲染 /
渲染出来的字节）收敛成**一个**——尺寸与方向一样**记在库**里（SOOC、RAW 每文件一份，latest 每次落栈一份，
每条定稿各一份），一个解析函数按「当前显示哪个变体」给出尺寸，**flow info 与 view 都读同一份**（view 按该尺寸布局显示）。

**效率基线（本单元的硬指标）**：改完以后，**点开任意一张照片 / 切换任意变体，尺寸都是库里已存的值** ——
不读文件头、不渲染、不等任何后台任务（tiles 的一次分页查询里就带回来）。

**验收清单**（做完逐项勾；人类项单独标）：

- [x] `catalog.db` 升到 **v19**：`asset_files.display_width/display_height/orientation` +
      `develop_stacks.output_width/height`（latest）+ `issues.output_width/height`（定稿）；
      **只 ALTER、无数据重写**（升级毫秒级）
- [x] 迁移四件事齐：版本闸门 / `VACUUM INTO` 快照（`BACKUP_KEEP=7`）/ 逐条事务 / `foreign_key_check`；
      旧程序打开 v19 库被**拒绝**；升级失败可从快照恢复
- [x] 导入时对**每个文件**（bitmap / raw）各写一份展示尺寸 + 自己的方向；旧行 NULL
- [x] 懒回填：NULL 才补、值不符才改；不扫全库、不阻塞 UI、可重入、失败只记日志
- [x] 定稿创建时写 `issues.output_width/output_height` = `geometry.output_size(源展示尺寸)`（**零渲染**）
- [x] **latest 也落库**：`develop_stacks.output_width/output_height`，与编辑栈**同一笔事务**写；
      没有编辑栈的行 NULL（回退基准展示尺寸）
- [x] 唯一解析函数 `store::sizes::display_size(...)`（sooc / raw / latest / issue:N × NULL 回退），单测矩阵齐
- [x] `AssetItem` 带 `rawWidth/rawHeight` 与 `latestWidth/latestHeight`；`IssueDto` 带 `width/height`；
      `VariantSnapshot` 带 `width/height`；契约三处同步（Rust 结构 / `src/api/dto-contract.json` / `src/api/types.ts`）且契约测试绿
- [x] flow info 的「尺寸 / MP」**随显示变体变化**（SOOC / RAW / 每条定稿各按自己的口径）
- [x] tiles 比例 / viewer natural / 右栏预览框 / compare 也随变体，且**毫秒级就有**（不再等全尺寸渲染）
- [x] **view 按该尺寸布局显示**：`<img>` 的布局尺寸 = 变体尺寸（不再是位图固有尺寸）；「100%」= 相对该尺寸的 1:1
- [x] `export_variant_details` 的尺寸改读库（那次**全尺寸渲染**只剩直方图职责）
- [x] **身份不变**：固定样例栈的 `profile_hash` / `DevelopStack::signature()` / 大图缓存文件名 / XMP 输出
      与改动前逐字节一致（尺寸只进 DB 列，不进结构体）
- [x] 单测（Rust + TS）全绿；`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`
- [x] `pnpm smoke:ui` 通过（`problems: []`）
- [ ] **Windows 真机冒烟（人类）**：RAW+JPG（尺寸不同 + 竖拍）一张，切 SOOC / RAW / 各定稿，
      看图与 flow info 的尺寸逐项确认
- [x] 纪律同步：`memory/FUNCTION-IMAGING.md`（尺寸口径）、`memory/FUNCTION-BROWSE.md`（flowinfo）、
      `implementations/YYYY-MM-DD_display-size-source.md`（含迁移/回填实测与真机结论）

---

## 1. 问题（为什么做）

### 1.1 现状：三个口径（已核对代码与真库数据）

| 口径 | 来源 | 谁在用 | 问题 |
| --- | --- | --- | --- |
| **像素** | issue → `issues::preview_bytes`（FullCache AVIF，Screen 档长边 1920）或 `view_image`（RAW 管道） | viewer `<img>` 的**固有尺寸** | 只到 1920 档；与界面上的"尺寸"无关 |
| **尺寸（基础）** | `assets.width/height`（catalog，**只记展示基准文件**，位图优先）+ `assets.orientation`，在 DTO 边界归一（`browse.rs::oriented_dimensions`） | tiles 比例、viewer natural（fit / pan / compare）、flow info「尺寸/MP」、右栏文件信息、右栏预览框 | **不随显示变体走**：RAW / 定稿与之不同时全部按 SOOC 算 |
| **尺寸（变体）** | `export_variant_details`：**全尺寸渲染** captured 后 `image.dimensions()` | `display-variants.natural`（tiles 比例与 viewer natural 的兜底） | 秒级（20MP RAW）；且 flow info 完全不读它 |

**真库证据**（`C:\src\tmp\testrespos`）：资产 10 的 `assets` 行 = 3712×2784（位图 MYP1000048.JPG，orientation=8），
同资产 `_RAW/MYP1000048.RW2` = 5208×3904，定稿 12 的预览 = 1440×1920（RAW 显影 + 摆正 + 1920 档）。
三个数字互不相同，而界面只认第一个。

**已核实的两个既定事实（不要踩反）**：
1. `assets.width/height` 存**原始像素轴**（未应用方向）——这是设计（`store/backfill.rs` 有注释、`browse.rs` DTO 边界才归一）。
2. RAW 文件与定稿**没有任何尺寸记录**，只能靠渲染或读文件现场得到。

### 1.2 后果
- flow info 的尺寸/MP 永远按基准位图（SOOC）算 → 切到 RAW / 定稿后是错的（崔总报告的正是这条）；
- tiles 比例与 viewer natural 在 details（全尺寸渲染）回来前是错的，回来时跳一次；
- 只为"知道多大"付一次全尺寸渲染；
- "哪个口径"散在三处，任何新消费方都要再猜一次 —— 这次问题的结构性根因。

---

## 2. 口径定义（先钉死术语）

| 术语 | 定义 | 存哪 |
| --- | --- | --- |
| **原始轴尺寸** | 文件头里的 width/height（**未**应用 EXIF 方向） | `assets.width/height`（沿用，不动语义） |
| **方向** | EXIF 1..8（`normalize_orientation` 的输入） | `assets.orientation`（沿用）、`asset_files.orientation`（新增，每文件自己的） |
| **展示尺寸** | `oriented_size(原始轴, 方向)` = 屏上「宽 × 高」 | `asset_files.display_width/display_height`（新增） |
| **输出尺寸** | 成片尺寸 = `EditGeometry::output_size(源展示尺寸)`（已含方向与裁切/旋转） | `develop_stacks.output_width/height`（latest，新增）、`issues.output_width/height`（定稿，新增） |
| **显示变体** | `sooc` / `raw` / `latest` / `issue:N`（`DisplayVariant`） | 选择状态在内存，尺寸解析见 §4 |
| **变体尺寸** | 解析函数按显示变体给出的「展示尺寸」（= 界面上该显示多大） | 四类**各自落库**（文件 / 栈 / 定稿行）；解析函数只做选择与回退 |

**铁律**：
1. **原始轴尺寸与方向分开存；展示尺寸是"算出来"的一等公民**（每文件入库，见 §3；界面永不自己换算方向）。
2. **输出尺寸必须由渲染器同一个函数算**：`EditGeometry::output_size`（不允许界面侧或第二处再推一遍）。
   **写入点只有两个**：编辑栈落库（latest）与定稿创建；都写在同一笔事务里，与它描述的对象同生共死。
3. **一个变体一个口径，解析只允许一处**（§4 的函数）；消费方只读它，不各自回退。

---

### 2.1 两条尺寸来源的统一（2026-10-11 定案，崔总口述）

尺寸只有两条来源，**谁负责哪一类是钉死的**：

| 文件类型 | 权威来源 | 说明 |
| --- | --- | --- |
| **位图**（JPG/PNG/TIFF…） | **我们自己解文件头**（`media::meta::read_photo_meta`） | 文件头写的就是成像尺寸，与实拍像素一致 |
| **RAW** | **rawler 回传的 develop 输出尺寸**（`RawlerBackend::output_size`，照抄 `CropDefault` 的算法） | 崔总 2026-10-11：「图片尺寸以 rawler 回传为主……**对 RAW 来说不能用私自解算像素数来替代正式 develop 输出的真实位图像素数**」 |

理由（真库实测）：RAW 文件头给的是**整传感器帧**（MYP1000037.RW2 = 5264×3904），
而正式 develop 输出是**有效区**（5184×3888）—— 两者差 1.5%。用文件头那份当展示尺寸，
布局盒就比真像素大一圈，「100%」与出图对不上。

**唯一入口**（别在别处写第三份）：

* `store::sizes::raw_output_size(path)` —— RAW 的出图尺寸（原始轴）；
* `store::sizes::apply_display_size_source(path, meta)` —— 展示尺寸口径（RAW 换成出图尺寸并按方向摆正；其余原样）；
* 写入点共四处，全部走上面两个函数：**导入登记**（`import/sink.rs`）、
  **逐张懒补**（`ensure_file_sizes`）、**整库批量回填**（`examples/backfill-metadata --sizes`）、
  **浏览目录扫描**（`backfill_dir_display_sizes`）。

**规则的边界（同一件事写清楚，免得以后各写各的）**：

* RAW 行**没有值就是未知**（`None`）—— **不许**退到 `assets` 的 EXIF/文件头尺寸顶替
  （`preferred_size("raw")` 直接 `None`）；界面按「还没拿到」处理，保底回填后自然变准。
* **浏览目录扫描不写 RAW 行**（那是私自解算的数，见上）—— 它只写位图行；
  RAW 的比例读数也**只认库里的记录**，记录没建好就报 0（界面用默认占位比例）。
* **允许的近似**（不写库、只当提示，因此不违反上面两条）：渲染时「要解多少像素」的输入规模提示
  （`thumbnail/render.rs::render_raw_file` 的 `source_size`）与浏览网格的占位比例 ——
  它们是**请求/摆位**的提示，真实像素数一律以解码结果与库里的记录为准。

## 3. 数据层（`catalog_0019`）

### 3.1 schema

`crates/raybend/src/store/migrations/catalog_0019_display_size.sql`（新文件）：

```sql
-- catalog.db schema v19：显示尺寸（与方向一样记在库里）
--
-- 出处：`specs/display-size-source.md` §3。多版本尺寸：
--   * 每个文件一份（bitmap / raw 各自的方向与展示尺寸）；
--   * latest 一份（挂在编辑栈行上，与栈同一笔事务更新）；
--   * 每条命名定稿一份（成片输出尺寸，已含方向与几何）。
-- 展示尺寸 = oriented_size(原始轴, 方向)；`assets.width/height` 的原始轴语义不变。
-- 全部 NULL-able：NULL = 未知（旧行），消费方回退旧行为；无数据重写。
ALTER TABLE asset_files ADD COLUMN display_width INTEGER;
ALTER TABLE asset_files ADD COLUMN display_height INTEGER;
ALTER TABLE asset_files ADD COLUMN orientation INTEGER;      -- 1..8；NULL = 未知
ALTER TABLE develop_stacks ADD COLUMN output_width INTEGER;  -- latest 成片输出尺寸
ALTER TABLE develop_stacks ADD COLUMN output_height INTEGER;
ALTER TABLE issues ADD COLUMN output_width INTEGER;          -- 定稿成片输出尺寸
ALTER TABLE issues ADD COLUMN output_height INTEGER;
```

并在 `crates/raybend/src/store/migration.rs` 的 `CATALOG_MIGRATIONS` 末尾登记：

```rust
Migration { version: 19, name: "display_size", sql: include_str!("migrations/catalog_0019_display_size.sql") },
```

（列表注释里补一行 `* v19 display_size：每文件方向 + 展示尺寸；latest 与定稿的输出尺寸`。版本号必须从 1 连续递增，有测试守着。）

### 3.2 写入时机

1. **导入**（`store/assets.rs` 的 `register_file` / `INSERT INTO asset_files`，由 `import/sink.rs` 调用）：
   对每个非 sidecar 文件**读一次文件头**（`media::meta::read_photo_meta`：只读头、毫秒级），
   一次拿到 `(展示宽, 展示高, 方向)`，写进该行的三列。**不允许**为写这个再读第二遍文件、也不允许在别处再算一次方向。
   `assets.width/height/orientation` 保持现状（主文件，原始轴）——**不动**。
2. **编辑栈落库**（`crates/raybend/src/store/develop.rs::save`，由 `develop_commit` / 自动保存调用）：
   **同一笔事务**里写 `develop_stacks.output_width/output_height` =
   `stack.geometry.output_size(源文件展示尺寸)`（源 = `stack.source_base` 对应的 `asset_files` 行）。
   于是 latest 的尺寸与 latest 本身**同生共死**：栈在则尺寸在，每次编辑（每次 confirm 落库）都同步更新，
   tiles 一页查询里就能带回来（无需读文件、无需渲染）。
   源尺寸未知（老库未回填）→ 顺手读一次文件头补齐 `asset_files.display_*`（懒回填），两都拿不到才写 NULL。
3. **定稿创建**（`crates/raybend/src/store/issues.rs::create`）：写 `issues.output_width/output_height` =
   `stack.geometry.output_size(源文件展示尺寸)`，与定稿行同一笔事务。
   源尺寸拿不到（库离线 / 行缺失 / NULL）→ 写 NULL，**不阻塞创建、不报错**。
4. **懒回填**（幂等；只在 NULL 或与磁盘不符时写）：
   - `dir_meta_ensure`（`src-tauri/src/source.rs`）与 `store::backfill::refresh_assets`：已经在读文件头 →
     顺手写回该文件的三列；同时修正 `assets.width/height`（若有偏差）；
   - `issue_library`（读定稿列表）：`issues.output_width/height` 为 NULL 且能算 → 写回；
   - `crates/raybend/examples/backfill-metadata.rs`：扩展成"把老库的方向/尺寸补全"（保留 `--dry-run`）。
   回填**不许扫全库、不许阻塞 UI**；写走既有单写者（`CatalogDb::write`），失败只记日志（下次再来）。

### 3.3 语义与不变量
- **NULL = 未知**：消费方回退到旧行为（见 §4 的判定矩阵），不报错、不弹窗。
- **栈与它的尺寸不分割**：`develop_stacks` 的写入入口唯一（`develop::save`），尺寸在同一事务里一起写；
  加一条测试钉住「栈变→尺寸跟着变」，防止将来新增写入路径时漏掉尺寸（漏了也只是回退到旧算式，不会崩）。
- 定稿被删 → 尺寸随行删除；源文件被替换 → `invalidate_source` 不动尺寸（尺寸与像素无关），
  但回填路径要在**写前比较**，不一致才改（防抖 + 自愈）。
- 位图与 RAW 的方向**可以不同**：各按自己那行算，不许用 `assets.orientation` 代替。

### 3.4 决策记录：latest 的尺寸为什么挂在 `develop_stacks`（崔总 2026-10-11 问）

**先摆事实（现状）**：`develop_stacks` 的主键就是 `asset_id`（一照片一行），建表注释写明
「`latest` —— **就是这张表**：当前编辑结果，唯一可编辑的那个」；SOOC / RAW 是虚拟的（文件本身），
issue 才是另一类实体（不可变、多条、`profile_json` 内联）。且 `develop::save` 在栈全空时会**删掉行** ——
「有行 = 有 latest，没行 = 没编辑过」。

所以「latest 归到照片主体」在结构上**已经成立**；真正要定的只是**尺寸挂哪一行**：

| 方案 | 写侧 | 读侧 | 结论 |
| --- | --- | --- | --- |
| **A. `develop_stacks.output_*`（本 spec）** | 与栈**同一笔 UPDATE、同一笔事务**（`develop::save` 是唯一写函数）→ 不可能漂移 | `LEFT JOIN` 走主键，等于零成本 | ✅ 采纳 |
| B. `assets.latest_width/height` | `assets` 另有写者（导入 / 回填 / 元数据刷新）→ 派生值出现第二写者，正是本次问题的同款病 | 省一次 JOIN | ❌ 否决 |
| C. 像 issue 那样独立成实体 | 要新表/新列，改所有读写路径与缓存命名 | 无收益 | ❌ 否决（latest 是可变·单条·与照片一对一，issue 是不可变·多条） |

**如果将来真要「彻底归到照片主体」**：那等价于把 `develop_stacks` / `develop_params` / `develop_curves`
三张表合并进 `assets`（读写路径 / XMP / 撤销 / 缓存命名全动）——那是另一个量级的重构，本单元不做，
登记到 `memory/FUTURE.md` 更合适（可选，不阻塞本单元）。

### 3.5 影响面与兼容边界（为什么不会「改一处炸三处」）

> export / editor / browse 共用 latest 与「这张多大」这两个概念 —— 下面是全仓核对过的边界（2026-10-11）。

**A. 结构性安全（已核对，不靠猜）**
- 三张目标表的**所有写语句都带列名**：全仓 `rg` 无 `INSERT INTO <表> VALUES`（裸写）、无 `SELECT *` ——
  追加可空列不会破任何既有语句（连测试夹具都带列名）。
- `develop_stacks` 的**写入口唯一**：`store/develop.rs`（`save` / `ensure_stack` / 删除）；
  XMP 导入也走 `develop::save`（`xmp/adopt.rs:52`）→ 尺寸挂进 `save()` 那条 UPDATE 就是全网生效。
- `issues` 的写入口唯一：`store/issues.rs`（`create` + XMP 导入那条 INSERT + `rename` / `delete`）
  → 两条 INSERT 都要写 `output_*`。
- `asset_files` / `assets` 的其他写者（回收站、目录同步、标签、AI、回填）全部带列名且**不碰新列**
  （回收站只改 `rel_path` / `recycled_from`）。

**B. 身份敏感链（本次绝对不许动）** —— 尺寸是**派生元信息**，不得进入任何「身份」：

| 身份 | 由什么算 | 带尺寸进去会怎样 |
| --- | --- | --- |
| `DevelopStack::signature()` | 结构体（参数/曲线/几何/…） | 进缩略图缓存键（`render_sig_with_edit`）→ 全量重渲 |
| `issues::profile_hash(stack)` | 结构体的规范化 JSON | 进 `issues.profile_hash` / `FullCache` 命名（`issue-<id>-<hash>`）/ 导出定稿序号匹配 I00–I99 / XMP `rb:profiles` / `still_current` 校验 |
| `VariantSnapshot.profile_hash` / `reference` | 同上 | 导出队列与入队校验同上 |
| XMP 包 | `rb:profiles`（hash + 栈） | 本轮**不写尺寸**（`xmp/mapping.rs` 无尺寸字段），sidecar 完全不受影响 |

**实现约束**：尺寸**不进** `DevelopStack` 结构（只在 DB 列上）；`save()` 用入参 `stack.geometry` 现算后写列。
不得为了“顺手”把尺寸加进 profile JSON / hash / 签名 / XMP —— 那才是真正会“炸三处”的做法。

**C. 各消费方本轮动不动**

| 消费方 | 现在读什么 | 本轮 | 不改会不会出问题 |
| --- | --- | --- | --- |
| browse（tiles / view / compare / 右栏 / flow info） | `assets` 尺寸 + details 渲染 | ✅ 改唯一读口 | —— |
| export（`export_variants` / `export_snapshots` / `export_variant_details` / 队列 / 命名） | `VariantSnapshot`（新增字段；队列是**内存态**，`export/jobs.rs` 的 `Arc<Mutex<State>>`） | ✅ 只补 `snapshot.width/height` 与 details 读库 | 不会错（导出用真实渲染像素 + maxEdge），但尺寸继续晚到 |
| editor（视口 / 裁切 / 预览队列 / 撤销 / 定稿） | 自己的 Rust 视口尺寸 + `DevelopStack` | ❌ 本轮不改（实施记录标为后续统一项） | 不会（尺寸只新增列、不参与任何身份与渲染输入） |
| XMP（写/读 sidecar） | `rb:profiles` 的 hash + 栈 | ❌ 不动 | 不会（尺寸不进 sidecar） |
| 缩略图 / 大图缓存 | `render_sig` / `FullCache` 命名 | ❌ 不动 | 不会（身份没变） |
| 导入 | 文件头元数据 | ✅ 顺手写每文件展示尺寸 | —— |
| 回收站 / 目录同步 / 元数据回填 | 文件行 | ✅ 回填路径补尺寸（其余不动） | —— |

**D. 回归闸门（证明“没炸”的测试）**
- **身份对比测试**：同一份固定栈，改动前后 `profile_hash` / `signature` / 大图缓存文件名 / XMP 输出保持**逐字节不变**（尺寸不进结构体，这条必绿）；
- **老库回退测试**：v18 → v19 后未回填的库，tiles / 看图 / flow info / 导出 / 编辑器打开同一张照片的行为逐项与今天一致（尺寸走回退链）；
- 导入 / 回填 / 回收站 / XMP 导入 / 导出各跑一遍冒烟（这些路径都有现成单测，新增列后必须全绿）；
- 契约三处同步。

---

## 4. 解析层（唯一函数）

新增 `crates/raybend/src/store/sizes.rs`（`store/mod.rs` 注册）：

```rust
/// 显示变体 → 该变体的展示尺寸（UI 上「这张显示多大」的唯一口径）。
pub enum SizeVariant { Sooc, Raw, Latest, Issue(i64) }

/// 返回展示尺寸；数据不足给 None（调用方回退，见判定矩阵）。
pub fn display_size(conn: &Connection, asset_id: i64, variant: SizeVariant) -> Result<Option<(u32, u32)>>;
```

**判定矩阵**（实现与单测的唯一依据）：

| 变体 | 用谁 | 缺失回退 |
| --- | --- | --- |
| `Sooc` | bitmap 行的 `display_*` | raw 行 → `assets` 展示尺寸 → NULL |
| `Raw` | raw 行的 `display_*` | bitmap 行 → `assets` 展示尺寸 → NULL |
| `Latest` | `develop_stacks.output_width/height`（该行非 NULL） | 空栈 / 无栈行 → 基准展示尺寸；NULL 但栈非空 → 现算 `output_size(源展示尺寸)` → NULL |
| `Issue(id)` | `issues.output_width/height`（非 NULL） | 现算 `geometry.output_size(源展示尺寸)` → NULL |

**约束**：
- 方向只调用一处（`media::meta::oriented_size`，或对已存 `display_*` 的行直接读）；几何只调用 `EditGeometry::output_size`。
- **前端不许有第二份实现**：TS 侧只转发/选择，不做方向与几何运算。
- `display_size` 是同步纯查询（只读库，不读文件、不渲染）。

---

## 5. 暴露层（DTO + 契约）

| DTO | 新字段 | 语义 | 位置 |
| --- | --- | --- | --- |
| `AssetItem` | `rawWidth` / `rawHeight` | RAW 文件的**展示尺寸**（无 RAW → `null`） | `src-tauri/src/browse.rs`（`From<AssetRow>`） |
| `AssetItem` | `latestWidth` / `latestHeight` | **latest 的输出尺寸**（无编辑栈 → `null`；tiles 比例/看图初值用它） | `src-tauri/src/browse.rs`（JOIN `develop_stacks`） |
| `IssueDto` | `width` / `height` | 定稿的**输出尺寸**（可空；映射 `issues.output_*`） | `src-tauri/src/issues.rs` |
| `VariantSnapshot` | `width` / `height` | 该变体的展示尺寸（`display_size` 结果，可空） | `crates/raybend/src/export.rs::snapshot` |

- `AssetItem.width/height`（既有字段）保持"基准位图的展示尺寸"语义，但**改从 `asset_files.display_*` 取**
  （SQL 里展示文件已经是 `f`，直接加两列；NULL 时退回现有 `oriented_dimensions` 计算——兼容老库）。
  这样"新导入"和"懒回填过的老库"都走同一列，只有从未回填的行才走旧算式。
- `AssetRow` 增加读 raw 行的 `display_*`（子查询或第二个 LEFT JOIN；表上有 `asset_id, role` 索引，代价可忽略）。
- **契约三处同步**：Rust 结构 → `src/api/dto-contract.json` → `src/api/types.ts`；
  `src-tauri/src/contract.rs` 与 `src/api/dto-contract.test.ts` 必须绿（漏一处会当场红）。
- `export_variant_details`（`src-tauri/src/export.rs`）：`width/height` 改为 `display_size(...)`（毫秒级）；
  **渲染只为直方图**保留（可进一步：直方图也走缓存，本单元不动）。

---

## 6. 消费方改造

### 6.1 前端唯一读口（新增 `src/lib/display-size.ts`）

```ts
export interface DisplaySize { width: number; height: number }
/** 当前显示变体的尺寸：sooc→基准；raw→asset.rawWidth/Height；latest→asset.latestWidth/Height（无栈回退基准）；
 *  issue:N→该定稿的 width/height。全部是库里已有值，无读取、无渲染。 */
export function displaySizeOf(asset: AssetItem, choice: string | undefined, issue: Issue | undefined): DisplaySize | null;
```

改造点（每处都只读它，不许自己算）：
- `features/browse/grid-source.ts`：`aspect` / `naturalOf` 改用它（变体尺寸已有 → 不再等 details）；
- `features/browse/display-variants.ts`：`DisplayChoice.natural` 直接取 `VariantSnapshot.width/height`（毫秒级到位），
  `details` 只负责直方图（`natural` 的"第二阶段"取消）；
- `components/ui/viewer/photos.ts` + `viewer/store.ts`：`ViewerPhoto.natural` 由上面那份来（见 §6.2）；
- `workspaces/browse/BrowseWorkspace.tsx`：**flow info** 的 `selectedMetadata.select(path, assetItemExif(item))`
  增加"显示尺寸覆盖"（建议 `select(path, fallback, sizeOverride)`：覆盖优先于文件回读的 EXIF 尺寸）；
  值 = `displaySizeOf(...)`；
- `features/browse/BrowsePanels.tsx`：右栏「文件信息」的尺寸行同源（同一事实只允许一个表达）；
- `components/ui/PreviewFrame.tsx` 的 `natural`、`features/photo-grid/viewing.ts` 的 compare `naturalOf`：同源；
- `features/fullscreen/FullscreenViewer.tsx`：payload 携带 `VariantSnapshot.width/height` → 同样喂 `natural`。
- **编辑器本轮不改**：它走 editor viewport 自己的尺寸口径；在实施记录里标为后续统一项。

### 6.2 view 按该尺寸显示（本单元的行为变更，崔总 2026-10-11 定）

现状：`Viewer.tsx` 的 `<img>` 没有 width/height，只有 `transform: scale(zoom)` →
**屏幕尺寸 = 位图固有尺寸 × zoom**，而 fit/pan/百分比按 `natural` 算 —— 两套口径。

改为：**布局尺寸 = 变体尺寸**（`natural`），位图只提供像素：

- `<img>` 显式 `style={{ width: natural.width, height: natural.height }}`（都 > 0 时才写），
  `transform`（居中 + pan + scale）不变；
- `natural` 以**库里的变体尺寸**为准：`show()/goTo()/focus()` 时就从 photo 带入；
  `img.onLoad` 的 `setNatural` **只在"没有已知尺寸"时兜底**（老库 NULL / 未入库的照片 / 全屏外来图），
  不再用位图固有尺寸覆盖已知口径（这条就是"2M 的 RAW 被压成 1M 大小"的根因）；
- 于是 fit / pan / `visibleRect` / 缩放百分比 / clampPan 与布局**同一个基准**；「100%」= 该尺寸下的 1:1
  （预览像素被放大到该尺寸；真正的原生 1:1 像素视口属 M2-W3，不在本单元）；
- `setNatural` 里"从小图到大图保持屏上大小"的比例换算保留（只服务兜底路径）；
- 未入库 / 无尺寸的场景保持今天的行为（固有尺寸布局），**零回归**。

---

## 7. 迁移与升级纪律（回顾 2026-09 定案，本单元逐条遵守）

1. **只走现有迁移框架**（`store/migration.rs` + `store/migrations/*.sql`），四件事一个不少：
   - **版本闸门**：库 `user_version` 比程序支持的版本新 → **拒绝打开**（防止旧程序写坏新结构）；
   - **迁移前快照**：`VACUUM INTO` 到 `<app data>/backups/`（`BACKUP_KEEP=7`，catalog 文件名带库 ID 标签，
     不挤掉别的库的快照）；
   - **逐条事务**：每条迁移单独事务，失败整条回滚且 `user_version` 不前进（绝不允许半迁移的库）；
   - **完整性检查**：迁移后 `foreign_key_check` 暴露"迁移写坏数据"。
2. **不在 SQL 里做重活、不做生成**（2026-09-29 口径）：本次迁移**只有 `ALTER TABLE ... ADD COLUMN`** ——
   不读文件、不生成派生图、没有"升级中"的长任务，因此前端 `MigrationGate` 遮罩只需覆盖毫秒级 SQL
   （现有实现即够，不新增遮罩语义、不做进度条）。
3. **补尺寸是派生数据、可重入、可中断**：NULL = 未知；回填失败**不**升级失败；绝不把"补尺寸"变成升级阻塞项
   （对照 `todos/2026-09-29-catalog-preset-image-upgrade.md` 里"必需预置图"那类情形，本次不属于）。
4. **只向前迁移，没有 down**：回退 = 用 `backups/catalog_<库ID>_*.db` 快照恢复（实施记录里复述恢复步骤）。
5. **新列可空**：即便旧程序绕过闸门也读得懂（不加 NOT NULL / CHECK 强约束）。
6. **不写回照片、不动 XMP**（§2.19）：本次改动不产生任何进 sidecar 的数据 —— 实施记录里写明
   「XMP 侧影响：无」。不改目录/命名/`_RAW/` 规则。
7. **命令体系**（§2.15）：无新用户功能 → 实施记录写明「不接入命令注册表及理由」。
8. 迁移的**测试**：v18 → v19 升级后列存在、旧行全 NULL、行数与既有数据不变；闸门（新库被旧程序拒绝）已有测试覆盖，
   本单元补一条"v19 库在前向版本下的行为"。

---

## 8. 边界与风险

| 场景 | 行为 |
| --- | --- |
| 库离线 / 文件 `missing_since` 非空 | 尺寸 NULL → 回退（§4 矩阵）→ flow info 显示基准尺寸，不报错 |
| 坏 RAW（读头失败） | 该文件三列 NULL；tiles 用占位比例（现状） |
| 位图与 RAW 方向不同 | 各按自己那行（`asset_files.orientation`），不用 `assets.orientation` 代替 |
| 定稿被删 / 重名 / 改名 | 行删则尺寸随行消失；改名不动尺寸 |
| 编辑栈改了（latest 变） | 与栈**同一笔事务**写（`develop::save` 是唯一写入口）；回填只在写前比较、不一致才改；有测试钉住「栈变→尺寸跟着变」 |
| 升级中断（断电/崩溃） | 单事务 + `user_version` 未前进 → 下次重跑；快照兜底 |
| 性能 | 迁移 O(1)（ALTER）；`display_size` 是单行/两行查询 + 纯算术；回填只写缺值/不符值 |
| 老库未回填时 | **读侧现算**（latest/定稿用库里的栈与源尺寸，零 I/O）→ 比例正确；`raw` 的**绝对值**在文件行回填前退到基准（`assets` 原始轴 + 方向），切变体时顺手回填 |

---

## 9. 测试

**本轮（2026-10-11）新增的钉子**：

* `raw_output_size_follows_rawlers_crop_default_rule`（`raw/rawler_backend.rs`）：
  照抄 rawler `CropDefault` 的四条规则（crop_area → active_area → 不裁；adapt；四色 CFA 半分辨率）。
* `display_size_source_prefers_rawler_and_never_invents_for_unknown_raw`（`store/sizes.rs`）：
  位图原样返回；RAW 读不到 rawler 的数就**不动它**（不编数、不拿文件头顶）。
* `sooc_and_raw_fall_back_to_the_other_file_then_to_assets`：补了「RAW-only 且 RAW 行没值 → 未知」。
* `latest_keeps_the_stored_value_and_the_fill_only_fills_nulls` +
  `ensure_sizes_fills_missing_output_sizes_then_stops`：存值优先；缺值保底**算完写回**、幂等。
* `size_refresh_migration_nulls_only_raw_based_rows`（`store/migration.rs`）：
  v20 只置空 RAW 那条链（位图行、sooc 基的栈与定稿一个字节都不动）。


**Rust**
- `store::sizes`：矩阵全覆盖（sooc/raw/latest/issue × 有/无文件 × 恒等/裁切几何 × 有/无方向 × NULL 回退）；
- 迁移：v18 库 → v19（列存在、值 NULL、数据不丢、`user_version=19`）；连续版本号测试自动覆盖新条目；
- 导入写入：RAW+JPG 对两条 `asset_files` 的展示尺寸与方向正确（含 orientation=6/8）；
- 回填：幂等（跑两次，第二次零写入）、只写缺值/不符值；
- `snapshot`：`width/height` 正确（含 issue 与 latest 的几何）；
- **身份不变**：固定样例栈的 `profile_hash` / `signature` 与改动前一致（钉死“尺寸不进身份”）；
  既有缩略图/大图缓存命中不受影响；XMP 包字节不变；
- **栈与尺寸同写**：`develop::save` 写裁切/旋转后，`develop_stacks.output_*` 同事务更新；
  新增写入路径（将来）漏写尺寸时，测试或 lint 能暴露（至少单测钉住现有入口）；
- `browse`：`AssetItem.width/height` 仍为基准展示尺寸；`rawWidth/rawHeight` 为 RAW 展示尺寸；
  `latestWidth/latestHeight` = 栈输出尺寸（无栈 NULL）；
  ❗**2026-10-11 真机修正**：栈行存在但列为 NULL（老库未回填）时，读侧按栈**现算**
  （`store::sizes::latest_size`，在 `store::query::page` 里做，零 I/O）——
  不许退回基准尺寸：那是另一个口径（未裁切的画面），比例可以完全不同
  （真机症状：裁切过的 latest 被拉成原图的 4:3）。定稿同理（`store::issues::list`）。

**前端（`node:test`）**
- `displaySizeOf` 矩阵；
- `display-variants`：snapshot 一到 `natural` 就有值（不再等 details）；
- flow info：切 SOOC / RAW / issue 时 `selectedMetadata.data()` 的尺寸跟着变（含 sizeOverride 优先级）；
- viewer：`natural` 已知时 `onLoad` **不覆盖**；未知时仍兜底（防回归）；
- 契约测试（`pnpm test` 内的 `dto-contract`）绿。

**人类项**
- Windows 真机：RAW+JPG（尺寸不同 + 竖拍）一张，切 SOOC / RAW / 两条定稿：
  ①flow info 尺寸/MP 逐项变化且与所选一致；②view 的 fit 与 100% 以该尺寸为准；③tiles 比例正确、不再"跳一下"。

---

## 10. 实施顺序（建议作业单）

1. migration 文件 + 注册 + 迁移测试（先让库能升到 v19）；
2. `store/assets.rs` 写入 + `store::sizes` 解析 + 单测；
3. `develop::save` 写 latest 尺寸（同事务）+ `issues::create` 写定稿尺寸；
   `issue_library`/`dir_meta_ensure`/backfill 懒回填 + 单测；
4. DTO 三处 + 契约 JSON/TS + 契约测试；
5. 前端：`lib/display-size.ts` → grid/display-variants/viewer/flowinfo/右栏/compare/fullscreen；
   view 的布局尺寸改造（§6.2）；
6. `export_variant_details` 尺寸改读库；
7. 全量质量门 + 冒烟 + 实施记录 + memory 同步。

---

## 11. 纪律同步（做完必须改）

- `memory/FUNCTION-IMAGING.md` §3/§4：补「显示尺寸口径」一段（多版本尺寸、`display_size`、与三档图规格的关系）；
- `memory/FUNCTION-BROWSE.md` §5.9/§5.10（右栏 / flow info）与术语表：尺寸随显示变体；view 按该尺寸布局；
- `memory/ARCHITECTURE.md` §6.4：catalog 表结构描述补新列（如那里有清单）；
- `implementations/YYYY-MM-DD_display-size-source.md`：迁移实测（v18→v19、快照路径）、回填实测、
  真机结论、XMP 侧影响：无、命令体系：不接入及理由。

---

## 12. 不做（明确排除）

- 不做 1:1 原生像素视口（Screen 档仍是长边 1920；那是 M2-W3）；
- 不改缩图规格（**两档**：grid 384 / screen 1920；`strip` 192 于 2026-10-11 取消，见
  `specs/thumbnail-tiers-and-import-memory.md`）与渲染/导出算法；
- 不把尺寸写进 XMP；不改目录与命名规则；
- 不新增用户命令、不改快捷键；
- **不改** `assets.width/height` 的"原始轴 + DTO 归一"既有语义；
- 不动编辑器的尺寸口径（标为后续统一项）。
