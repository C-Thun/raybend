# M2-W2 未完成清单 · PhotoGrid 合并设计（暂停点记录）

记录时间：2026-09-19 21:52 CST
状态：**本文件是待办与设计的真源**，不是完成记录。做完一项就删掉对应段落（或在 `implementations/` 写完成记录后标注）。

---

## 0. 为什么会有这份文档

2026-09-19 人类连提三件事：①「找现成的」写进纪律（已进 `AGENTS.md` §2.12）；
② 干掉 `BrowseGrid`、统一到 `PhotoGrid`；③ 把因 ② 暂停下来的活记下来，免得后面忘。

② 只做了一半就停了（原因见 §2 的「为什么暂停」），所以先落这份文档。
**纪律：这份文档里的事项，任何一条都不许在没写清「已完成部分」的情况下重做一遍。**

---

## 1. 已完成、可验收（本轮，已提交或待提交）

| 项 | 状态 |
| --- | --- |
| 宽度改错了地方：`--panel-w-right` 被误改成 375（只作用于**导入**右列）→ **已恢复 300**；浏览右列改由 `BROWSE_RIGHT_WIDTH = 375`（常量，因为右列不可拖、没必要存偏好） | ✅ |
| 浏览左列最小宽度 +20%：220 → **264**，且拖拽闸门与存储夹取**合成同一份** `LAYOUT_BOUNDS.browseLeftWidth`（原先 `BrowseWorkspace` 手抄了一份，改一处不生效） | ✅ |
| 点空白取消选择：判据由「target 是不是容器」改成「**有没有落在 `role="option"` 上**」——一行里右侧空槽位现在也能取消选择（人类报的那个判定 bug） | ✅ |
| RAW 角标：`Tile.raw` 由 `boolean` 改为**模式** `"raw" \| "plus"`（`RAW` / `+RAW`），导入侧调用点同步；**浏览侧第一次真正接上这个角标**；文件名条回滚成「主名 + 扩展名」 | ✅ |
| 右栏数据面（后端）：`marking::TextField` 六个可编辑字段 + `Op::Text`（可撤销）+ IPC `SetText`；`apply_exif` 补写 `gps_lat/gps_lon`；`catalog_0004` 加 `asset_files.file_created_ms`（扫描读 `created()`，取不到退回 mtime）；`ROW_COLUMNS`/`AssetItem`/TS 类型/契约测试同步 | ✅（`tsc` + `cargo test` 绿） |

---

## 2. 【暂停】PhotoGrid 合并（`BrowseGrid` → 删除）

### 现状：同一个东西有两份

| 能力 | 唯一实现（该留下） | 重复的那份（该删） |
| --- | --- | --- |
| 网格视图 | `features/photo-grid/PhotoGrid.tsx`（458 行） | `features/browse/BrowseGrid.tsx`（702 行） |
| 行模型（切片 / 分组标题 / 行高） | `features/photo-grid/rows.ts` | `features/browse/rows.ts` |
| 时间分组规则（跨天断 / >1h 断 / 未知时间归末组） | `lib/time-group.ts` | `browse/rows.ts::browseGroups`（又写一遍） |
| 片内「按文件名自然序」置换 | —— | `browse/rows.ts::sliceOrder`（只有浏览侧有，属于数据层） |

### 为什么暂停

这次改动**不是改一个文件**，而是：重写 `PhotoGrid`（吃新契约）+ 写两个数据适配器 +
删两份文件 + 改两侧工作区 + 冒烟。做了一半时会留下**编译不过的树**，
那正是 `AGENTS.md` §2.11 说的「污染已完成的部分」。所以停在这里，把设计写全，下一次一次做完。

### 设计（已定稿，照这个做）

**① 唯一契约**（`components/ui/tiles/source.ts`）：网格只认它，不认「导入/浏览」。

