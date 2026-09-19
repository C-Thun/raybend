# 换筛选时窗口内容不动 + `+RAW` 三形态标签 + 右栏标签展示

完成时间：2026-09-19 19:33:40 CST

## 本次改动的范围

1. **锚定**：进入 / 离开 / 改变筛选条件时，让「当前那张」照片停在它原来的位置上（窗口内容不动）；
2. **三种 tile 形态的标签**：只有位图写扩展名、只有 RAW 写 `RAW`、**位图 + RAW 写 `+RAW`**；
3. 右栏补上**标签展示**（此前完全没显示），标签弹窗第一次点保存没反应的根因修复。

## 涉及文件

| 文件 | 改了什么 |
| --- | --- |
| `crates/raybend/src/store/query.rs` | `AssetRow.has_raw`（`EXISTS(… role = 'raw')`）+ 测试 |
| `src-tauri/src/browse.rs` | `AssetItem.has_raw` |
| `src/api/types.ts`、`dto-contract.json`、`dto-contract.test.ts` | 契约同步 |
| `src/lib/virtual-window.ts` (+ 测试) | 新增 `rowTop(rows, index)`：第 N 行的顶边 = 前面所有行高之和 |
| `src/components/ui/VirtualGrid.tsx` | 新增 `scrollTo`（精确钉位，按 `key` 去重）+ 滚动容器标记 `data-virtual-scroller` |
| `src/features/browse/BrowseGrid.tsx` | 锚定逻辑：持续记参考照片 → 筛选指纹变化时记待钉目标 → 新数据铺好后钉回原位 |
| `src/features/browse/BrowseGrid.tsx` | `tileTag()`：三形态标签 |
| `src/features/browse/BrowsePanels.tsx` | `AssetTags` 组件（右栏标签段） |
| `src/features/browse/TagDialog.tsx` | `save()` 先把输入框里未提交的名字落成一条 |

## 关键决策与理由

### 锚定（窗口内容不动）

- **参考照片 = 「第一条可见行里的第一个 tile」**，记录它离滚动容器顶边的像素距离。
  人类提的「以选中图片为基准」在这里就是：**待钉目标优先用锚点那张**（新列表里找得到的话），
  找不到才退回刚才记下的那张可见照片。
- **被筛掉了就什么都不做**：硬滚到「第 N 行」在筛掉一大半的情况下会把用户甩到别处，比不动更糟。
- **不新增滚动 API**：`VirtualGrid` 只多一个 `scrollTo`，数学仍在 `lib/virtual-window.ts`
  （`rowTop` 与 `focusRow` 用的 `rowScrollTop` 共用同一份行高累加，有单测）。
- **`key` 去重**：只有指纹变了才执行一次，否则每次渲染都会把用户正在进行的滚动拽回去。

### `+RAW` 标签

- 判据来自**数据**（`is_raw` = 展示的是 RAW；`has_raw` = 这个资产还有 RAW），不在前端猜。
  展示文件的选择规则（有位图就位图）本来就在 SQL 的 `MIN(role)` 里，这里只是把「有没有 RAW」也问出来。
- `RAW` / `+RAW` 的词面走语言包（`browse.raw`），不在代码里硬写。

### 右栏标签

- 名字取 `store.tags()`（词典，按需拉一次、幂等），id 取 `store.markings()`；**词典没到就显示 `#id`**，
  不装作没有标签。
- 弹窗里新建的标签立刻 `rememberTag` 进词典 —— 右栏当场就能显示，不必等下次刷新。

### 「第一次点保存没反应」

- 真因：用户打完字直接点保存，而保存只提交「已挂上 / 已摘掉」的清单 ——
  输入框里那串字既没挂上也没保存。修法是 `save()` 开头先 `commitInput()`（与回车同一个入口）。

## 验证方式（冒烟，Agent 侧）

- `pnpm typecheck` / `pnpm test`（699）/ 三条 lint / `pnpm build` 全过；
- `cargo test -p raybend -p raybend-desktop`（805 + 44 + 6 + 2 + 2）、clippy 无告警；
- `timeout 300 pnpm check:browse` → `problems: []`（含上一批的筛选态断言）；
- `pnpm check:win` → exe 比 dist 新、4 个资源名全命中。

## 遗留问题

- **未经人类目视验证**：锚定手感（换筛选时是否真的「不动」）、`+RAW` 与 `RAW` 的可辨识度、
  右栏标签段的排版。
- 胶片带（`browse film`）只保证「当前那张还在视野里」，**没有**做像素级锚定 ——
  横向胶片的锚定策略要等人类看过网格版效果再定。
- 锚定只在**筛选指纹变化**时触发；换目录 / 换库仍然回到顶部（那是 `resetKey` 的既有语义）。
