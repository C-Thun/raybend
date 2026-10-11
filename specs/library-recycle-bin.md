# 库级回收站（Library Recycle Bin）— 工作单元规格

**状态**：已定案待实施（崔总 2026-10-10 两轮讨论 + 决策答复；设计稿确认后开工）。
**关联**：`memory/FUTURE.md` #10（应用内回收站，本单元结项）；`AGENTS.md` §11.3（删除语义，本单元修订）；
`memory/FUNCTION-REPOSITORY.md` §1（库结构，本单元修订）；`memory/FUNCTION-BROWSE.md` §3.2/§4.3/§5.9；
`specs/xmp-sidecar.md`（增补「库内移动」）；`specs/command-scenes.md`（快捷键场景化，并行单元）；
`crates/raybend/src/store/delete.rs`（现有删除实现，本单元改造）。

---

## 0. 一句话与验收清单

**一句话**：把「删除」的落点从系统回收站改为**库内 `recycle/` 目录**，建立应用内可见、可还原的二级回收站
（`photos/ → recycle/ → 真删`）；文件层是真实移动，数据层只是标记。

**验收清单**（做完逐项勾）：

- [ ] 拖动选中的照片到 mid 左上角垃圾桶卡片松手 → 照片移入库内回收站（无确认），toast 反馈
- [ ] `Delete` 键 / toolsbar「删除」按钮保留，确认弹窗保留（这一步不弹拖动那条路）
- [ ] 移入后：photos 下文件（含 `_RAW/`、`.xmp`）消失，`recycle/<同一相对路径>` 出现；库记录保留（评级/标签/编辑栈/issue/桶成员照旧）
- [ ] 常规视图（网格/胶片/看图/搜索/桶/标签/计数）不再出现回收站照片
- [ ] 左列库目录区顶部出现回收站入口（有内容才出现，带数量），可展开回收站树、点击出图
- [ ] 回收站视图：tiles 低浓辅色底、toolsbar 只剩 `[还原选中] │ [清空回收站]`、固定按删除时间倒序
- [ ] 右列 info 能看到「原位置」
- [ ] 「还原选中」把照片移回原位（目录不存在则重建；同名冲突自动 `_01` 后缀并提示）
- [ ] 回收站内拖动到左上角粉碎卡片 → 真删（无确认）；「清空回收站」→ 强确认后真删全部
- [ ] 真删后：磁盘文件、sidecar、`cache/full/<id>`、DB 记录、FTS 全部清理
- [ ] 清空/还原到空后：回收站入口消失，视图落回 photos 根
- [ ] 命令注册表：`recycle.restore`、`recycle.empty` 登记（默认键留空、理由写明）；真删不进命令表
- [ ] 拖动统一为 pointer 方案（相片桶落点迁移完毕，无 HTML5 拖动残留）
- [ ] 单测（Rust + 前端）通过；`pnpm typecheck && pnpm test && pnpm lint:* && pnpm build` 通过；冒烟通过
- [ ] 纪律文档同步（§9 清单）

---

## 1. 语义与术语（先钉死）

| 术语 | 含义 | 目的地 | 可逆 | 确认 |
| --- | --- | --- | --- | --- |
| **删除 / 移入回收站** | 把选中照片从 `photos/` 移入库内 `recycle/` | 库内回收站 | ✅ 可还原 | 按钮 / `Delete` 键：**要**（弹窗保留）；拖动：**不要** |
| **还原**（restore） | 把回收站照片移回原位置 | `photos/` 原位 | ✅ | 不要 |
| **真删**（purge） | 从回收站永久删除选中照片 | 磁盘 → 消失 | ❌ | **不要**（拖动即执行） |
| **清空回收站**（empty） | 真删回收站内全部 | 磁盘 → 消失 | ❌ | **要**（强确认） |

**与旧语义的关系（纪律修订）**：`AGENTS.md` §11.3 现写「删除 = 移到系统回收站（不提供永久删除）」，
本单元改为「删除 = 移入库内回收站；清空/真删 = 永久删除（只在回收站内可用）」。
「回收站内真删」是唯一永久删除路径，且**只有拖动一条路**（无按钮、无快捷键、无菜单）——这是崔总
对所有删除入口做的减法：拖动是明确手势，其余入口一律不给。

---

## 2. 物理层

### 2.1 目录