```ts
export interface GridItem {
  id: string;             // 稳定 id：导入 = 路径；浏览 = 资产 id 的字符串
  path: string;           // 取图用的绝对路径
  fileName: string;
  ext?: string | null;
  aspect?: number;        // 展示用宽高比（已应用方向、已夹取）
  isRaw?: boolean;        // 展示的就是 RAW → 角标 `RAW`
  hasRaw?: boolean;       // 同一张还有 RAW → 角标 `+RAW`
  missing?: boolean;
  excluded?: boolean;     // 只有导入侧有这个概念
  marks?: { rating: number; colorLabel: string | null; flag: "pick" | "reject" | null; locked: boolean };
}

export interface TilesSource {
  count(): number;                                     // 显示序共几格
  itemAt(index: number): GridItem | null;              // null = 该页还没取回来（占位块）
  itemById(id: string): GridItem | null;
  ensureRange?(start: number, end: number): void | Promise<void>;   // 浏览按页取；导入不实现
  aspectOf(id: string): number;
  naturalOf(id: string): { width: number; height: number } | null;
  ensureNatural(entries: readonly { id: string; path: string }[]): void | Promise<void>;
  slices(): readonly RowSlice[] | undefined;           // 分组（显示序）；undefined = 平铺
  status(): "idle" | "loading" | "ready" | "error";
  error(): string | null;
  reload(): void | Promise<void>;
  scopeKey(): string;                                  // 虚拟列表复位键
  selection(): { ids: ReadonlySet<string>; anchor: string | null };
  select(id: string, mode: "replace" | "toggle" | "range"): void;
  selectGroupRange(start: number, count: number, additive?: boolean): void;  // 「全选这天」要覆盖**未取回的页**
  clearSelection(): void;
  tileStep(): number; setTileStep(step: number): void; commitTileStep(): void;
  thumb(path: string): ThumbEntry; requestThumb(path: string): void;
}
```

**② 行模型改「下标制」**（`features/photo-grid/rows.ts` → 建议随网格一起放 `components/ui/tiles/rows.ts`）：

```ts
interface TileRowModel { kind: "tiles"; key: string; height: number; slots: readonly number[] }
interface GroupRowModel { kind: "group"; level: "day" | "slice"; start: number; count: number; /* 日键、时间范围、unknown… */ }
function buildGridRows(input: { count: number; columns: number; cellSize: number; slices?: readonly RowSlice[] }): GridRowModel[]
```

* 行里放**显示序下标**，渲染时 `source.itemAt(slot)` —— 洞（未加载）自然就是 `null`；
* `RowSlice = { start, count, dayId, dayStart, dayCount, startMs, endMs, offsetMinutes, unknown, id }`，
  **两侧各自算好**（导入：从 `store.grouping()` 转；浏览：从 `timeline()` + `sliceOrder` 算），
  分组规则仍然只有 `lib/time-group.ts` 一份。
* 好处：**「分页 / 洞 / 置换」全部留在数据层**，网格不知道这些概念 —— 那正是 `BrowseGrid` 现在最复杂的部分。

**③ `PhotoGrid` 要吃的东西**（props）：

```ts
{ source: TilesSource; thumbs?: ThumbQueue; viewer?: ViewerStore; focusNudge?: number;
  pinsKey?: string; onInteract?: () => void; watermark?: (s: {status; error; count}) => JSX.Element | null;
  isExcluded?: (id: string) => boolean; class?: string }
```

要把 `BrowseGrid` 里这几件**通用**的事搬进来（它们不是浏览专有）：
滚动锚定（`pinsKey` 变时把参考照片钉回原位）、看图关闭后的**焦点回归**（`focusNudge`）、
按可见范围 `ensureRange`、可见范围 `ensureNatural`、外部可传入 viewer/thumbs、
水印做成 props（两侧文案不同：空目录 vs 没选库）。

**④ 两个适配器**（数据胶水，**不是组件**）：
* `features/import/photo-grid-source.ts`：包 `PhotoGridStore`（`itemId = path`，`thumb(id)` 即 `thumb(path)`）；
* `features/browse/grid-source.ts`：包 `BrowseStore`（`id = String(asset.id)`，绝对路径 = `joinPath(root, relPath)`，
  `itemAt(i)` 走 `store.itemAt(i)`，置换与分页留在这里）。

**⑤ 删除**：`features/browse/BrowseGrid.tsx`、`features/browse/rows.ts`（+ `rows.test.ts`，其分组规则已被 `lib/time-group.ts` 覆盖）。
`BrowseWorkspace` 改用 `PhotoGrid` + `grid-source`。

**⑥ 分层提醒（已踩过）**：`src/components/ui/tiles/x.ts` 里要 import `lib/` 得写 `../../../lib/`（退三层）；
另外 `features/*` **不能互相 import**（`ARCHITECTURE.md` §1.1），所以契约与行模型的类型要么放
`components/ui/tiles/`，要么放 `lib/`，**不要**让 UI 层反向依赖 `features/`。

**⑦ 两条风险（盯着它写）**：
* **显示序置换**：浏览的「片内按文件名排」与「数据按时间序分页」是两套下标 —— 置换只在数据源里做一次，
  网格只认显示序。错了就是**格子与照片错位**（历史踩过）。
