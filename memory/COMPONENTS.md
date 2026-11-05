# 组件库方案（COMPONENTS）

> **这份文件管「同一个东西只允许存在一份」**（`AGENTS.md` §2.12 复用优先的落地册）。
> 写任何界面前先来这里找；找到了就用它（不够就**改它**），确实没有才新建 ——
> 新建完**当场**把它补进本文件（同一次改动，别欠着）。
>
> 建立：2026-11-01（崔总：反复强调复用，要求把库列表整理成方案并写进纪律）。
> 索引在 `AGENTS.md` §12（分工：那边是「谁在哪」，这边是「为什么这么用、怎么扩展、踩过什么坑」）。
> 画稿与视觉规格在 `design/*.pen` + 同名 `.md` 与 `memory/DESIGN.md` §10.1。

## 0. 使用流程（五步，缺一步都算返工）

| 步 | 做什么 | 判据 |
| --- | --- | --- |
| ① 查目录 | 在本文件 §2–§6 与 `AGENTS.md` §12 里按**能力**找（不是按名字找：想要「一整行的拖动排序」，不是「叫 Drag 的组件」） | 找到候选就进 ②；完全没有才进 ④ |
| ② 读源码 | **读**那个组件的实现（不是看名字猜），确认它管什么、边界在哪、有哪些调用方 | 读完仍不确定就先看 `KitchenSink` 里的实例 |
| ③ 复用或一般化 | 能用就直接用；差一点就**在原组件里加参数 / 抽出公共部分**，让旧调用方行为不变 | 改动后旧调用方的测试必须**不改**就过 |
| ④ 新建 | 只有 ①–③ 都不成立才写新组件 | 同一次改动里写清「为什么现有的扩不了」（`implementations/`），并把新组件补进本文件 |
| ⑤ 回归 | 旧调用方能编译、能过测试；新调用方与旧调用方**观感一致** | 同一件事两种手感 = bug（不是风格问题） |

**禁止**：
* 复制一个组件改名用（`FooRow2.tsx` 之类）；
* 「这个场景特殊」——先问「特殊在哪一条**属性**」，通常加一个 `variant` / `size` / render-prop 就够；
* 在 `components/ui/` 里 import `features/` 或 `api/`（`lint:arch` 会拦）：外部事实由调用方传进来。

## 1. 分层（写代码前先决定「它属于哪一层」）

| 层 | 目录 | 判断标准 | 例子 |
| --- | --- | --- | --- |
| 原语 | `src/components/ui/` | 无业务含义，任何工作区都能用 | `Button` / `Slider` / `MenuItem` |
| 范式件 | `src/components/ui/` | 表达一条**全项目统一的规矩** | `RemoveButton`（禁行图标）、`EasyCopy`、`EasyDestroy` |
| 共享件 | `src/components/ui/<组>/` | 跨工作区的组合，但仍不认识 catalog / IPC | `tiles/` `viewer/` |
| 工作区专用 | `src/features/<模块>/` | 只服务一个工作区 | `editor/area-panel.tsx` |
| 纯逻辑 | `src/lib/` | 有单测、无 DOM | `stack-resize.ts` `row-drag-sort.ts` |

## 2. 原语（`src/components/ui/`）