```text
<库根>/
├── catalog.db
├── photos/            # 导入落地（不变）
├── recycle/           # ★ 新增：与 photos 同级、镜像 photos 相对路径
│   ├── 2026-08-15/
│   │   ├── MYP0001.jpg        # ← photos/2026-08-15/MYP0001.jpg 移入
│   │   ├── MYP0001.xmp        # sidecar 随行
│   │   └── _RAW/
│   │       └── MYP0001.ORF    # ← photos/2026-08-15/_RAW/MYP0001.ORF 移入
│   └── ...
└── cache/             # 库内大图缓存（不变）
```

- **`recycle/` 是应用管理的保留目录**（像 `photos/`、`_RAW/`、`cache/` 一样写进 `FUNCTION-REPOSITORY.md` §1）。
  用户手动往里放/删内容不受支持；移入与还原只认 DB 里登记的资产。
- **镜像规则**：`photos/<相对路径>` ⇄ `recycle/<同一相对路径>`。原路径由 `asset_files.recycled_from` 记录，
  不靠字符串反推（为将来非 photos 来源留口）。
- 首次移入时 `create_dir_all` 建目录。
- **空目录维护**：还原/真删/清空后，回收站内的空目录由应用自动清理；`photos/` 下的空目录不动
  （用户可用现有「删除空目录」）。
- **清空的强语义**：清空后 `recycle/` 目录整体清干净——包括 DB 未登记的残留文件（应用管理目录的常识语义；
  写进文档警示）。

### 2.2 移动规则

- 移动 = `std::fs::rename`（同卷；库不允许放网络/云盘，`AGENTS.md` §6.4）。**不做 copy+delete 回退**（同卷应总成功；失败如实报错）。
- 一个资产的全部文件一起走：`asset_files` 里 role=`bitmap`/`raw` 的每个文件（含 `_RAW/` 下的）。
- **sidecar（`.xmp`）随行**：仅限「我们的」（`xmp::is_ours`，含 `rb:` 域），与照片同目录镜像；
  解析失败/移不动只记日志不阻塞（sidecar 是镜像资料，不是真相源）。
- **文件不在磁盘上**（`missing_since` 非空 / 实际不存在）：跳过移动，DB 照常标记（还原时同理）。
- **执行顺序**：先移文件、后写 DB（沿用现有纪律与 `check_session` 闸门）。某资产任一文件移失败 →
  **不改该资产的 DB**、进失败清单报告；全部移成功 → 事务内写 DB。

---

## 3. 数据层（「数据层只是加标记」的准确边界）

崔总定的原则：**移入回收站在文件层是实质性动作，在数据层只是标记**。落实为：

### 3.1 schema 变化（迁移 `catalog_0018_recycle.sql`）

```sql
-- assets：是否在回收站（Unix ms；NULL = 不在）
ALTER TABLE assets ADD COLUMN deleted_at INTEGER;
CREATE INDEX idx_assets_deleted_at ON assets(deleted_at) WHERE deleted_at IS NOT NULL;

-- asset_files：进回收站前的原 rel_path（还原用；NULL = 不在回收站）
ALTER TABLE asset_files ADD COLUMN recycled_from TEXT;
```

**除这两列 + `rel_path` 前缀改写外，数据层不做任何别的事**：评级、旗标、色标、标签、文字、地点、
编辑栈、issue、桶成员、AI 分析结果**全部不动**——这就是「还原后一切照旧」的机制来源。

### 3.2 不变量（实施与测试都按这条检查）

> **`asset_files.rel_path` 永远描述文件的真实位置；`assets.deleted_at` 描述它是否在回收站。**

- 移入：`deleted_at = now`；每个文件 `rel_path = 'recycle/' + rel_path`（`recycled_from` 记旧值）。
- 还原：`deleted_at = NULL`；`rel_path = recycled_from`；`recycled_from = NULL`。
- 真删：记录真删（含级联），没有「标记删除」的第二形态。

### 3.3 谓词（唯一来源）

Rust 侧只允许一个可见性判定入口（新增 `store::query::Visibility::{Live, Recycled}` 或等价物）：

- `Live` ⇒ `a.deleted_at IS NULL`（**所有常规查询的默认值**）
- `Recycled` ⇒ `a.deleted_at IS NOT NULL`

**纪律**：新增任何「列资产 / 数资产」的查询不允许绕过这个判定；本单元实施时按 §3.4 清单逐处核对。

### 3.4 可见性审计清单（实施时逐项核对，不假定已完成）

