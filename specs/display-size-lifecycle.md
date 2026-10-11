# 展示尺寸的生命周期：谁在什么时候写、缺了怎么保底、老数据怎么修

> 本单元（2026-10-11 定案，崔总口述）——**只管「写入时机 / 保底 / 修正」**：
> 尺寸**从哪来**由 `specs/display-size-source.md` 拥有，view 要什么**像素**由 `specs/view-true-image.md` 拥有。
> 三份互不重复：口径 → 像素 → 生命周期。
> 落点是 todo #19（定稿尺寸 = 实际裁切）与本轮暴露的老数据问题。

---

## 0. 为什么有这一轮

上几轮把「每个变体认自己那一行尺寸」修完了（`display-size-source.md`）。但**写入时机与刷新责任**没写清，
于是今天连着暴露三件事：

1. **读侧越权现算**：我一度把 `latest_size` 改成「现算优先」，理由是「源尺寸会被修正」——
   这违背崔总的模型：*browse 时数据库里绝对是现成的*，读侧不该现算。
2. **老数据基于旧口径**：RAW 的展示尺寸从「文件头整帧（5264×3904）」改成「rawler 出图（5184×3888）」后，
   老库里 RAW 行与**所有派生值**都还是旧数：真库实测 `asset_files.raw = 5264`、
   `develop_stacks.output_* = 5264`、`issues.output_* = 3712×2784`（后者更是**错值**——
   该定稿 `source_base='raw'`，却存了位图的尺寸）。
3. **缺值行为没统一说法**：有的地方现算不写回（下次还得再算），有的地方回退到别的变体（已禁）。

崔总的口径（2026-10-11 原话与转述）：

> 尺寸和方向标注到 issue 里（包括 latest / sooc / raw），所以这些数据就是在 editor 里随时确认的：
> **sooc 和 raw 导入时就确定，不会变**；**用户保存的 issue 是点「定稿」时保存下来的**；
> **latest 是 editor 里随时调随时写**。理论上这个值可以去实际测（但基于 RAW 的 issue 可能有成本），
> 但只要这套流程做完，**在 browse 时数据库里绝对是现成的**。

> 如果老数据有缺的，可以**留一个现算保底，算完了存进去**；而且进 editor 后每一操作，
> **涉及到哪部分，哪部分就自然重建**。

> 定稿，**本质上是定稿的实际裁切**。

---

## 1. 写入责任表（唯一事实）

| 数据 | 谁写 | 什么时候 | 之后 |
| --- | --- | --- | --- |
| `asset_files.display_*` / `orientation`（**sooc** = 位图行） | 导入登记 | 导入那一刻（读一次头） | **不变** |
| `asset_files.display_*` / `orientation`（**raw** = RAW 行） | 导入登记 | 导入那一刻（= **rawler 出图尺寸**，见 `display-size-source.md`） | **不变** |
| `develop_stacks.output_*`（**latest**） | `store::develop::save`（editor 每次 confirm 随笔写） | editor 里每次保存 | 存值即权威 |
| `issues.output_*`（**定稿**） | `store::issues` 的建定稿路径 | 点「定稿」那一刻（= 该定稿**实际裁切**的结果：源展示尺寸 × 它自己的几何） | 存值即权威 |
| 任何消费点 | —— | **只读** | 现成 |

**原则**：读侧**不现算**。唯一的例外是**缺值保底**（§2），且保底算完必须**写回**。

---

## 2. 缺值保底：现算 → 立刻写回

任何消费点读到 **NULL**（老库没回填 / 新导入还没登记完 / 源尺寸当时读不到）时：

1. 用**同口径的现算函数**算出来（算法只有一套，`store::sizes::{raw_output_size,
   apply_display_size_source, stack_output_size, issue_output_size}`）；
2. **写回**该行（幂等：只填 NULL，**绝不覆盖已有值**；失败只当「这次没算出来」，不报错、不阻断 UI）；
3. 下次读就是现成的。

成本分级（决定保底可以发生在哪里）：