| 组件 | 文件 | 管什么 | 别再写 |
| --- | --- | --- | --- |
| `Button` / `IconButton` | `Button.tsx` | 按钮族（`primary/accent/secondary/ghost` × `sm/md`）、纯图标按钮、带角标 | 原生 `<button class="...">` 手搓 |
| `Checkbox` / `RadioCircle` / `Switch` / `Input` | `Form.tsx` | 勾选（方）、勾选（圆）、开关、文本框 | 原生 `<input type="checkbox">`（不吃主题令牌、行为不一致） |
| `SegmentedControl` | `SegmentedControl.tsx` | 凹槽 + 移动色块的分段控件 | 一排按钮假装分段 |
| `CompactChoice` | `CompactChoice.tsx` | 紧凑单选 | — |
| `Slider` | `Slider.tsx` | 连续 / 定档滑块（键盘与无障碍由 Ark 承载） | 原生 `<input type="range">` |
| `Tooltip` / `ButtonTip` | `Tooltip.tsx` | 提示气泡；按钮自带 `tip` 优先用它，**不要外套 Tooltip** | 第二套 tooltip machine |
| `Menu` | `Menu.tsx` | 下拉菜单 | — |
| `SuggestInput` | `SuggestInput.tsx` | **带建议列表的行内输入框**（focus 出列表、前缀筛选、↑↓、Enter 采用、Esc 先关列表）；候选合并/筛选在 `lib/suggest.ts` | 第二套「输入 + 下拉」（不要用 Ark Combobox 再做一份：这里的第一公民是**自由文本**） |
| `Dialog` / `ConfirmDialog` / `DialogFrame` | `Dialog.tsx` | 模态弹窗（**取消在左、确认在右**）；`DialogFrame` 是唯一 Ark 三件套实现 | 自己拼 Backdrop/Positioner |
| `ToggleBlock` | `ToggleBlock.tsx` | 可按下按钮（模式开关） | — |
| `Badge` / `CountBadge` | `Badge.tsx` | 计数 / 状态标签 | — |
| `PathText` | `PathText.tsx` | `shortpath` 路径缩写 | 自己截路径字符串 |
| `TreeNode` | `TreeNode.tsx` | 树形行（展开箭头 + 图标 + 名称 + 勾选） | 手搓树行 |
| `Panel` | `Panel.tsx` | 面板容器（标题行、可折叠） | — |
| `SplitHandle` / `SplitStack` | `SplitHandle.tsx` / `SplitStack.tsx` | **全项目唯一**的拖拽分隔条与 N 段可拖布局（数学在 `lib/stack-resize.ts`） | 第二套 splitter |
| `ScrollBox` / `SubtleScrollbar` | `ScrollBar.tsx` / `SubtleScrollbar.tsx` | 滚动区 / 2px 无感横条 | — |
| `VirtualGrid` | `VirtualGrid.tsx` | 行式虚拟滚动 | — |
| `StateWatermark` | `StateWatermark.tsx` | 载入 / 空 / 错误 / 提示水印 | 各写一份水印 |
| `Toast` / `ToastHost` | `Toast.tsx` + `toast.ts` | 轻提示 | — |
| `MetadataRows` | `MetadataRows.tsx` | 「名称 : 值」两列信息行 | — |
| `SectionTitle` | `SectionTitle.tsx` | **段标题**：左侧 3px 主色竖条 + 加粗 `<h3>`（竖条 `aria-hidden`，语义由标题文本承担） | 再手写一行「色值 + 字重」假装标题（2026-11-05 导出右栏先落地，其余面板跟进时用它） |
| `ToolsSeparator` | `ToolsSeparator.tsx` | toolsbar 组分隔符 | — |
| `Histogram` / `HistogramPanel` | `Histogram.tsx` / `HistogramPanel.tsx` | RGB 直方图 / 直方图块 | — |
| `Progress` | `Progress.tsx` | **全项目唯一**进度条（确定态 / `value=null` 不确定态） | 第二根进度条 |
| `DisplayColorStatus` | `DisplayColorStatus.tsx` | 显示色彩状态 | — |
| `Tile` | `Tile.tsx` | 网格基本单元 | — |
| `PhotoMarks` | `PhotoMarks.tsx` | 一张照片的标记簇（只读） | — |
| `RowDragSort` | `RowDragSort.tsx` | **全项目唯一**整行拖动排序（手势 + 让位 + 占位洞 + 跟随光标的整行幽灵），几何在 `lib/row-drag-sort.ts` | 第二套「拖着排序」（2026-11-01 之前 LUT 与 area 各写了一份，已合并） |
| `ShimmerText` | `ShimmerText.tsx` | **一遍流光**：同一份内容画两遍，上层用移动的 `mask-image` 裁成光带（`.rb-shimmer-mask`）；`once` = 一次性强调，默认 = 循环状态指示，`dimBase` = 「处理中」那种底层压暗 | 自己拼「两遍文字 + mask」（CSS 原语只有 `styles/motion.css` 一份） |
| `readTokenPx` | `tokens.ts` | 把 CSS 令牌读成数字（虚拟化行高） | — |

## 3. 范式件（一条规矩 = 一个组件）

