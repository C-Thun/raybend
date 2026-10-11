# toolsbar 紧缩 · 统一悬停提示 · 导出筛选与画廊小改

> 工作单元：崔总 2026-10-11 口述（无里程碑编号，属导出工作流与全局工具条的体验修整）
> 状态：**spec 待崔总审** → 审后 Pencil 画稿（崔总二审）→ 实施
> 需求原文：`todos/2026-10-11-toolsbar-compact-export-filter-raw-tile.md`
> 关联：`memory/DESIGN.md` §12.12（工具条分组/分隔/文字按钮带图标）、`design/export.md` §2.1 与 §4.2、
> `design/browse.md` §2.1、`design/editor.md` §2.1、`specs/export-issue-ordinal.md` §4（**本轮修订**）

---

## 0. 背景与目标

导出工作流需要**按照片自身属性筛出要导的图**（评级 / 色标 / 喜欢）。browse 已有整套筛选能力，
直接搬过来的问题是 export toolsbar 全是文字按钮、再挂四组筛选控件会长得没法看。
所以本轮按崔总的五步走：**先精简四个 flow 的工具条文字 → 把悬停提示收进按钮组件 →
再在 export 上接筛选组 → 顺带做 export 上部网格与 tile 的两处调整**。

目标（可验收）：

1. 四个 flow 的 toolsbar 都是「图标为主、文字只在图标说不清或要表达状态时出现」，且不牺牲可发现性；
2. 纯图标按钮的悬停提示**只有一套实现、一种观感**（不再混用原生 `title=`）；
3. export toolsbar 出现筛选组（与队列动作竖线隔开），筛选在画廊生效、条件在 chips 里可见可删；
4. export 上部在常见中列宽度下**能出现四列**；
5. export 每张 tile = 主图 + ≤2 张 issue + 一个入口按钮（`全部定稿` / 只多一张 RAW 时叫 `RAW`）；
   **RAW 只住在「全部定稿」弹窗里**（排末尾、位置固定，不占小片位）。

---

## 1. 五条改动

### 1.1 toolsbar 文字精简（全 flow 审计）

判据（沿用并补充 `memory/DESIGN.md` §12.12）：**图标能独立表意 → 去文字**；文字承载**状态值**
（锁档位、展示范围、编辑源）或该动作**容易误认**（排除族图标本就共用一枚禁行符）→ 保留文字。
去文字后**必须**有 `label`（无障碍名）与统一悬停提示（§1.2）。

#### 分隔线的口径（崔总 2026-10-11 重定，**取代**「一个独立控件也算一组」那条）

**只在功能大类之间画分隔线；能少用就少用。** 图标本身的形态、配色与间距已经足够区分相邻控件；
分组靠**物理位置与语义**，不靠满排竖线。当前实现里的多余竖线要一并收掉。

| flow | 大类划分 | 分隔线 |
| --- | --- | --- |
| browse | ① 删除 ② 撤销 / 重做 ③ 其余全部（筛选开关 + 旗标 + 星级 + 色标 + 赞踩 + 标签 + 相片桶 + 锁） | **2 根** |
| export | ① 队列动作（送入 / 移出 / 取消选中 / 重置队列） ② 筛选组（旗标 / 标签 / 星级 / 色标 / 赞踩） | **1 根** |
| editor | ① 编辑源（SOOC/RAW） ② 撤销 / 重做 ③ 其余（自动调整 + 裁切 / 旋转 / 对比） | **1 根**（只保留撤销 / 重做**右侧**那根；SOOC/RAW 自身有边框感，右侧改为**留白**，边框 + 竖线太重复） |
| import | 单按钮，无 | 0 根 |
| browse（回收站视图） | ① 还原选中 ② 清空（不可逆） | 保留 1 根（可逆 vs 不可逆本就是大类；若认为不必，说一声即可去掉） |

