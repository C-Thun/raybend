# 浏览筛选语义（阈值 / 旗标 / AND）+ 看图关闭后的焦点回归

完成时间：2026-09-19 19:31:04 CST

## 本次改动的范围

人类 2026-09-19 的一批批注，集中在**浏览模式的筛选与旗标**上，外加一条键盘 bug：

1. 筛选图标与筛选态语义；
2. 筛选开启后，工具条上的标记**不再跟随选中的照片**，它就是「筛选条件」本身；
3. 星标改成**阈值**语义（选 3 星 ⇒ 4、5 星也出现）；
4. 赞/踩互斥，重复点击 = 取消该条件；
5. 进入筛选时若已选中照片 ⇒ 用工具条当时的标记状态作为初始条件；
6. 旗标说法去掉「留下 / 丢弃」，tile 上的旗标图标不再是星；
7. **bug**：Enter 进看图 → Esc 退出后照片仍选中，但再按 Enter 进不去。

## 涉及文件

| 层 | 文件 | 改了什么 |
| --- | --- | --- |
| Rust 查询 | `crates/raybend/src/store/query.rs` | `Filter.ratings: Vec<u8>` → `min_rating: Option<u8>`（SQL `a.rating >= ?`）；新增 `FlagFilter { mode, ids }`（`pick`/`reject` → `IN (…)`，`none` → `NOT IN`；空列表时 `pick/reject` 给 `0 = 1`、`none` 不加条件） |
| Rust 示例 | `crates/raybend/examples/query-bench.rs` | 跟上字段改名（bench 的星标筛选即「3 星及以上」） |
| IPC | `src-tauri/src/browse.rs` | `FilterDto.min_rating`、`FlagFilterDto`；`AssetItem` 新增 `has_raw` |
| 前端契约 | `src/api/types.ts`、`dto-contract.json`、`dto-contract.test.ts` | 同步 `minRating` / `flag` / `hasRaw` |
| 前端 store | `src/features/browse/store.ts` | 查询前用 `withFlagIds()` 把旗标 id 补进 `filter.flag`；`tagList/loadTags/rememberTag`（标签词典，幂等） |
| 前端筛选 | `filter.ts`、`FilterBar.tsx`、`BrowseToolbar.tsx` | chip 增加旗标类型、文案 `≥N 星`、组合子默认 `and`；工具条在筛选态读 `store.filter()` |
| tile / 网格 | `Tile.tsx`、`BrowseGrid.tsx` | pick 图标 `IconStarFilled` → `IconFlagFilled`；看图关闭后的焦点回归 |
| 冒烟 | `scripts/check-browse-boot.mjs` | 新增筛选态三条断言 + `Enter → Esc → Enter` 回归 |

## 关键决策与理由

- **星标是阈值不是枚举**：`rating >= N`。原实现是 `IN (…)`，用户心里的模型却是「至少 N 星」——
  SQL 换一个比较符就对了，前端 chip 也写成 `≥N 星`，让语义在界面上可见。
- **旗标只有两种模式**：`pick`（有旗标）/ `none`（无旗标）。「不要了」由**删除**表达（有回收站兜底），
  不再有与 `pick` 长得几乎一样的「弃」按钮 —— 点错了看不出来。
- **筛选态 = 条件本身**：工具条在筛选态**不读选中照片**，只读写 `store.filter()`。
  这样点另一张照片不会偷偷改条件（冒烟里有断言：换选中后 chip 数量不变）。
- **`markDisabled()` 在筛选态永远可用**：筛选态下工具条上的标记是「改条件」，不是「给照片打标」，
  没有选中照片时也应该能改。
- **看图关闭后的焦点回归挂在「看图开没开」这个状态上**（`focusNudge` 计数器），
  而不是网格自己的 `viewerActive` 跃变 —— 真机有**多条关闭路径**（Esc、对比态「返回」、关闭按钮），
  只盯其中一条会漏（实测：对比态点返回之后焦点就没人管了）。
- **焦点要补几帧**：关掉看图会触发一次数据重载（标记 / 分页），虚拟列表的行元素整块换掉 ——
  第一帧 focus 上的 tile 下一帧可能已经不在 DOM 里（焦点又掉回 `body`）。
  判据收紧成「**焦点丢了才补**」：焦点已在网格里、或用户点到了别处（工具条、输入框），就不再插手。

## 验证方式（冒烟，Agent 侧）

```bash
pnpm typecheck                 # 0
pnpm test                      # 699 通过
pnpm lint:colors lint:arch lint:i18n
cargo test -p raybend -p raybend-desktop   # 805 + 44 + 6 + 2 + 2 通过
cargo clippy -p raybend -p raybend-desktop --all-targets   # 无告警
pnpm build && pnpm check:win   # Windows 产物与 dist 一致
timeout 300 pnpm check:browse  # problems: []
```

冒烟新增断言（都会在真机 Chrome 里跑一遍）：

- 筛选态旗标区是「有旗标 / 无旗标」两个按钮，且**不存在**「留下 / 丢弃」；
- 换「选中」之后筛选条件条数不变；
- 星标 chip 文案含 `≥`；
- `选中 → 回车进看图 → Esc 退出（焦点必须在网格里）→ 再回车能重新进看图`。

## 遗留问题

- **未经人类目视验证**：工具条在筛选态的手感（点标记 = 改条件）、旗标图标的可辨识度。
- 赞/踩互斥的复用只在 store 层做了收敛（`setFlag` 一条路径），但**跨目录**的语义仍需真机确认。