| 位置 | 处理 |
| --- | --- |
| `store/query.rs`（rows / count / ids / grouped / tree / 排序） | 统一加 `Visibility` 条件（核心入口） |
| `store/fts.rs` | 移入时删 FTS 行、还原时重建（`refresh_asset`）；搜索自然不含回收站 |
| `store/organization.rs`（桶成员列表与计数、目录标签） | 走 `query.rs` 的统一条件；若直连 SQL 则逐处加 |
| `store/assets.rs` 统计（`count(*) FROM assets`） | 明确口径：库统计是否含回收站（推荐：常规统计不含；回收站数量由专用查询给） |
| `store/rebuild.rs`（`rescan_library` 重扫 `photos/`） | 重扫不得把回收站资产当「缺失」；跳过 `deleted_at` 记录 |
| `store/backfill.rs`（元数据回填） | 回收站资产不参与「待回填」挑选（无害项可不改，核对后定） |
| `ai/`（待分析选取、`source_key`） | 回收站资产不进入分析队列 |
| `color/assets.rs`（批量取目标） | 前端传 id，无需过滤；核对 |
| `export.rs`（按 id 取文件） | 按 id；前端在回收站视图禁用导出入口 |
| `xmp/adopt.rs` / `collect.rs`（采纳） | 导入期逻辑，与回收站无关 |
| `pool.rs` / `migration.rs` 的内部计数 | 与可见性无关（健康检查/迁移），不动 |
| `xmp/audit.rs`（巡检） | 只扫 `photos/`；回收站内 sidecar 不扫描（不误报，也不覆盖）——记录此选择 |

---

## 4. 视图与交互

### 4.1 左列（`BrowseLeftColumn`）

- **回收站入口行**：位于当前库卡片之下、目录树之上（「库目录区最上方」）；**DB 有回收站资产才出现**。
  - 带回收站图标、明显区别于普通目录的样式（设计稿定）；行尾显示数量（`回收站 · 12`，带变量动态信息，§2.20 合规）。
  - 可展开：展开出回收站内的目录树（复用 `listDirs` 懒加载，读 `root/recycle`；`_RAW` 仍不显示）。
  - 点击入口 = 进入回收站根视图；点击子目录 = 进入该目录视图。选中态与 photos 树互斥。
- **无内容时整行消失**（含展开态）；回收站与 photos 各自记住展开/选择状态。

### 4.2 中列（tiles）

- 复用 `TilesShell` + `PhotoGrid`；`PhotoGrid` 的 store 来源切到回收站范围。
- **tiles 底色**：低浓辅色（设计稿定 token；与「中性面」四级的叠加规则一并给出）。
- **toolsbar mid**：只保留 `[还原选中] │ [清空回收站]`（两枚按钮，按 §12.12 组分隔规则）。
  其余全部不可用（隐藏或禁用由设计定；**禁用而非删能力**的口径见 §4.4）。
- **tiles statusbar**：浏览类照常（尺寸档、排序控件可显示但固定？——**固定按删除时间倒序**，排序控件隐藏；
  信息档位/看图入口保留）。
- **排序**：固定 `deleted_at DESC, id DESC`（最近删的在前）。
- **空态**：回收站被清空/还原空的瞬间 → 入口消失，视图落回 photos 根（toast 告知）。

### 4.3 右列（info）

- 显示常规元信息 + **「原位置」**（`recycled_from` 的目录部分，`shortpath` 口径）；
- 编辑/标记/导出/外部编辑器入口在回收站视图禁用或隐藏（设计定）。

### 4.4 可用性口径（「不能做任何原有操作」的准确边界）

| 能力 | 回收站内 |
| --- | --- |
| 看图（单张/对比）、网格缩放、信息档位、胶片带 | ✅ 允许（纯浏览） |
| 排序 | 固定删除时间倒序（控件不给） |
| 评级/色标/旗标/标签/锁/文字/地点 | ❌ 禁用 |
| 编辑（进编辑器）、定稿 | ❌ 禁用 |
| 导出（含入队）、外部编辑器、AI 分析 | ❌ 禁用 |
| 拖到相片桶 / 目录树 | ❌ 禁用（拖动只有粉碎一个落点） |
| 还原 / 清空 | ✅ 本节入口 |
| 真删 | ✅ 仅拖动 |