| 组件 | 文件 | 规矩 |
| --- | --- | --- |
| `EasyCopy` | `EasyCopy.tsx` | 指向即知可复制 → 点击变「已复制」 |
| `EasyDestroyButton` / `EasyDestroyHost` | `EasyDestroy.tsx` | 移除类操作的快速通道：默认弹确认、`Shift` 跳过（判定在 `lib/easy-destroy.ts`） |
| `RemoveButton` / `RemoveGlyph` | `RemoveButton.tsx` | **移除 / 排除 / 删除 / 还原 一律禁行图标**（`AGENTS.md` §11.3），命中区 ≥ `--remove-hit`，自带 `stopPropagation` |
| `DragPreview` | `DragPreview.tsx` | 拖动预览（整叠卡片 + 数量角标）——全项目唯一 |
| `PreviewFrame` | `PreviewFrame.tsx` | 全图总览（比例夹取 + 视野框） |
| `ProfileSelect` | `ProfileSelect.tsx` | 预设 / 配置文件选择 |
| `RepositoryCard` | `RepositoryCard.tsx` | 库卡片 |

### 3.1 图标纪律（同一个动作只允许一枚图标）

| 动作 | 唯一图标 | 实现 | 现有调用点 |
| --- | --- | --- | --- |
| 移除 / 排除 / 删除 / 还原 | **禁行**（圆 + 镂空横杠） | `<RemoveButton>` / `<RemoveGlyph>` | 导入「最近」、编辑区层、定稿行、LUT 链层、LUT 分类项 |
| 真删（回收站粉碎） | 同上 | 同上 | 回收站拖动（无按钮） |

❗**禁止**再用 `IconBan`（手写）或 `IconTrash` 表达这四个动作：
2026-11-01 之前导入「最近」/ area / 定稿 / LUT 四处出现过三种图标，崔总当场指出「全部统一掉」。
「删除」与「移除」在用户眼里是同一件事（都是从集合里拿掉），不必靠图标区分语义。

### 3.2 开发期工具（不进生产包）

| 件 | 位置 | 口径 |
| --- | --- | --- |
| 调试面板**入口与框架** | `shell/TitleBar.tsx`（主题/密度开关左边，`import.meta.env.DEV` 守卫 + `lazy`） | 入口与框架**固定**，弹窗内容**每轮按需要换**；生产构建整条分支被摇掉 |
| 本轮内容：控制点容差映射 | `dev/MaskDebugDialog.tsx` + `lib/mask-debug.ts` | 八个权重数字（亮度/色相 × 完全容差/过渡终点 × 容差 100/1000）+ 白线长度端点；改一下立刻重出帧，`localStorage` 记住；「恢复默认」问 Rust 要（不抄数字） |
| 生效链路 | `developPayload().maskDebug` → `editor_set_params` → `DevelopStages.weight` | 只影响**编辑器真帧**；缩略图 / 导出 / 定稿走固化常量 |

**要调什么就加什么**：这类参数不需要走命令注册、不进语言包的「用户可见」判定
（`i18n` 仍然是必须的，切语言时它们也跟着翻）。

### 3.3 功能提示区（flowbar）

| 件 | 位置 | 口径 |
| --- | --- | --- |
| 提示状态 | `src/lib/flow-hint.ts` | `showFlowHint(owner, text)` / `clearFlowHint(owner)`；**只存成品字符串**（`lib` 不许 import `i18n`，调用方 `t()` 后传进来）；后指到的覆盖前一条，离开**只清自己那条** |
| 提示槽位 | `shell/FlowBar.tsx`（设置按钮右边） | 居中、吃满到 info 区前、**留 100px**、超出右缘渐隐（`.fade-x-end`） |
| 现在的内容 | `features/editor/area-markers.tsx` | 指到控制点圆片 → 「按住 Ctrl 键可拖动」 |

**什么时候用它**：某操作**只有做过才知道**（按住修饰键、拖某个边、双击某处），
而且这句话**没有常驻价值**（不是「不带变量的固定说明文字」，`AGENTS.md` §2.20）。
指向即出现、离开即消失 —— 不要拿它做状态播报（那是 toast / 状态栏的事）。

### 3.4 错误与提示的**唯一通道**

| 场景 | 走哪 |
| --- | --- |
| 操作失败 / 落库失败 / 取样失败 / 读库失败 | **`props.toast.show({ tone: "danger" })`**（`shell` 建、往下传；`AGENTS.md` §2.29） |
| 成功 / 已复制 / 已移除 | toast（`tone: "success"`） |
| 表单校验（对话框内、紧贴字段） | 允许留在弹窗内联（**只有这一种例外**：错误属于那个字段，弹出到背后反而看不见） |

