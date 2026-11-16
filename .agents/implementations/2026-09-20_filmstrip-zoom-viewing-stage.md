# 胶片带缩放与共享看图舞台重构
完成时间：2026-09-20 20:40:20 CST

## 改动范围

- 胶片带缩略 tile 改为 17 档高度（96–240px），默认 141px、宽度 169px；总高度由
  tile 高度 + 上下各 8px 推导，默认 157px。
- 胶片带内普通滚轮继续横向滚动；`Ctrl + 滚轮` 一次调整一档，不增加可见控件。
  尺寸变化被纳入胶片带像素锚定键，缩放前后的参考照片不横跳。
- 修复 tiles statusbar 缩放滑杆拖一格就中断：`TilesShell` 只按 statusbar 是否存在决定
  挂载，配置对象变化通过 accessor 更新，不再重建 `TilesControlBar` / Ark Slider 实例。
- 新增共享 `PhotoViewingController` 与 `PhotoViewingStage`，import / browse 的
  `PhotoGrid + Viewer/CompareView + FilmStrip + statusbar` 只组装一次。
- `PhotoGrid` 进入 view / film / compare 时只设为不可见并禁用指针，不再由 import 的
  `Show fallback` 卸载；退出后保留虚拟列表 DOM、scrollTop，并由网格内部统一收回焦点。
- 命令打开、对比派生、compare-focus 胶片带、三态、对比尺寸补读与锚点移动统一进控制器。
  胶片带选择统一走 `TilesSource.select`，对比当前图统一走 `TilesSource.setAnchor`。
- import 胶片带与网格改为共用 `PhotoGridStore.thumbQueue`，去掉第二条缩略图队列。
- import 的 Tab 三态同步采用 browse 的左右栏隐藏规则，不再只有胶片带发生变化。

## 关键决策

1. 选择集合仍由共享选择模型的 store 持有，因为 toolsbar、命令与库内标记也需要读取它；
   但照片组件内的所有选择交互和 view/film/compare 派生只经过共享舞台与 `TilesSource`，
   工作区不再维护第二套 UI 选择逻辑。
2. statusbar 滑杆问题不是 fit 数学错误，而是受控值变化时整个 statusbar 组件被条件表达式
   重建，导致 pointer capture 丢失；因此修复落在组件生命周期，而不是给滑杆加延时或焦点补丁。
3. 胶片带尺寸档位独立于 tiles 网格档位，也不持久化；每次进入看图使用 141px 默认档。

## 设计与文档

- `design/browse.pen`：两种 FilmStrip 状态同步为默认 157px，StripThumb 为 169×141px；
  Pencil 结构检查显示容器无破损，普通胶片带后续 tile 的裁切是横向滚动内容的预期表现。
- 更新 `design/browse.md`、`BROWSE.md`、`DESIGN.md` 与 `AGENTS.md`，记录尺寸、隐藏手势、
  共享舞台、网格不卸载与 statusbar 实例稳定性红线。

## 涉及文件

- `src/lib/film-strip-size.ts` / `.test.ts`
- `src/lib/selection.ts` / `.test.ts`
- `src/components/ui/viewer/FilmStrip.tsx`
- `src/components/ui/tiles/{source.ts,TilesShell.tsx,TilesControlBar.tsx}`
- `src/features/photo-grid/{viewing.ts,viewing.test.ts,PhotoViewingStage.tsx,index.ts,store.ts,source.ts}`
- `src/features/browse/{store.ts,grid-source.ts}`
- `src/workspaces/{import/ImportWorkspace.tsx,browse/BrowseWorkspace.tsx}`
- `scripts/check-browse-boot.mjs`
- `design/browse.pen` / `design/browse.md`、`BROWSE.md`、`DESIGN.md`、`AGENTS.md`

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：72 个测试文件全部通过（新增胶片带尺寸、共享看图控制器、锚点边界测试）。
- `pnpm lint:colors`：通过。
- `pnpm lint:arch`：通过。
- `pnpm lint:i18n`：通过。
- `pnpm build`：通过；仅保留既有的主 chunk 大于 500kB 提示。
- `pnpm smoke:ui`：通过，`problems: []`；页面、viewer Enter 返回、网格比例、工作区与控制台
  冒烟均无错误。
- `pnpm check:browse`：通过；新增校验 v2 分域持久化、胶片带默认尺寸与 Ctrl + 滚轮档位、
  三条完整通道直方图，以及真实鼠标路径下的标题栏菜单。

## 未由 Agent 声称验证的 E2E

- Windows 真机真实照片库中：拖动 tiles statusbar 缩放滑杆持续不中断。
- import / browse 在长列表中滚到中段，分别进入单图、film、compare 后退出，位置不回顶部，
  import 回车退出后再次回车仍能进入。
- 胶片带 `Ctrl + 滚轮` 的 17 档手感、默认 141px 与最大 240px 在实际窗口高度下是否合适。