| flow | 控件（i18n key） | 现状 | 本轮建议 | 理由 |
| --- | --- | --- | --- | --- |
| import | `tools.batch_exclude` 批量排除 | 图标 + 文字 | **保留文字** | 单按钮不挤；禁行图标在本项目里被移除/排除/删除四义共用（`AGENTS.md` §11.3），光看图不看字会认错 |
| browse | 删除 `browse.delete` | 图标 + 文字 | **精简**（IconTrash + 提示） | 垃圾桶是通用符号；误点有确认弹窗兜底 |
| browse | 撤销/重做 `browse.undo` / `browse.redo` | 图标 + 文字 | **精简** | 箭头通用；提示里带动作名（「撤销：标 3 星」） |
| browse | 筛选开关 `browse.filter`（漏斗） | 图标 + 文字 | **精简** | 漏斗是筛选通用符号；开关状态由主色底 + 下方 chips 表达 |
| browse | 旗标 `browse.flagToggle` | 图标 + 文字 | **精简**（实心旗） | 旗子是一等符号（menu 里也用它） |
| browse | 清空旗标 `browse.flagClear` | 图标 + 文字 | **精简**（斜旗 + 提示） ⚠ | 与旗标相邻，两颗同类图标易混；提示文案必须写全「清空旗标」 |
| browse | 星标 ×5 | 已纯图标 | 补提示 | 提示＝`grid.rating`（标记态）/`browse.filterRatingAtLeast`（筛选态） |
| browse | 色标 ×6 | 已纯图标 | 补提示 | 提示＝颜色名 / `browse.likeNone` 那套既有文案（`labels.ts`） |
| browse | 赞 / 踩 | 已纯图标 | 补提示 | `browse.like` / `browse.dislike` |
| browse | 标签 `browse.tag` | 图标 + 文字 | **精简** | 标签图标是通用符号 |
| browse | AI 识别 `ai.recognize` | 已纯图标 | 补提示 | — |
| browse | 相片桶菜单 `org.bucketActions` | 已纯图标 | 补提示 | — |
| browse | 锁 `browse.lock` / `lock1` / `lock2` | 图标 + 文字 | **保留文字** | 文字＝当前档位（锁/一级锁/二级锁），是状态不是标签 |
| browse left | 库目录 / 相片桶 / 标签 `org.library` 等 | 已纯图标 | 补提示 | 三入口互斥开关，现有 `label` 直接转提示 |
| browse right | 导出至外部编辑器 `external.title` | 图标 + 文字 | **精简** ⚠ | 右侧段挤压时会盖住 mid，缩短收益直接；`external-link` 有「用别的软件打开」的通用语义 |
| browse（回收站视图） | 还原选中 / 清空回收站 | 图标 + 文字 | **保留文字** | 低频破坏性语境，文字承担安全提示；该视图按钮少 |
| browse（相片桶照片视图） | 从相片桶移出 `org.removeSelected` | 文字 | **保留文字** | 移除类动作，与删除/排除近义，文字说清对象 |
| edit | 编辑源 SOOC/RAW | SegmentedControl | **保留文字** | 值是 `SOOC`/`RAW`，不是标签 |
| edit | 撤销/重做 | 图标 + 文字 | **精简** | 同 browse |
| edit | 自动调整 `editor.autoAdjust` | 图标 + 文字 | **精简**（魔棒） ⚠ | 魔棒＝自动增强的通用隐喻；若崔总认为太含蓄可回退 |
| edit | 裁切/旋转/对比 | 图标 + 文字 | **精简** | 行业标准图标（crop / rotate / compare-h） |
| edit left | LUT `editor.toolsbar.lut` | 图标 + 文字 | **精简**（调色板） | 面板开关，开关状态由主色底表达 |
| edit right | 重置修改 `editor.panel.resetAll` | 图标 + 文字 | **精简**（refresh） ⚠ | 与「定稿」并排；定稿保留文字（主按钮 + 产品术语），重置用图标压宽度 |
| edit right | 定稿 `editor.issue.toolbar` | 图标 + 主色实底 + 文字 | **保留文字** | 产品术语（issue/定稿）不能只靠书签图标 |
| export | 送入队列 `export.enqueue` | 图标 + 文字 | **精简** ⚠ | 主操作；有 `Enter` 热键与队列计数动画兜底 |
| export | 移出队列 `export.remove` | 图标 + 文字 | **精简** | playlist-x 与送入成对，语义靠位置即可辨 |
| export | 取消选中 `export.clear` | 图标 + 文字 | **精简** | square-off 是「清空选择」的通用符号 |
| export | 重置所有队列 `export.reset` | 图标 + 文字 | **精简** ⚠ | 破坏性动作但有确认弹窗；橡皮图标 + 提示 |
| export left | 展示范围 `export.scope.*` | 图标 + 文字 | **保留文字** | 文字＝当前档位（全部/仅定稿/编辑过） |
| export right | 全部停止 `export.stopAll` | 图标 + 文字 | **精简** ⚠ | 停止符号通用；只有导出中才可点，状态靠可用性表达 |

