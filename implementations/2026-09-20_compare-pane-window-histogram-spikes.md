# 对比：栏区是窗口 + 直方图尖刺 + 双击缩放

完成时间：2026-09-20 01:38:12 CST

## 范围

人类 2026-09-20 报的三件事（原文已进 `docs/user-requirements.md`）：

1. **对比里图片被限制在自己的画幅宽高内**（放大后也铺不满那一格）——
   正确口径是「2 图 / 3 图 / 4 图三种分栏，每格分到的栏区都像一个窗口：
   图片比例不变、不拉伸，但在窗口内可以任意缩放，放大后能在整格里看」；
2. **双击缩放失效**；
3. **直方图仍有「像尖刺一样的边缘异常」**。

顺带把两条只有一份的东西收口（`AGENTS.md` §2.12）：滚轮缩放合并逻辑、
以及对比那套只服务于「已铺满画框的图片」的夹取 API。

## 涉及文件

- 对比几何：`src/lib/viewer-compare.ts`、`src/lib/viewer-compare.test.ts`
- 看图件 store：`src/components/ui/viewer/store.ts`、`store.test.ts`
- 交互共享件：`src/components/ui/viewer/interaction.ts`、`interaction.test.ts`
- 对比视图：`src/components/ui/viewer/CompareView.tsx`
- 单张看图：`src/components/ui/viewer/Viewer.tsx`
- 右栏视野框：`src/features/browse/ViewerReadout.tsx`、`BrowsePanels.tsx`、
  `src/workspaces/browse/BrowseWorkspace.tsx`
- 直方图：`src/lib/histogram.ts`、`src/lib/histogram.test.ts`
- 冒烟：`scripts/check-browse-boot.mjs`
- 规范：`BROWSE.md` §5.7、`DESIGN.md` §13.1/§13.3、`plans/M2-W2.md` 2.3
- 需求原文：`docs/user-requirements.md`

## 关键决策与理由

### 1. 两层盒子：栏区是窗口，画框是内容

上一版把**画框**当成了裁剪边界（外层盒子缩到图片比例、`overflow` 挂在它上面），
所以无论怎么放大，图片都被关在自己那个小盒子里 —— 正是人类报的症状。
现在的口径写进了 `lib/viewer-compare.ts` 的文件头：

```text
┌─ 栏区（窗口）────────────────┐  ← 可见/裁剪边界（overflow: hidden），有底色与描边
│      ┌─ 画框（等比例，居中）┐  │  ← 基准比例的盒子；放大后可以大于窗口
│      │      图片内容        │  │
│      └────────────────────┘  │
└──────────────────────────────┘
```

* `compareGeometry(photos, pane)` 一次给出**画框 + 每帧的扣取区与图片摆放**
  （`fitAspectWithin` + `cropToAspect`，都是老函数）：扣取区正好铺满画框，
  所以「所有画幅对齐到同一个画框」这件事由**构造**保证，不靠视图里各写一遍百分比。
* 画框是 `contain` 出来的 ⇒ 图片**永远等比例**（窗口横竖变化只改大小不改比例）。
* 放大 = 在画框上叠 `translate + scale`（`transform-origin: center`），
  超出窗口的部分由**窗口**裁掉。

### 2. 倍率单位：相对画框的倍数，住在 CompareView

对比里各图像素尺寸本来就不同（4000×3000 与 6000×4000），「同一个倍率」只能是
**相对画框**的倍数。这个单位不属于单张看图的 store（那是「原图像素 × zoom」），
所以 `rel` / `pan` 归 `CompareView`：

* 对比不污染单张看图进入前的倍率与位置；单张看图也不必理解画框；
* 数学仍只有一份 —— `compareGeometry` 与 store 的纯函数 `clampPan` / `zoomPanAt`
  （`MIN_ZOOM` / `MAX_ZOOM` 也照旧用同一对界限换算）。

### 3. 夹取规则收成一条 `clampPan`

`clampPan` 从「收原图尺寸 + 倍率」改成「收**已经乘好的内容盒尺寸**」，
单张看图传 `natural × zoom`、对比传 `画框 × rel`，于是**两份实现并回一份**；
`clampFramePan`、`resetFit`、`zoomWithinFrame`、`panWithinFrame` 全部删除
（对比不再借用单张看图那套「适配倍率」的状态）。`panPercent` / `percentToPan` 也删了：
位移按百分比同步现在是**构造性质**（同一个画框），不需要再换算。

### 4. 双击与滚轮