实现口径：**不是把按钮藏掉就完事**——对应命令的 `when()` 必须为假（命令面板/快捷键也进不去），
`edit.delete` 在回收站视图 `when` 为假（防二次移入）。

---

## 5. 拖动（统一 `lib/pointer-drag.ts`）

> 崔总的定位：**本期拖动只有一个新增语义 = 删除**（移入回收站 / 回收站内真删）；
> 「拖到目录」是后续单元。但**基础设施一次统一**：tiles 拖动全部改 pointer 方案，
> 相片桶落点从 HTML5 DnD 迁移过来，不留两套（并存会互相抢手势）。

### 5.1 生命周期（与 export 现有实现同构）

1. `pointerdown` 在 tile 上 + 移动超阈值（6px）→ 进入拖动；
2. 拖动中：被拖项抬高/半透明；跟随指针显示**卡片堆叠预览**（export 同款）+ 数量角标；
   - 拖选中项 → 带整批 `selection().ids`；拖未选中项 → 只带它（保持现状语义，`BrowseWorkspace.onPhotoDragStart` 现有规则迁移）；
3. 拖动开始后 mid 区**左上角浮出热区卡片**（淡入 ≤150ms）：
   - 常规视图：垃圾桶图标（圆角矩形卡片）；指针进入热区 → 激活态（放大/主色或危险色描边）；
   - 回收站视图：碎裂/粉碎图标（更强警示）；
4. 松手：在热区内 → 执行（常规=移入回收站；回收站=真删）；区外 / `Esc` / 窗口外 → 取消；
5. 点击抑制：拖动过的手势不触发 tile 单击（沿用 `trackPointerDrag` 的 blockClick 机制）。

### 5.2 约束

- 跨库来源（相片桶/标签的混合来源）下**不给热区**（这些照片不属于当前库，无法定位回收站）；
- 拖动不是唯一入口（按钮 + `Delete` 键在），键盘用户可完成删除（§2.15 无障碍口径：拖动是增强）；
- 拖动期间 busy 闸门：批量移动执行中不接受第二次拖动。

---

## 6. 命令接入（§2.15）

| 命令 | 文案（zh/en） | scope | when | defaultKey |
| --- | --- | --- | --- | --- |
| `recycle.restore` | 还原选中的照片 / Restore selected | `tiles` | 回收站视图 && 有选中 | **留空**（低频、点击不差几秒；避免与浏览场景键位相撞） |
| `recycle.empty` | 清空回收站 / Empty recycle bin | `global` | 回收站视图 | **留空**（同上） |

- **真删不进命令注册表**（崔总定：只有拖动一条路）。故障排查后若要键盘可达，再单独评审。
- `edit.delete` 保持现状（按钮 + `Delete` 键 + 确认弹窗），**when 增加「非回收站视图」条件**。
- 拖动本身不是命令。
- 回收站入口/树的展开不是命令（鼠标交互；`Alt+1/2/3` 系列不扩）。

---

## 7. 工程接入点（实施定位）

| 层 | 位置 |
| --- | --- |
| 移动/还原/清理核心 | `crates/raybend/src/store/delete.rs` 改造（`Trasher` trait 保留测试注入；`SystemTrash` → `LibraryRecycle`） |
| 迁移 | `crates/raybend/src/store/migration.rs` + `migrations/catalog_0018_recycle.sql` |
| 会话闸门 | `check_session`（沿用 `delete_assets_checked` 模式） |
| Tauri 命令 | `src-tauri/src/browse.rs`：`browse_delete`（改语义）+ 新增 restore / purge / empty / 回收站计数 |
| 前端 API | `src/api/browse.ts`、`src/api/db.ts`（回收站树读盘复用 `listDirs`） |
| 状态 | `src/features/browse/store.ts`（回收站模式状态 + 范围查询 + scopePath 平行状态） |
| 确认弹窗 | `src/workspaces/browse/BrowseWorkspace.tsx`（清空强确认；删除确认保留） |
| 拖动 | `src/lib/pointer-drag.ts` + `PhotoGrid` / `BrowseWorkspace` + 热区组件（新建） |
| 左列 | `src/features/browse/BrowsePanels.tsx`（入口 + 树） |
| 命令 | `src/features/commands/catalog.ts` |

---

## 8. 边界与失败处理（逐条有反馈）