> ⚠ = 可回退项，列入 §2 待崔总一次拍板（Q1）。回退只需要把该行改回「保留文字」，不影响其它部分。
> 精简后**分组与分隔不变**（`memory/DESIGN.md` §12.12）；`Button` 纯图标档保持既有 `--btn-h-*` 与
> `--btn-pad-x`（命中区与节奏不缩水），不做「方形小按钮」特例。

### 1.2 图标按钮的悬停提示统一进按钮组件

**现状**：`TitleBar`（主题开关）走 `Tooltip` + `IconButton`；`lut-panel` / `preset-panel` 走 `Tooltip`；
browse / editor 工具条用**原生 `title=`**（样式不可控、延迟约 1s、与全系统提示两套观感）。
本轮把能力收进按钮组件，工具条不再自己写提示。

**API（`components/ui/`，实现只有 `Tooltip.tsx` 一份）**：

| 组件 | 新增/变化 | 提示内容 |
| --- | --- | --- |
| `Button` | 新增 `tip?: string`、`tipPlacement?` | `tip` 给了就显示；没给且**没有可见文字子节点**时，退回 `aria-label` |
| `IconButton` | `label`（已有）直接转成提示，不再写 `title=` | `label` |
| `ToggleBlock` | 新增 `tip?: string`（已有 `label`） | `tip` 优先；没给且无文字子节点时用 `label` |

**统一口径**：

1. **位置**：三个组件内置默认 `placement="bottom"`（工具条贴在窗口顶部，气泡朝下有空间）；
   Zag 定位器自带 `flip` + `shift`，靠近窗口下缘会自动翻上去。`Tooltip` 本体默认值（`top`）不动，
   面板里的既有用法不受影响。
2. **禁用态也要能弹**：禁用按钮不派发鼠标事件，提示挂在按钮上会失灵 —— 组件内部在需要时把触发器
   包一层 `inline-flex` 的包装元素（`pointer-events` 留在包装上），这是「为什么不能点」最需要说话的场景。
3. **延迟**：沿用 `Tooltip` 默认（150ms 开 / 80ms 关），提示类是短文案，要快。
4. **移除**工具条上的原生 `title=`（`BrowseToolbar` 的删除/撤销/重做、`editor/toolbar.tsx` 的三处）；
   **非按钮元素**上的 `title`（tile 文件名、错误原因、路径缩写等）保持不变。
5. i18n：提示文案一律走既有语言包 key（不新增硬编码）；筛选用与新键见 §1.5。

### 1.3 export toolsbar 接筛选组

**状态归属（决定）**：筛选条件与筛选开关**继续只住在 `browseStore`**，export 只是第二个入口
（`AGENTS.md` §2.12：同一个能力只允许一套实现）。理由：

- export 画廊的数据源就是 `browseSource(browseStore)`，真正的筛选发生在查询层（服务端 filter），
  另立一份 export 自己的筛选条件=再加一条数据路径，还会与分页/虚拟化打架；
- 「筛选是这批照片的视图状态」跨 flow 共享是自然的：在 export 筛出「≥3 星」再回 browse，
  看到的仍是同一批图，chips 也解释得清；
- 代价与规则一起写明（见下）。