* 双击 = **适配 ↔ 100%**，与单张看图同一条口径；「适配」不再是一个会失同步的
  `fit` 标志，而是 `rel === 1` 本身。100% 换算到**基准那幅**（`oneToOneRel`），
  读数也按它显示（所以双击后读数就是 `100%`）。
* 滚轮：把「按帧合并 + 指数映射」抽成 `interaction.ts::createWheelZoom`，
  **单张看图与对比共用一份**（两处手感必须一致，也不能各写一套累计逻辑）；
  对比里锚点是「光标落在哪一格的本地坐标」——各格一样大、内容按同一画框对齐，
  所以在哪一格上缩放锚到的都是同一个内容点。
* 对比态下单张看图件**没挂载**，所以 `+` / `-` / `0` / `1` 在 `CompareView` 里接；
  双击落在缩放/返回按钮上时不当作切换适配（那两个按钮自己有点击行为）。

### 5. 右栏在对比态不画视野框

`visibleRect()` 描述的是**单张看图那一个窗口**里看得见哪一块；对比是好几格各自
独立的窗口，一个框表达不了。`ViewerReadout` 加 `showVisibleBox`，对比时传 `false`。

### 6. 直方图尖刺的真因：色带只在「自己当冠军」的列上有值

`histogramBands` 旧写法是「每列排序后把 low/mid/high 分给对应层」——
只有本列冠军才有值，其余层是 **0**。相邻两列的冠军一换，那条折线就从曲线上
**直直落回基线**，画出来就是一根细尖刺（人类截图里那些竖线，全部出现在
通道高度接近的列附近，正是冠军换人的地方）。

新写法：**每层用自己的边界曲线表达、处处有定义**（厚度可为 0）：

| 层 | 区间 |
| --- | --- |
| 三色重叠（灰） | `[0, min(r,g,b)]` |
| 单通道 r | `[min(r, max(g,b)), r]`（自己不是最高时厚度 0，贴着 `max(g,b)`） |
| 黄（r+g） | `[min(b, min(r,g)), min(r,g)]` |
| 青（g+b） | `[min(r, min(g,b)), min(g,b)]` |
| 紫（r+b） | `[min(g, min(r,b)), min(r,b)]` |

冠军切换时只是厚度连续变成 0，不再编造数据里没有的区域；并列时厚度自然为 0，
连「平局规则」都不需要了。

## 验证（Agent 侧，冒烟）

- `pnpm test`：704 passed / 0 failed（新增/改写：`compareGeometry` 的窗格关系、
  `oneToOneRel`、零尺寸与未知尺寸边界、`clampPan` 的两种调用方、
  `createWheelZoom` 的「一帧一次 / 可重复 / 卸载取消」、`histogramBands` 的
  「冠军换人不下基线」与「各层厚度之和 = 最高通道」）
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过
- `pnpm build`：通过
- `pnpm check:browse`：通过。新增断言（真浏览器、假后端）：
  栏区 `overflow: hidden`、适配时画框落在栏区内、**双击后画框溢出栏区且读数 100%**、
  再双击回到适配、**真鼠标事件拖动 60/−40 px 后画面位移跟手（±1.5px）且倍率不变**
- `pnpm smoke:ui`：通过，`problems: []`

## 未经 Agent 验证（需人类真机/目视）

- Windows 上对比的视觉与手感：2/3/4 三种分栏是否都「整格可见、放大铺满」、
  拖动是否跟手、双击是否顺、滚轮锚点是否符合直觉；
- 直方图在真实照片上是否还有尖刺（截图里那类异常只能靠眼睛判定）；
- 单张看图的滚轮/双击是否与之前手感一致（实现被抽成共享件，需要复验一次）。

## 遗留问题

- **弹窗层级**（人类 2026-09-20 新报，已记进 `docs/user-requirements.md` 与 todo）：
  模态浮层应是全局最高（仅次于 `titlebar` 与右上角 toast），现在 `view` / `film`
  覆盖层在它之上 —— film/view 态下点库齿轮，弹窗被挡、只有周围遮罩变暗。
- **切语言时横向选择器选中底色**（同批新报，已记进需求原文与 todo）：
  切中英文那一瞬间，`titlebar` 的紧凑/宽松与 flow 选择器的选中段底色会变（疑似过渡色
  或选中判定依赖被重建的 DOM），操作一次即恢复。
- 对比的 `rel` / `pan` 目前住在视图里，**没有单测**（纯函数部分已覆盖）；
  若以后要把它挪进 store 或 Rust 原生视口，需要一并搬走。