| 保底对象 | 现算要做什么 | 成本 |
| --- | --- | --- |
| 位图行 | 读一次文件头 | 便宜（µs–ms） |
| RAW 行 | rawler 元数据（`raw_image(..., dummy=true)`） | 几十 ms —— **只在必要时做**，别在写事务里循环 |
| latest / 定稿 | 当前源尺寸 + 存着的几何 | 纯算术（零 I/O）；源尺寸缺 → 先按上面两行补源 |

**「实际测」（真渲染量尺寸）只做最末兜底**：崔总说「理论上可以去实际测，但基于 RAW 的 issue 可能有成本」——
所以正常路径**不测**；先用地（元数据 / 几何 / 已有源尺寸）算。真渲染只在「元数据也拿不到」时才考虑，
且必须落成**可重复执行的测试代码**（AGENTS §2.8），不许临时手跑。

**机器复用（不新造第二套）**：懒补的既有入口就是本节的实现 —— `ensure_file_sizes`（显式入口）、
`dir_meta_ensure`（浏览读头时顺手写回）、`export_snapshots`（切变体时对 ≤8 张补文件行）、
`issues::backfill_output_sizes`（老定稿行）、`examples/backfill-metadata --sizes`（整库一次补齐）。

---

## 3. editor：动哪一部分，哪一部分就自然重建

editor 保存是**唯一**的快照入口（`EditorWorkspace.confirmEdit`，AGENTS §2.21），写入时把**它触及的
派生尺寸**一并对齐：

| editor 里的动作 | 触及的数据 | 重建什么 |
| --- | --- | --- |
| 任意参数 / 几何（裁切、旋转、比例）改动并 confirm | `develop_stacks`（含 `output_*`） | **latest 的输出尺寸**（当前源 × 当前几何，同笔事务） |
| 点「定稿」 | `issues`（含 `output_*`） | **该定稿的输出尺寸**（那一刻的实际裁切） |
| 发现源行尺寸是 NULL（老库） | `asset_files.display_*` | 走 §2 保底补上源，再算上面两项 |

于是「这个资产进过 editor 并保存过」⇒ latest / 定稿尺寸**必然是最新且现成的**，
browse 侧零成本读取。源尺寸本身不由 editor 写（导入时定死，永不变）。

---

## 4. 老数据修正（一次性迁移 v20）

**触发**：本轮改了 RAW 的导入口径（文件头整帧 → rawler 出图尺寸）。老库里：

* RAW 行的 `display_*` 基于旧口径；
* `source_base='raw'` 的 `develop_stacks.output_*` / `issues.output_*` 基于**旧源尺寸**算出来（并留下错值）。

**做法**（崔总点头的便宜方案）：迁移只做 **SQL 一步置 NULL**，不在迁移里开 RAW（大库慢、还要在迁移里调 rawler）：

```sql
-- catalog_0020_display_size_refresh.sql（版本 20）
UPDATE asset_files SET display_width = NULL, display_height = NULL
 WHERE role = 'raw';                       -- 位图行不动（它的源没变）
UPDATE develop_stacks SET output_width = NULL, output_height = NULL
 WHERE source_base = 'raw';
UPDATE issues        SET output_width = NULL, output_height = NULL
 WHERE source_base = 'raw';
```

置 NULL 之后：**§2 的保底会在首次碰到时按新口径重算并写回**；要一次到位就跑
`examples/backfill-metadata --sizes`（整库补齐）。新库不受影响（天然正确）；
位图基的定稿与 latest 不受影响（`source_base='sooc'`，源没变，不置 NULL）。

**同时回退读侧的越权现算**：`store::sizes::latest_size` 与 `issue_size` 回到
**存值优先、缺值才现算**（§2 的保底路径）；本轮我临时的「现算优先」改动撤销，
那条回归测试改写为「保底路径会写回，且第二次读不再算」。

---

## 5. 验收清单

- [x] `latest_size` / `issue_size`：**存值优先**；NULL → 现算 → **写回**（幂等，不动已有值）；
      第二次读不再现算（单测钉住）。