**export 的组**（崔总 2026-10-11 定：**export 里没有筛选开关** —— 只有筛选模式、没有设置模式，
这批按钮**恒为筛选语义**；与队列动作之间**只画一根**竖线）：

```text
[送入队列][移出队列][取消选中][重置所有队列] │ [旗标][标签][★★★★★][●×6][赞][踩]
└─────────── 队列动作（纯图标）───────────┘   └────── 筛选组（恒筛选、仅筛不标）──────┘
```

- **旗标筛选**：与 browse 筛选态同一套（「有旗标」/「无旗标」两个开关，再点取消；旗标在内存里，
  id 列表由 store 填）。
- **标签筛选**（本轮新增）：一个**标签菜单**（沿用 `Menu` 原语）列出标签词典，点一条＝加上 / 去掉一条
  标签条件（组内或，与 `filter.tags` 的既有语义一致）。**只筛不设置**：不像 browse 的标签按钮那样
  打开 `TagDialog`，**也没有** AI 识别标签的入口。后端早支持（`Filter.tags`），chips 补 `tag` 分支。
- **星级 / 色标 / 赞踩**：与 browse 筛选态一致（阈值 / 组内或 / 互斥）。
- **无漏斗、不改模式**：export 不需要在「设置标识」与「筛选标识」之间切换。实现不另写一套 ——
  复用 `features/browse/mark-actions.ts::applyMarkIntent`（唯一实现），export 入口传 `forceFilter`
  （星 / 色 / 赞踩 / 旗标 / 标签的条件语义全部与 browse 筛选态一致）；条件写入后 `filterMode`
  同步置真，**只为让 browse 与 chips 行保持同一份可解释的状态**。
- **chips 行**：在 export 中列**上部画廊之内**渲染同一个 `FilterBar`（`data-filter-bar` 锚不变），
  位置在 top 网格之上、纵向分割条之上 —— 只作用于画廊，不暗示对下方队列生效。
  筛选开启或条件非空时才出现；每条 chip 可单独摘掉，「共 N 张」显示筛选后的数量。
- **锁 / 旗标不在 export 出现**：browse 设下的这两类条件照常作用于同一个列表，export 里通过 chips
  摘掉即可（chips 是通用删除入口，不需要各自的设置控件）。
- **命令体系（§2.15）**：`view.filter.toggle`、`view.filter.clear`、`mark.rating.*`、`mark.color.*`、
  `mark.like`、`mark.dislike` 扩到 export 场景（`scenes` 加 `"export"`、`when` 加 export 判定），
  默认键不变（已有键位，无新增冲突）。顺带收敛一处既有不一致：标记命令的 `enabled` 与工具条口径对齐
  （**筛选态不需要先选照片**，工具条早就如此，命令面板此前仍要求 `hasSelection`）。
- **无 XMP 影响**：筛选是纯视图状态，不落库、不写 sidecar（`AGENTS.md` §2.19 评估结论：不涉及）。

### 1.4 export 上部 tile 最小边长 240 → 192（−20%）

- `src/lib/export-prefs.ts`：`EXPORT_TOP_SIZE_BOUNDS = { min: 192, max: 320 }`。
  档位由 `tileSizeSteps(bounds)` 生成：`192, 202, 214, 226, 240, 254, 270, 286, 302, 320`（10 档）。
- 四列算式（`--tile-gap: 3px`）：中列 920 时 `4×226 + 3×3 = 913 ≤ 920` —— 四列可达成；
  旧下限 240 时 `4×240 + 9 = 969 > 920`，所以永远差一列（正是崔总说的「刚好会空」）。
- 「适合窗口」、滑杆、滚轮、按钮、命令与恢复偏好都读同一份 bounds，自动跟着变宽，无需另改。
- **偏好迁移（§2.16 的 localStorage 版）**：key `raybend.export-display.v3` → **v4**；
  旧值是**档位下标**，bounds 变了含义就变 ⇒ 必须按**尺寸**回填：
  `新位置 = tilePositionForSize(tileSizeAt(旧位置, 旧 bounds), 新 bounds)`。
  迁移测试写进 `src/lib/export-prefs.test.ts`（旧的 v1/v2 迁移链保持）。