* **看图件归属**：导入侧网格自建 viewer，浏览侧与胶片带/右栏**共用**工作区那一份 —— 用「可传入，不传则自建」。

**⑧ 验收**：`tsc` 0 / `pnpm test` / `cargo test` / 三条 lint / `pnpm build` /
`timeout 300 pnpm check:browse`（冒烟里 `role="option"`、`[data-virtual-scroller]` 这些选择器不变，
所以那批断言应当原样通过）/ `pnpm smoke:ui` / Windows 构建 + `check:win`。

---

## 3. 【做了一半】浏览右栏（按人类 09-19 的规格）

**规格（从上到下）**：

* tiles 模式：Exif 详细信息（**可滚动**）→ 标签信息 → 文件基础信息 → 地理信息
* view 模式：全图缩略图（放大时显示当前看的那块框）+ 直方图 → 标签信息 → 文件基础信息 → 地理信息
* 文件基础信息：文件名 / 创建日期 / 所在路径 / 所属库 / 作者（可编辑）/ Description（可编辑）
* 地理信息：GPS 经纬 / Country / Province-State / City / Sublocation（后四个可编辑）

**已完成（后端 + 契约）**：`TextField` 六个字段的落库与撤销、`SetText` IPC、GPS 写入、
`created_ms` 列与扫描、`ROW_COLUMNS`/`AssetItem`/TS 类型/契约测试。

**未完成（前端）**：
1. `BrowsePanels.tsx::AssetInfo` 的**分区与顺序**按规格重排（现在只有「拍摄信息 + 标签 + 文件信息」）；
2. **可编辑控件**（点值 → 输入框 → 回车提交，走 `store.setText(field, value)`，Esc 取消）；
3. 「所属库」需要从工作区传库名进来（`AssetInfo` 现在拿不到）；
4. 创建日期用 `createdMs` 显示；
5. view 模式的全图缩略图 + 视口框（`ViewerReadout` 已有预览 + `visibleRect()` 框 ✓，确认它排在直方图之前即可）；
6. EXIF 侧还**没读** `Artist` / `ImageDescription`（`media/exif.rs` 目前只读器材/曝光/尺寸/GPS）——
   想「导入时自动带出作者」需要补读 + 老库回填；人类已把「库级默认 author」记进 `FUTURE.md` 的意图里。

---

## 4. 【未做】其他已识别的「一个东西两份」与遗留

| # | 重复/遗留 | 位置 | 处理 |
| --- | --- | --- | --- |
| 1 | 库卡片 | `features/repositories/RepositoryList.tsx::RepositoryCard` vs `BrowsePanels.tsx` 内联那份 | 以**导入那份**为准（人类 09-19 明确：拿 import 的替换掉），做成一个纯 props 组件（`name/path/photosCount/online/onSelect/onSettings/onRemount`），渲染差异用参数控制 |
| 2 | 左列（库列表 + 目录树） | `workspaces/import/LeftColumn.tsx` vs `features/browse/BrowsePanels.tsx::BrowseLeftColumn` | 同上：一份组件 + 参数（浏览侧多搜索条与「查看所有库」伪卡片） |
| 3 | 工具条 | `features/browse/BrowseToolbar.tsx` vs 导入侧内联 | 抽成同一份（两侧功能开关用配置传） |
| 4 | 重建数据进度反馈 | `repository_rebuild` 目前没有进度事件 | 走导入那套事件流（`emit` → 前端进度条） |
| 5 | film 的像素级锚定 | 只保证「当前那张在视野里」 | 等网格锚定经人类确认后再定横向策略 |
| 6 | 标签保存链路 | `TagDialog.save()` 已先 `commitInput()`；右栏标签段已加 | **真机验证**：打标签 → 保存 → 关软件重开还在（Rust 侧 `asset_tags` 路径有单测，但真机没验过） |

---

## 5. 给人类的两条提醒

1. **右列宽度**现在是常量 `BROWSE_RIGHT_WIDTH = 375`（浏览）/ 令牌 `--panel-w-right = 300`（导入）——
   如果你希望它也进「设置」或可拖，说一声，我按可拖重做（把手只加左侧是 `DESIGN.md` §8.6 的纪律）。
2. 本次前端改动**没有**目视验证过（我看不到界面）：RAW 角标 `RAW`/`+RAW`、点空白取消选择、
   RAW 角标在指向/选中时消失 —— 这三条请你在真机上过一眼。