❌ 不要再往工作区 / 面板里塞 `<p class="text-danger">` 那种随手写的错误行
（2026-11-01 崔总：影调面板底下那行红字就是这么来的）。
断言式检查：`grep -rn 'text-danger' src` 命中的地方，要么是图标 / 危险按钮配色，要么是对话框内联校验。

## 4. 成套共享件

| 组件 | 位置 | 管什么 |
| --- | --- | --- |
| `TilesShell` / `TilesControlBar` / `PhotoStatusBar` | `components/ui/tiles/` | tiles 一族外框与控制条（`TilesControlBar` 只能由 `TilesShell` 装配） |
| `Viewer` / `ViewerControls` | `components/ui/viewer/` | 单张看图（缩放/平移/适配/100%）；**取图由调用方注入** |
| `FilmStrip` / `CompareView` / `photosFromSource` | `components/ui/viewer/` | 胶片带与对比画幅；`photosFromSource` 是 tiles → 看图件的唯一转换 |
| `PhotoGrid` / `PhotoViewingStage` / `viewing.ts` | `features/photo-grid/` | 照片区控制器（import 与 browse 必须同一份） |
| `DirTree` | `features/dir-tree/` | 目录树 |
| `ExifStrip` + `createSelectedFileMetadata` | `features/exif-strip/` | flowbar 的 `flowinfo` |
| `CommandPalette` / `ShortcutSettingsDialog` | `features/commands/` | 命令面板与快捷键设置（注册表 `catalog.ts`） |

## 5. 编辑区（area / 控制点）专用件

| 件 | 位置 | 口径 |
| --- | --- | --- |
| `AreaPanel` | `features/editor/area-panel.tsx` | 层列表：行高 30、类型图标（默认=滑杆 / 全图=方块 / 控制点=色点）、双击改名、启停、删除、整行拖动（`RowDragSort`） |
| `AreaMarkers` | `features/editor/area-markers.tsx` | 画布标记：三环同心圆片、**水平**引线（白线 + 细黑勾边）、线上名字（白字 + 勾边）、容差 handle；尺寸屏幕像素固定；手势用**原生监听**（`on:pointerdown`）挡住视口 |
| 覆盖层矩阵 | `features/editor/ViewportOverlay.tsx` + `lib/overlay-math.ts` | 子元素写**图像像素**，矩阵由 Rust 给；`lib/overlay-live.ts` 负责手势期间**每帧**读一次（标记实时跟随） |
| 控制点八色 | `src/styles/tokens.css` `--point-0..7` | 层里只存槽位下标 |
| `AreaGlyph` | `features/editor/area-glyph.tsx` | 层类型记号（默认=滑杆 / 全图=方块 / 控制点=圆点）**唯一一份**：层列表行、拖动幽灵、右栏层名条都用它；层名条传 `class="text-brand"` 走主色 |

## 6. 新建组件的检查表（提 PR 前自问）

1. 名字是否表达了**能力**而不是**场景**（`RowDragSort` 而不是 `LutDrag`）？
2. 有没有第二处能受益？（有 → 一般化）
3. 视觉是否走令牌（`lint:colors`）？文案是否走 i18n（`lint:i18n`）？
4. 有没有单测（纯逻辑抽到 `src/lib/`）？
5. 加进 `src/dev/KitchenSink.tsx` 了吗？
6. 本文件 §2–§5 与 `AGENTS.md` §12 补上了吗？

## 7. 已合并 / 已禁止的重复实现（别再写回来）

| 曾经的两份 | 现在 | 记录 |
| --- | --- | --- |
| LUT 链整行拖动 与 area 层整行拖动 | `components/ui/RowDragSort.tsx` + `lib/row-drag-sort.ts` | 2026-11-01 合并（手势/让位/幽灵/边缘滚同一份） |
| `IconBan`（area）、`IconTrash`（定稿）、`RemoveButton`（LUT / 最近） | `RemoveButton` 一处 | 2026-11-01 统一 |
| 导入 / LUT 两根进度条 | `Progress` | 2026-10-30 |
| 两个 tooltip machine（外层 `Tooltip` 套按钮） | 按钮自己的 `tip` | 2026-10-11 |
| 编辑区 / 浏览各一套总览图 | `PreviewFrame` | 2026-10-31 |