### 1.5 export tile：主图 + ≤2 issue + 一个入口按钮

> **2026-10-11 第二轮修订**（崔总口述，取代本节的初版「RAW 按钮」设计）：RAW 回到
> 「像 SOOC 一样的小片」形态，但**只住在「全部定稿」弹窗里**；tile 上不再有任何 RAW 专用控件。

```text
┌──────────────────┐
│                  │
│     主图 tile     │   ← 与现在一致（mainVariant；选中/队列徽标/尾号不变）
│                  │
├──────────────────┤
│ [issue A][issue B]│  ← 最多 2 张（一行两列，方形小图，**永不含 RAW**）
├──────────────────┤   ← 底部留 4px 呼吸位（不贴下一行的大图）
│ [入口按钮]        │  ← 见下表：`全部定稿` / `RAW` / 不出
└──────────────────┘
```

| 元素 | 显示条件 | 行为 |
| --- | --- | --- |
| 主图 | 恒有（`mainVariant`） | 不变（选择、入队按钮、尾号徽标、拖动……全部照旧） |
| issue 小片 | `orderedIssues()` 去掉 `raw` 后取前 2 | 点击＝选/取消选该 issue（**不变**）；队列徽标、拖动、`data-export-issue` 锚不变 |
| 入口按钮 | 弹窗里比 tile 多出东西才出（见下） | 打开既有的 `全部定稿` 弹窗（`setAllIssues`，内容不变） |

**入口按钮的两种名字**（同一种外观，纯文字样式 + 按钮的指向手形）：

| 名字 | 条件 | 含义 |
| --- | --- | --- |
| `全部定稿` | 还有非 RAW 的定稿没摆下（`hidden > 0`） | 打开弹窗，里面能看到全部定稿 + RAW |
| `RAW` | 没别的可看，**只多一张 RAW**（`hidden === 0 && raw !== null`） | 同一个弹窗，固定只有那张 RAW 原片 |
| 不出按钮 | 弹窗里没有任何比 tile 更多的内容 | tile 不被入口刷屏 |

- **RAW 的呈现**：弹窗里它排**末尾**（`orderedIssues` 的 rank 已保证），**选中与否都不动位置**，
  也**永远不会冒到 tile 的小片区**（这点与 SOOC 不同）—— 它是「原片在那里，你要不要一起导」，
  不适合当快捷动作。选中它＝正常入队（导出即 `IRA` 尾号，`specs/export-issue-ordinal.md` §2 不变）。
- **小片的图不直读 RAW**（崔总 2026-10-11：直读太贵）：借同一张照片的 SOOC 小图 +
  既有「RAW 模拟」滤镜（`Tile.approximate` → `.photo-raw-approximate`）显示成没修过的原片样子；
  同一张照片的两片因此共用一条缓存键，零额外解码。只有**非主行的** RAW 变体才借
  （RAW 单文件的照片主行就是它自己，没有 SOOC 可借）。
- **条高公式**：`src/workspaces/export/source.ts::issueExtraHeight({tiles, entry}, cellSize)` 改为
  `4 + (tiles > 0 ? small : 0) + 24 + 4`（上留白 + 小片行 + 按钮行 4+20 + **下留白 4**），
  测试在 `source.test.ts`。
- **反馈**：入队不弹 toast，靠下方队列计数动画；失败走 `store.reportError`。
- **i18n**：新增 `export.rawEntry`（中英都是 `RAW`，是入口按钮在「只多一张 RAW」时的名字）。
- **命令体系（§2.15）**：入口按钮与 RAW 片**不单列命令**（单张 tile 上的手势级动作；
  键盘路径是「选中 + Enter 送入队列」）。结论写进实施记录。
- **修正旧规格**：`specs/export-issue-ordinal.md` §4 的修订指针指向本节（尾号体系 `IRA` 不变，
  变的是它在画廊的呈现方式）。

---

## 2. 崔总已定案（2026-10-11）

