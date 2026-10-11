# 基础组件清单进 AGENTS.md（§12）

2026-10-11 14:58:59 完成。

崔总：「我们其实基础组件没那么多 —— tips 组件、相片拖动组件、弹窗组件、树组件、tab 组件、
面板组件、拉杆、几种按钮……也就 20 到 30 个？你整理一下，在 AGENTS.md 里也列一下，
明确需要使用基础组件时从这里挑、复用；以后要是做了新的认为算基础组件的，再添加进这个列表里。」

---

## 一、做了什么

**新增 `AGENTS.md` §12「基础组件清单（要用就从这里挑）」**，分三层：

| 分组 | 内容 |
| --- | --- |
| 12.1 原语（无业务含义） | `Button` / `IconButton`、表单四件（`Checkbox` / `RadioCircle` / `Switch` / `Input`）、`SegmentedControl`、`CompactChoice`、`Slider`、`Tooltip`、`Menu`、`Dialog`、`ToggleBlock`、`Badge`、`PathText`、`TreeNode`、`Panel`、`SplitHandle` / `SplitStack`、`ScrollBox` / `SubtleScrollbar`、`VirtualGrid`、`StateWatermark`、`Toast`、`MetadataRows`、`ToolsSeparator`、`Histogram` / `HistogramPanel`、`DisplayColorStatus`、`Tile`、`PhotoMarks`、`tokens.ts`（令牌读值） |
| 12.2 范式组件（全项目唯一一种做法） | `EasyCopy`、`EasyDestroyButton` / `EasyDestroyHost`、`RemoveButton` / `RemoveGlyph`、`DragPreview`、`PreviewFrame`、`ProfileSelect`、`RepositoryCard` |
| 12.3 成套共享件（跨工作区组合） | `TilesShell` / `TilesControlBar` / `PhotoStatusBar`、`Viewer` / `ViewerControls`、`FilmStrip` / `CompareView` / `photosFromSource`、`PhotoGrid` / `PhotoViewingStage`、`DirTree`、`ExifStrip` + `createSelectedFileMetadata`、`CommandPalette` / `ShortcutSettingsDialog` |

每行给 **组件名 → 文件 → 一句话职责**；并写清四条使用纪律：

1. 先用这份清单里的（不够就改它、一般化它），**不写第二个**（§2.12）；
2. 没有才新建，且**同一次改动里补进这张表**（§2.12 的末尾也加了这句 + 指向 §12 的指针）；
3. 本表是**索引**；**视觉规格**在 `memory/DESIGN.md` §10.1（那一节的末尾补了一行「实现索引见 AGENTS.md §12，两边要同步改」），画稿在 `design/*.pen`；
4. 「看一眼全员」的入口：dev 下 `/dev/kitchen-sink`（`src/dev/KitchenSink.tsx` 画廊，`pnpm smoke:ui` 也在校验它挂着）。

**不列进来的**（写进文末）：`src/shell/`（外壳 chrome 各一份，但不是可复用原语）、
`src/features/<模块>/` 的工作区专用件、`src/lib/`（纯逻辑，无组件）。

## 二、整理时核对的事实

* 组件都在 `src/components/ui/`（43 个条目，含 `tiles/` 与 `viewer/` 两个子目录与若干 `.ts` 逻辑件）；
  跨工作区的三件套（照片区控制器 `features/photo-grid`、目录树 `features/dir-tree`、
  flowinfo `features/exif-strip`、命令面板 `features/commands`）住在 `features/` 但按红线就是共享件；
* `memory/DESIGN.md` §10.1 本来就有一份编号的「第一批最小基础组件」（17 个，代码注释里 `#1..#17` 就是它）
  —— 那是**设计规格**清单，与本次的**实现索引**分工不同，所以 §12 只引用它、不复制它的状态/密度口径
  （避免两份规格各自漂移）；
* 分层纪律照抄 `lint:arch`（`components/ui` 不许 import `features/` 或 `api/`）+ `lint:colors`（不许硬编码色值）
  + i18n（不许硬编码中文），写进 §12 的引言，免得挑组件的人先踩这三条。

## 三、验证

* 文档改动：`AGENTS.md`、`memory/DESIGN.md`（Markdown 检查干净；两个文件的相对引用（§2.12 / §11.2 / §11.4 / §11.5 与 `memory/DESIGN.md` §10.1 / §12.3）逐条核对过存在）；
* 组件名与文件逐条对着 `src/components/ui/**` 的导出核过（不做猜测）；`readTokenPx` / `MenuItemSpec`
  这类「是函数或类型，不是组件」的条目改写成注明（§12.1 最后一行）。
* 无代码改动 —— 无需重跑质量门（本次之前的代码改动已在同一轮里跑过：
  `cargo test --workspace` 1492 + 134 全绿、`pnpm typecheck / test(1279) / lint:colors / lint:arch / lint:i18n / build` 全绿、
  `pnpm smoke:ui` → `problems: []`）。

## 四、遗留

* 清单是**手写索引**（不是自动生成）：改/删组件时请顺手改对应行 —— 现在只靠纪律；
  若以后真有人漏改，可考虑加一条 `lint`（扫 `components/ui` 的导出与 §12 的表格比对），本轮不做。