- [x] 定稿尺寸 = **它的源 × 它自己的几何**（`issue_output_size` 唯一算法）；
      真库实例（`asset 3 / issue #8`，源 raw 无裁切）应为 **5184×3888**，不是 3712×2784。
- [x] 迁移 v20 只置 NULL 目标行（`role='raw'` / `source_base='raw'`），
      位图行与 `sooc` 基的栈 / 定稿不受影响；重复执行无副作用（版本闸门）。
- [x] 保底写回发生在**单一写者**上（写事务 / actor），不在读连接里裸写。
- [x] editor confirm 后，该资产的 latest `output_*` 与当前几何一致（同笔事务）；
      点定稿后 issue `output_*` 与那一刻的实际裁切一致。
- [x] 真库 drill（`examples/full-tier-probe.rs`，2026-10-11 19:4x，复制库 `/tmp/rbdrill6`）：
      **迁移刚跑完**（无回填）四个变体 = `sooc 3712×2784` / `raw None` / `latest None` / `issue:8 None`
      —— 宁可知未知，也不报错数（崔总口径）；**批量回填之后** =
      `sooc 3712×2784` / `raw 5184×3888` / `latest 5184×3888` / `issue:8 5184×3888`（该定稿源是 RAW，此前存的错值 3712 已修）。
- [ ] 门：`cargo test --workspace`、`pnpm typecheck && pnpm test`、`lint:colors|arch|i18n`、`build`、`smoke:ui`。
- [ ] 人类（Windows 真机）：browse / editor 里同一张照片四个变体的倍率一致（沿用 `view-true-image.md` 的验收）。

---

## 5.1 回填时机（本轮实现的实际行为）

置 NULL（或新导入还没写）之后，值回来的路是：

| 时机 | 谁做 | 覆盖范围 |
| --- | --- | --- |
| 进 editor（打开照片） | `ensure_sizes`（文件行 + latest + 各定稿） | 当前这张 |
| 取快照 / 切变体（`export_snapshots`） | `ensure_sizes_conn`（≤8 张） | 引用到的那些 |
| 浏览目录（`dir_meta_ensure`） | 只补**位图行**（RAW 行不做私自解算） | 该目录的位图 |
| 维护动作 | `examples/backfill-metadata --sizes` | 整库一次补齐 |

**待补的便利设施**（不是正确性缺口）：库里长期存在 NULL 的 RAW 行时，可以在开库后跑一个
**有界限的后台补齐**（逐目录、可中断），省得用户必须挨个点开或敲 CLI。已登记在
`memory/PLAN.md` 的问题清单里，本轮不做。

## 6. 不做（边界）

* **不**把「真渲染测尺寸」做成常规路径（RAW 上贵；崔总已明确）；
* **不**回算「导入后不会变」的 sooc / raw 尺寸（模型就是不变；真变了那是导入 bug，不是读侧职责）；
* **不**在读侧保留任何「现算优先」（除 §2 的 NULL 保底），browse 必须纯读；
* **不**动 XMP 侧（尺寸不进 sidecar / 不进哈希 / 不进签名，`display-size-source.md` 的身份红线）。

---

## 7. 代码指针

| 位置 | 作用 |
| --- | --- |
| `crates/raybend/src/store/sizes.rs` | 尺寸唯一解析口：`display_size` / `preferred_size` / `latest_size` / `issue_size` / `raw_output_size` / `apply_display_size_source` / `stack_output_size` / `issue_output_size` / `ensure_file_sizes` |
| `crates/raybend/src/store/develop.rs` | `save`（latest 的 `output_*` 同笔事务写） |
| `crates/raybend/src/store/issues.rs` | 建定稿写 `output_*`；`backfill_output_sizes` 老行回填 |
| `crates/raybend/src/store/migration.rs` + `migrations/catalog_0020_*.sql` | v20：置 NULL 老数据 |
| `crates/raybend/src/raw/rawler_backend.rs` | `output_size`（rawler 出图尺寸，唯一读法） |
| `src-tauri/src/source.rs` · `export.rs` | `dir_meta_ensure` / `export_snapshots`（保底写回的现场） |