| # | 事项 | 定案 |
| --- | --- | --- |
| Q1 | §1.1 的 ⚠ 精简项 | **全按建议执行**（清空旗标 / 外部编辑器 / 自动调整 / 重置修改 / 送入队列 / 重置所有队列 / 全部停止 → 纯图标；锁 / 展示范围 / SOOC-RAW / 定稿 / 批量排除 / 回收站两键保留文字） |
| Q2 | export 与 browse 共用一份筛选状态 | **是**；但 **export 没有漏斗开关**（恒筛选模式） |
| — | 分隔线 | **只在大类之间**：browse 2 根、export 1 根、editor 1 根（只留撤销/重做右侧；SOOC/RAW 右侧改成留白）；存量多余竖线一并收掉（§1.1） |
| — | export 筛选组内容 | 星级 / 色标 / 赞踩 **＋ 旗标筛选 ＋ 标签筛选**（标签只筛不设置、不识别） |
| Q3 | 「全部定稿」弹窗不含 RAW | **取消**（2026-10-11 第二轮）：弹窗**含** RAW（排末尾、固定不动）；不含的是 tile 小片区 |
| Q4 | 只有 1 片时按钮行 | 由 `entry` 决定（见 §1.5）：只多一张 RAW 时按钮叫 `RAW`，没有更多内容就不出按钮 |

---

## 3. 改动落点（文件级）

| 层 | 文件 | 改什么 |
| --- | --- | --- |
| UI 原语 | `src/components/ui/Button.tsx`、`ToggleBlock.tsx` | 内置 `Tooltip`（`tip` / `label` 回退、禁用态包装、默认 bottom）；删 `title=` 用法 |
| UI 原语 | `src/components/ui/Tooltip.tsx` | 仅在被需要时加 `placement` 透传/说明，本体行为不变 |
| 工具条 | `src/features/browse/BrowseToolbar.tsx` | 去 7 处文字（§1.1）+ 提示接入 + 筛选控件抽成共享组件 |
| 工具条 | `src/features/browse/FilterControls.tsx`（新） | browse / export 共用的筛选控件（星 / 色 / 赞踩 [+ 漏斗]） |
| 工具条 | `src/features/browse/FilterBar.tsx` | 无须改结构（export 直接复用）；如加 `class` 传入即可 |
| 工具条 | `src/features/editor/toolbar.tsx` | 去 5 处文字 + 提示接入 |
| 工具条 | `src/workspaces/export/Toolbar.tsx` | 去 4 处文字 + 提示接入；新增筛选组（漏斗 + 星 + 色 + 赞踩） |
| 组装 | `src/App.tsx` | export 的工具条插槽接线；命令 `scenes`/`when` 扩到 export；`markDisabled` 口径同步 |
| 命令 | `src/features/commands/catalog.ts` | 筛选/标记命令扩场景与 enabled 口径 |
| 状态 | `src/features/browse/mark-actions.ts` | `applyMarkIntent` 支持「强制筛选语义」（export 入口） |
| 画廊 | `src/workspaces/export/ExportWorkspace.tsx` | `IssueStrip` 重做（≤2 + 一个入口按钮）、`FilterBar` 挂载、弹窗含 RAW、RAW 小片借 SOOC 图 |
| 画廊 | `src/workspaces/export/source.ts` | `issueExtraHeight` 新公式 |
| 画像 | `src/lib/export-model.ts` | 新增纯函数 `issueStrip(...)`（小片列表 / 隐藏张数 / RAW / 入口按钮种类），旧 raw 排序逻辑保留给内部排序 |
| 画像 | `src/lib/export-prefs.ts` | bounds 下限 192、prefs v3 → v4 迁移 |
| 导出 store | `src/workspaces/export/store.ts` | 显式 references 入队路径（拖放用）；**不再需要** RAW 专用入队动作（第二轮删除） |
| i18n | `src/i18n/zh-CN.ts`、`en-US.ts` | 新增 tip key，其余复用 |
| 冒烟脚本 | `scripts/check-export-boot.mjs` | 断言改为「≤2 小片 + 无 raw tile + RAW 按钮存在」 |
| 文档 | `memory/DESIGN.md` §12.12 | 补「可精简为纯图标 + 提示归按钮组件」的口径 |
| 文档 | `design/export.md` / `design/browse.md` / `design/editor.md` | 与画稿同步修订对应小节 |
| 文档 | `specs/export-issue-ordinal.md` | §4 加修订指针 |

