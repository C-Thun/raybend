# browse tiles 左下角「编辑 / 定稿」标记实装

完成时间：2026-09-29 14:36:24 CST

## 背景与需求

2026-09-22 崔总在 `Tile` 角落覆盖层收口时留了左下角空位（`data-tile-corner="issue"`，
当时只定了位置与职责，图形刻意留白）。2026-09-29 崔总给出口径并确认可理解后实装：

- **两个图标**：编辑过图标（**毛笔**，崔总在 pencil/brush/wand/adjustments 中选 brush）与
  issue 图标（**扇形展开的卡片** → Tabler `cards` 的 filled 变体）；
- **互斥规则**：没定稿但编辑过 → 只显示毛笔；**只要有一个 issue 就不显示毛笔**，
  改显示卡片图标 + 右侧数字（1–100 = 命名定稿数，SOOC/RAW/latest 不计入）；
- **样式**：浅色 + 深色勾边、无底色（与按 `i` 的常显标记同款，不是 RAW 那种实底药丸）；
- **显隐**：与 `RAW`/`+RAW` **完全一致、一起进退**（选中 / 指向 / 键盘聚焦 / `marks-name`
  强制条 → 退场，与信息条二选一）。

「编辑过」的判定口径由崔总在两个选项中拍板：**与原始源不同才算**（进编辑器动了又全部
改回默认不算编辑过）——正好等于后端既有的 `DevelopStack::is_empty` 反值（`develop.rs`
注释本来就写着「浏览和导出不得各自维护第二套 SQL 判据」，本次遵守：批量版
`has_edits_batch` 复用同一判据，只是把「哪些照片有栈」合成一趟 `IN` 查询）。

## 改动范围

| 文件 | 改了什么 |
| --- | --- |
| `crates/raybend/src/store/develop.rs` | 新增 `has_edits_batch`（分页批量判「编辑过」，chunk 500 防 SQLite 变量上限；判据仍是 `is_empty`）+ 单测（空批 / 不存在 id / 动过参数 vs 只剩自动基线） |
| `crates/raybend/src/store/query.rs` | `AssetRow` += `issue_count`（`issues` 计数子查询，100 夹取）与 `edited`（`page()` 统一批量充实）+ 单测（未编辑 / 只编辑 / 两次定稿三态） |
| `src-tauri/src/browse.rs` | `AssetItem` DTO += `issueCount` / `edited`（契约测试自动覆盖） |
| `src/api/dto-contract.json` | `AssetItem` 键表 += `edited` / `issueCount` |
| `src/api/types.ts` / `dto-contract.test.ts` | TS 侧接口与键表同步 |
| `src/features/browse/store.test.ts`、`src/features/exif-strip/from-asset.test.ts` | 夹具工厂补两个新字段 |
| `src/components/ui/tiles/source.ts` | `GridItem` += `issueCount?` / `edited?`（导入侧不传） |
| `src/features/browse/grid-source.ts` | 适配器把两个字段带进 `GridItem` |
| `src/features/photo-grid/PhotoGrid.tsx` | 装配处把两字段传给 `Tile` |
| `src/components/ui/Tile.tsx` | 左下角空位实装：两态互斥渲染、勾边样式、与 RAW 同套显隐；模块注释从「预留位」改为实装口径 |
| `src/index.css` | 新增 `.tile-info-icon-under`（描边型图标的「粗描边副本垫底」勾边） |
| `src/dev/KitchenSink.tsx` | tile 样例：`+RAW` 样例带 2 个定稿（两角同显隐的可点选验证）、100 封顶样例、毛笔样例 |
| `scripts/ui-smoke.mjs` | 断言升级：标记内容（图标存在、数字 1–100、毛笔无数字）、结构与 RAW 同套（角落层 / 不进照片盒 / 可见）、选中后**两角标一起退场**；顺手把 L77 既有 `new URL(requestedUrl)` 包进 try/catch（上批存储恢复引入的裸调用，检查项挂了） |
| `memory/DESIGN.md` | §12.9.1 左下行从「预留位」改为实装；新增 §12.9.2 完整口径（两态、数字、样式、判定、不接命令） |
| `memory/FUNCTION-BROWSE.md` | 新增 §5.1.2「tile 四角标记」摘要（指向 DESIGN 唯一事实源） |
| `memory/FUNCTION-IMAGING.md` | 定稿序号段后补「browse tiles 左下角定稿标记」一段 |

## 关键决策

1. **毛笔用「粗描边副本垫底」而不是 8 向 drop-shadow**：Tabler 没有 `brush` 的 filled
   变体（有 `cards` 的），`.tile-info-icon` 的 `paint-order` 招数只对带 fill 的图标成立；
   drop-shadow 滤镜在大网格滚动时每个 tile 两个太奢侈，所以叠一层
   `.tile-info-icon-under`（stroke 3 / 勾边色）在正稿下面，零滤镜成本。
2. **判定不另立 SQL**：`issue_count` 是纯计数（子查询走 `idx_issues_asset_time`）；
   `edited` 由 `page()` 拿到本页行后调 `has_edits_batch` 批量充实——判据仍是
   `DevelopStack::is_empty` 那一份，没有第二套真相。
3. **两态互斥在 `Tile` 里判**（`issueCount > 0` 优先），规则写在角落层注释里；
   `PhotoGrid` 只透传数据，不做显示推导。
4. **命令体系接入评估（AGENTS.md §2.15）**：纯显示规则（数据来自浏览查询），
   不是用户可触发功能——不接命令注册表，无默认热键（与 `RAW` 角标同口径）。

## 验证（Agent 侧：单测 + 冒烟）

| 项 | 结果 |
| --- | --- |
| `cargo test --workspace` | 1326 通过 / 0 失败（含 `has_edits_batch`、`issue_count_and_edited_flag_come_through_the_page`、契约测试） |
| `cargo check --workspace` | 通过 |
| `pnpm typecheck` / `pnpm test`（1114）/ `lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build` | 全绿 |
| `pnpm smoke:ui` | 全绿（`problems: []`）。量到：卡片+数字与毛笔两种形态都在角落层、不在照片盒、未选中时常显；数字样例含 2 与 100（1–100 断言过）；点选 `+RAW` 样例后 **RAW 角标与左下标记同时 `display:none`**、文件名条常亮 |

中途一次红：新样例误带 `context="library"`，触发「库内标记区 = 2」的既有计数断言；
左下标记不依赖 library 上下文，去掉后绿。

## 未由 Agent 声称验证的 E2E（AGENTS.md §2.8）

- Windows 真机：真库照片上「编辑过 → 毛笔、定稿 → 卡片+数字」的实际观感与勾边可读性
  （深浅主题、亮暗照片上都要看一眼）；
- 大库滚动性能：`page()` 每页多一趟 `IN` 查询 + 少数几张 `load`，理论成本可忽略
  （真实编辑过的照片占比通常很低），但 10 万张级别的手感由人类在真机确认。

## 遗留问题

- 无。