| 情形 | 行为 |
| --- | --- |
| 还原时目标已有同名文件 | 自动加 `_01`（与导入重名规则 §3.4 一致；`_99` 溢出继续涨位数）；bitmap/raw/sidecar 同步改名；完成后提示「部分文件还原时已有同名文件，已经改名还原」 |
| 还原时原目录不存在 | `create_dir_all` 重建 |
| 移入时文件被占用/权限失败 | 该资产不改 DB；失败清单进模态逐条列（沿用现有 `deleteFailures` 呈现） |
| 批量中途库离线 | `check_session` 中断；已完成的保持，未完成的报告；不产生半状态资产 |
| 锁 | 一级锁及以上挡住移入（现有规则）；已入站的资产真删不再判锁（入站已挡）；还原不受锁限 |
| 跨库来源 / 离线库 | 拖动热区不给、命令 `when` 为假、入口不可用（沿用 availability 闸门） |
| 清空时回收站内有未知残留文件 | 一并清除（§2.1 强语义），文档警示 |
| 用户手动删了 `recycle/` 里的文件 | 该资产表现为 missing（现有 `missing_since` 机制），还原时按缺失处理 |
| 大数量（1000+ 张） | rename 本身快；执行期间 busy；进度用 busy 态 + 结果 toast（不做进度条） |

---

## 9. 文档同步清单

- `AGENTS.md`：§2.3/§11.3（删除定义与表格）、§6.4/§6.5（库结构与缓存清理口径）
- `memory/FUNCTION-REPOSITORY.md` §1（`recycle/` 保留目录）
- `memory/FUNCTION-BROWSE.md` §3.2（toolsbar）、§4.3（目录树 + 回收站入口）、§5.9（删除流程）
- `memory/DESIGN.md` §12.11 #4（删除确认口径——按钮/键仍确认、拖动不确认）
- `memory/FUTURE.md` #10（结项）
- `specs/xmp-sidecar.md`（增补「库内移动」：移入/还原/真删三动作的 sidecar 伴随）
- `specs/xmp-write-toggle.md` §2.2（「照片进回收站」一行的落点描述）
- `todos/2026-10-10-library-recycle-bin.md`（归档移走）

---

## 10. 测试计划（Agent 侧）

**Rust（`store/delete.rs` 为主）**：
- 移入：单文件 / bitmap+RAW 配对 / sidecar 伴随 / 已 missing 文件 / 锁阻挡 / 空选择 / 会话中断；
- 不变量：移入后 `rel_path` 前缀与 `deleted_at`、`recycled_from` 三者一致；还原后完全复原（含 sidecar）；
- 可见性：移入后 `query.rs` 各入口不再返回该资产（rows/count/ids/tree/FTS）；
- 还原冲突改名：单冲突 / 多冲突连号 / `_99` 溢出 / bitmap+RAW 同步；
- 真删与清空：DB 行、FTS、cache/full 目录清理；清空对未知残留文件；
- 幂等与重复操作（已还原再还原、已清空再清空）。

**前端**：
- 拖动：阈值、hit-test（热区内/外/边界）、多选语义（选中带批/未选中单张）、点击抑制；
- 模式与命令：回收站视图下 `edit.delete` 不成立、`recycle.*` 的 when/enabled、跨库来源无热区；
- 左列：入口出现条件（有/无内容）、展开树、选中互斥。

**冒烟**：`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build && pnpm smoke:ui`。

**E2E（归崔总）**：真机拖动的手感与热区命中、批量 1000+ 的耗时体感、回收站视觉、还原后各视图一致性、
Windows 文件占用场景。

---

## 11. 已知取舍（记录，不实现）

- 真删只有拖动一条路 → 键盘用户无法单张真删（可清空）；后续若要补，单独评审。
- **拖动开始时自动切到相片桶面板**（旧 HTML5 时代 `dragenter` 的便利）本期未接：
  `photo-drag.ts` 的 `beginPhotoDrag` 预留了 `onStart` 回调，工作区暂未传 —— 登记为后续小项。
- `store::assets::counts`（库体检统计）仍包含回收站记录（它按 `count(*) FROM assets` 口径）；
  界面上的「照片数」不读它，无用户可见影响。
- `recycle/` 被用户手动干预（放/删文件）不受支持，只认 DB。
- 回收站不参与任何搜索/筛选/桶；还原后自动恢复。
- 不设自动清理策略（N 天/上限）——登记 `FUTURE.md`，本单元不做。
- `xmp/audit.rs` 不巡检回收站（记录 §3.4）。