---

## 4. 测试与验收

#### 单元测试（Agent，秒级）

- `lib/export-prefs.test.ts`：v4 迁移按尺寸回填；非法值回落；旧 v1/v2 链不破。
- `lib/export-model.test.ts`：`issueStrip` 边界（0 / 1 / 2 / 3+ 张、只有 RAW、RAW 即主图、SOOC 位置、
  选中的提到前面不变、raw 不进小片）。
- `workspaces/export/source.test.ts`：`issueExtraHeight` 新公式（0 / 1 / 2 / 3+ / cellSize 边界）。
- `workspaces/export/store.test.ts`：RAW 直入队（去重、错误、无预设、已入队、选择不变）。
- `components/ui`（沿用现有测试设施）：图标按钮渲染出提示内容 / 禁用态仍可被提示捕获（若测试环境不便，
  以 `check:*` 冒烟补、并在实施记录里写明取舍）。

#### 质量门（Agent）

```bash
pnpm typecheck && pnpm test
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
pnpm build && pnpm check:export        # 需另一终端 pnpm dev；断言已按新设计更新
```

#### 人类验收（崔总）

工具条观感与悬停提示手感、export 四列与筛选流程、tile 小片/RAW 按钮的实际点按、
真机 DPI 与两密度档。**「进程起来了」≠「功能对了」** —— Agent 只报告冒烟结论。

---

## 5. Pencil 画稿计划（开工后第一步，画完等崔总审）

当前活动画布：`design/export.pen`（已确认连通）。

1. **export.pen（主战场）**
   - `SQH2c` `Components / Export / ToolsBar 三段式与三态`：mid 改为紧缩后的纯图标按钮 + 新增筛选组，
     保留 1600 常态 / 560 挤压态；增加一格**悬停气泡**示范（图标按钮 + `$surface-layer` 气泡）。
   - `HYI2F` `Shell / Export / Dark-Compact` 主稿：同步 toolsbar 与上部画廊（≤2 issue + 两按钮）；
     追加筛选开启态（chips 行 + 星/色点亮）。
   - 新增 `States / Export / 画廊小片与 RAW`：0 / 1 / 2 / 3+ 张 issue 四态 + RAW 禁用态 + 全部定稿弹窗（不含 RAW）。
   - 新增 `States / Export / 四列网格`：290 / 920 宽两种中列下的列数与最小档示意（说明「−20% 换来四列」）。
2. **browse.pen**：主稿 `o2OJy2` 与 `States / 浏览标记与选择` 的工具条行去掉已精简的文字，补一处悬停气泡。
3. **editor.pen**：`FPvEw` `Components / Editor / ToolsBar 三段式` + 主稿 `P1JiB` 同步去文字。
4. **操作纪律**：一次只对一个 `filePath` 动手；每次动手前用 `pencil_get_app_state` 核对活动文档，
   画后用帧名/节点 id 复核；若 Pencil 把两个 `.pen` 解析成同一文档（2026-09-26 出过的事故），
   **立即停下报告崔总**，不自行清理。
5. 画稿定案后同步 `.md` 说明（`design/export.md` 等），再动代码（§5.1 设计先行）。

---

## 6. 本轮不做

- export 的**打标**能力（星/色/赞踩在 export 只筛不标）。
- 锁筛选的 export 入口（保持不在 export 出现）；**旗标筛选与标签筛选已改为要在 export 出现**（§1.3）。
- 新命令与热键（RAW 按钮不接命令；筛选组复用既有命令，标签菜单是控件不是命令）。
- tile 尺寸阶梯表本身（`TILE_SIZE_STEPS`）不动，只调 export 的 bounds 下限。
- 数据库 schema（无变化）；`localStorage` 仅 export 显示偏好版本号 +1（§1.4）。
