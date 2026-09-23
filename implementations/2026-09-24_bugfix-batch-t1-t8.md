# 一批 bug 修复：全屏复用 / 邻图预载 / 命令面板 / 流光 / 直方图 / 密度 / 编辑视口出图（T1–T8）

完成时间：2026-09-24 00:37:13 CST

> 上游：`plans/HANDOFF-2026-09-23-bugfix-batch.md`（上一轮的排查取证）+ 崔总当轮的**四条澄清**：
> ① T4 的旧根因（「没选中照片」）不成立 —— 选中照片也看不到 F11，且可用性判断本身值得收敛；
> ② T6 直方图「一定要小心，不要改坏」；③ T7 tiles 组件不纳入松紧调节；
> ④ T8「GPU 视图里出图功能从来没有 work 过」—— 等于把 W2 的「未验证」坐实成「没通过」。
> 本波不走 plannotator（沿用换手文档的 todos 口径），一条问题一条 todo、做完即勾。

---

## 0. 一句话

八条全部落地；**T8 找到了「从来没出过图」的根因**（`body` 那层不透明底色把 wgpu 直绘整个挡死），
并连带修掉三个会让它「看起来永远没出图」的次生 bug（提前置位 `paintedPath`、底色探针读错元素、
洞口事实补发缺失）。T1–T7 各自的根因与修法见下，全部有单测/冒烟判据钉住。

---

## 1. T1 全屏打开的是上一次那张 —— 放弃复用，每次真销毁重建

**修法**（按崔总口径「优化的点一定是载入速度而不是想着复用」）：

* `fullscreen_close` 由 `hide()` 改回 **`destroy()`**（`src-tauri/src/fullscreen.rs`）；
* 销毁是**异步派发**的（管理器条目要等运行时 `Destroyed` 才移除），所以状态里加一个
  `closing` 标记；`fullscreen_open` 发现上一扇正在销毁就**等它消失**（`wait_until_window_gone`，
  上限 500ms，用 `spawn_blocking` 让出 tokio worker）再建新窗 —— 不记这一笔的话
  「`Esc` 后立刻再开」会撞标签占用，而那时的兜底是复用，又回到旧毛病；
* 前端删掉为「隐藏复用」而生的 `visibilitychange` 兼底；保留三件提速项
  （建窗即带几何 / 窗口底色防白闪 / Rust 侧图像缓存）。

**验证**：Rust 单测（`closing` 标记、版本号、契约）；`cargo test -p raybend-desktop --lib` 65 通过。
真机「A→B 连做三次」归人类。

## 2. T2 每切一张等 1 秒多 —— 三处一起修

**根因（上一轮排查 + 本轮补证）**：

1. **Rust 侧 Screen 档根本没有磁盘缓存**：`view_image` → `display_image` → `render_file`
   每次都重新解码（RAW 走 worker 完整解码/内嵌预览，位图重新 decode+encode）。
   这是 1 秒多的**主因**；
2. 前端 `loadFor` 不读预载写下的 URL（预载只热 Rust 缓存）；
3. 一次预载两张，而 RAW 解码 worker 是串行单例 —— 用户要的那张排在预载后面。

**修法**：

* `crates/raybend/src/display/mod.rs` 新增 **`cached_image`**（带磁盘缓存的统一取图口）：
  与网格/胶片带共用同一套缓存键与 `render_sig`，写进 `_sources/thumbs.db`；
  「位图 + 原图 + 没编辑」那条直接给原文件的路**不缓存**（语义是「要原文件本身」）；
  `cache.rs` 加 `get_with_size`（一次查询连尺寸一起回，少一次读）；
* `src-tauri/src/thumbs.rs::view_image` 改走 `cached_image`（与 `thumb_get` 同一个缓存库）；
* `store.ts::loadFor` 先**吃预载缓存**（取出即接管，避免两处共用一个 URL），
  命中就直接 ready；正在预载的同一张则**等它**而不是再发一个重复请求
  （`pendingImageUrls` 从「代号」升级为「代号 + promise」）；
* `FullscreenViewer` 的预载改为**串行**（先下一张、再上一张），每一步前检查用户有没有翻走。

**验证**：Rust 新增 `cached_image` 测试（把源文件换成另一张图，第二次仍返回旧字节 = 命中）
与 `get_with_size` 测试；前端 store 新增两条测试（预载 URL 被单图路径吃掉、在途预载被等待）。
**真机 1 秒 → ≤150ms 的体感归人类**（本机无法复现大库 + RAW）。

> 缓存膨胀的欠账：`_sources` 库目前没有人在调度 GC（网格时代就如此），Screen 档比网格大一个量级；
> 登记为后续（缓存面板/容量策略属于 M2 之后的收尾）。

## 3. T3 Ctrl+K 方向键不滚动 —— 补 `scrollIntoView`

`CommandPalette.tsx` 给每行挂 ref，光标变化时 `scrollIntoView({ block: "nearest" })`
（`nearest` 只走最少的路，不整屏跳）。冒烟新增判据：连按 30 次 ↓ 后 `aria-selected`
那行必须仍在列表可视矩形内、且 `scrollTop > 0`。

## 4. T4 F11 / 全屏看图搜不到 —— 按崔总的澄清重做（含可用性框架收敛）

**先纠正上一轮的误判**：`when` 过滤确实存在，但**不是**「没选中照片」触发的 ——
选中照片时命令在列表里；真正搜不到的是**键位不在搜索索引**（实测 `scoreCommand("F11", …) === null`），
而「F11」恰恰是用户最自然的输入（面板每行右侧就写着它）。

**三条一起改**：

1. **键位可搜**：`lib/command-match.ts` 的 `CommandSearchItem` 加 `keywords`，
   面板把「当前键位 + 原始写法（`Mod+K`）」喂进去；带空格的查询同时试连写形式
   （`full screen` → `fullscreen`）；
2. **列出但禁用**（不再用 `when` 过滤）：新建 `features/commands/palette.ts` 纯函数
   `buildPaletteRows`（可单测），不可用的行灰掉并**说明原因**
   （`palette.unavailable.when` / `.enabled` 两条新文案）——与菜单「暗着但可见」
   （`DESIGN.md` §12.11 第 3 条）同一条纪律：用户能看出能力存在，也知道为什么现在点不了；
3. **可用性判定收成一条**：`lib/commands.ts::availabilityOf`（`when` + `enabled`，带原因），
   分发器 / 命令面板 / 标题栏菜单**三处读同一份**。改之前是三套口径
   （面板过滤、菜单只看 `enabled`、分发器两个都看）——同一件事三种说法，必然有一处说错。

**框架评估（崔总提的「总状态池」）**：这次做的是**最小可用的收敛** —— 把「命令能不能用」
这一个判定收成一个纯函数，四个消费方读它；`when`/`enabled` 的**数据源**仍由各工作区注册的
动作槽提供。一个真正的「总状态池」（把快捷键、UI 组件状态、命令可用性统一进一个 store）
是更大的重构，收益主要在跨界面一致性；风险是动到全部工作区的状态接线。**本轮不做**，
建议作为独立工作单元规划（`FUTURE.md` 登记）。

**验证**：`command-match` / `commands` / `palette` 三组单测（F11、`full screen`、`Mod+K`、
「不可用也列出」）；`check:browse` 新增断言（`file.exit`/`editor.lut.toggle` 不可用仍列出且带原因、
搜 F11 命中 `viewer.fullscreen`、方向键滚动）。

## 5. T5 流光轮廓比字高 4px —— 两层改同一网格单元

根因确认：底层在带 `px-8 py-1` 的父级里（受 padding 内缩），高光层是 `absolute inset-0`
（不含 padding）→ 垂直差正好 `py-1 = 4px`。修法不是「给两层写同一份 padding」（下次改一处又会错位），
而是把两层放进**同一个网格单元**（`grid` + `col-start-1 row-start-1`）——几何天然逐像素一致。
冒烟新增几何断言：两层的 `getBoundingClientRect()` 必须相同（±0.5px）。

## 6. T6 直方图等分虚线不可见 —— 只补 `background-size`

根因确凿：两处 `linear-gradient` 没给 `background-size`，渐变 `auto` 尺寸 = 整个元素，
「2px 亮 + 3px 空」只画一次。修法：竖线 `[background-size:100%_5px]`、横线 `[background-size:5px_100%]`。
**没有动任何 7 层包络 / 单通道叠加的语义**（崔总强调「不要改坏」）——
冒烟里原有的 7 块固定色、无混合模式、3 个通道按钮、逐点直连断言全部照旧通过，
另加一条**可见性断言**：每根等分线的 `backgroundSize` 必须含 `5px`（只数 span 个数抓不住这个 bug，
这正是它一直没被发现的原因）。

## 7. T7 切密度卡顿 —— tiles 组件退出松紧调节

按崔总口径（「tiles 内的 tile 不纳入松紧调节的范畴（也包括整个 tiles 组件）」）：

* `--tile-gap`（网格换行数学读的间距）**新令牌，固定 3px**，不再读密度令牌 `--gap`；
* `--tile-pad` / `--tile-bar-h` 从宽松块移除（**只在基础 `:root` 定义一次**）；
* `PhotoGrid` 的行模型改为只依赖**列数**（`columns` memo），拖左栏 / 切密度时列数没变就不重建
  O(照片数) 的行模型；
* `DESIGN.md` §8.1 新增硬规则第 5 条（tiles 不参与密度，含理由）。

**审计过的其余密度耦合**：`--gap` / `--pad-*` / `--panel-pad` / `--bar-*-h` / `--panel-w-*`
只作用于面板与条带（面板变宽会让中列变窄 → 列数可能变，这是设计本身的代价，无法也无意避免）。
`createTokenPx` 全仓只有一处（网格），已改为固定令牌 → 不再触发重算。

**验证**：`check:browse` 新增行为断言（切紧凑/宽松前后，格宽/格高/间距/行高必须完全不变）。
冒烟环境照片少，真机大库的最终体感归人类；但「tiles 不再因密度重建」这一条是结构性保证。

## 8. T8 编辑视口从来没出过图 —— 根因是 `body` 的底色（+ 三个次生 bug）

### 8.1 根因（物理约束被漏掉一层）

**`index.css` 的 `body { background: var(--surface-main) }` 是不透明的**。
窗口是 `transparent: true`、wgpu 直绘在 webview **底下** —— DOM 链上任何一层不透明都会把照片挡死。
W2 的透明链只做了「根 div → main → 洞口」，**漏了 `html` / `body`**。
spike 页（`dev/SpikeViewport.tsx`）在 JS 里显式置了 `html/body` 透明（注释就写着「否则洞口看着透明，
其实后面压着 body 的底色」），编辑器没做这一步 —— 所以「spike 看着正常、编辑器从来没出过图」。

修法：`html, body { background: transparent }` **常驻透明**（底色由各页面自己的根容器画；
必须显式写 `transparent`，因为 `color-scheme: dark` 会让「没有背景」的画布刷一层不透明的默认深色）。

### 8.2 三个次生 bug（不修的话「出图了也看不出来/看不懂」）

1. **`paintedPath` 提前置位**（`src-tauri/src/editor.rs`）：旧逻辑只要 `photo_path.is_some()`
   就在每帧后标「已画出」—— 而 `SetPhoto` 之后、解码回来之前那一帧只有洞口底色。
   后果：前端立刻把洞口切透明，且 `editorViewportNotice` 的「paintedPath 有值就不提示」
   会**压掉载入提示** → 一个空洞口、什么信息都没有。修法：加 `context.has_image()` 闸；
2. **洞口底色探针**（`features/editor/viewport.tsx`）：`readComputedBackdrop(hole)` 在洞口透明后
   读到 `rgba(0,0,0,0)` → Rust 回退深色兜底 → **浅色主题下洞口与缝变深灰**。
   修法：加一个 0×0 的稳定探针（永远带 `bg-surface-bar`），读它；
3. **洞口事实补发**（`src-tauri/src/editor.rs`）：前端上报器会去重，若首次上报早于会话建立，
   洞口矩形就**永远到不了渲染线程**（照片按整窗适配）。修法：`editor_bind_renderer` 时把
   已存下的原始载荷补发一次（`StoredViewport::replay`，新增单测）。

顺带：主题补报改走**上报器**（自带去重），不再每 250ms 一次裸 IPC ——
旧写法会让 Rust 每 250ms `dirty = true` 画一帧，违背「空闲时一帧都不画」的设计。

### 8.3 验证与真机门槛

* 离屏像素证据（`cargo run -p raybend --example editor-offscreen`）15 条断言全过 ——
  管线、矩阵、scissor、sRGB 清屏约定本身是好的；
* `ui-smoke` 新增断言：`html/body` 计算背景必须是 `rgba(0,0,0,0)`，App 根容器必须不透明；
* **真机验收（归人类）**：`pnpm debug:win` 构建后，进编辑 → 胶片带选一张 →
  洞口出图；`1` 切 1:1；滚轮锚点不漂；浅色主题下洞口底与界面一致。

---

## 9. 质量门（本轮全跑）

| 命令 | 结果 |
| --- | --- |
| `npx tsc --noEmit` | 0 错误 |
| `pnpm test` | **878 通过 / 0 失败**（上一轮 868，本波 +10：命令匹配 3 + 可用性 1 + 面板行模型 4 + store 2） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 三条全绿 |
| `pnpm build` | 通过（只有既有的 chunk 体积提示） |
| `pnpm smoke:ui` | `problems: []`（新增：html/body 透明 + 根容器有底色、流光两层几何一致、直方图 background-size） |
| `pnpm check:browse` | ✓（新增：命令面板三条 + 方向键滚动、tiles 不随密度变、等分线可平铺） |
| `cargo test -p raybend --lib` | **839 通过 / 1 ignored**（新增 `cached_image`、`get_with_size`） |
| `cargo test -p raybend-desktop --lib` | **65 通过**（新增 `closing` 标记、`replay`） |
| `cargo clippy -p raybend-desktop --all-targets` | 干净 |

## 10. 归人类的真机验收（Windows）

构建：先关掉正在运行的旧 exe（`pnpm debug:win` 的预检会拦住活着的实例），再 `pnpm debug:win`。

1. **全屏**：选 A → 全屏看 A → `Esc` → 选 B → 全屏**必须是 B**；连做三次；
   再按一次 `Esc` 后**立刻**再开，也要出得来（等旧窗销毁那条链）；
2. **载入**：连续 `→` 翻 20 张，除第一张外应基本秒开（≤150ms 的体感）；
3. **命令面板**：`Ctrl+K` 搜 `F11` / `全屏` / `fullscreen` 都能看到「全屏看图」；
   没选中照片时它是**灰的**且写着原因；方向键按住 ↓ 到底，选中行不跑出可视区；
4. **直方图**：三条竖线 + 一条横线**整条**是细密虚线（不再是两头 2px）；
5. **密度**：切紧凑/宽松，tiles 里的格子大小与间距**不变**（右栏 padding 仍要跟着变）；
6. **编辑视口**（本轮重点）：进编辑 → 胶片带选一张 → **洞口出图**；RAW 与 JPG 各一张；
   `1` 切 1:1 看真实像素、`0` 回适合窗口；滚轮压在特征点上不漂；
   浅色主题下洞口底色与界面一致；切主题后洞口底跟着变。

## 11. 遗留

| 项 | 说明 |
| --- | --- |
| `_sources` 缓存的 GC 调度 | 见 §2 末注；Screen 档写入后缓存增长更快，容量策略属于缓存面板那一波 |
| 「总状态池」重构 | 见 §4 末；已登记 `FUTURE.md`，作为独立工作单元评估 |
| T7 大库真机体感 | 结构性耦合已拆除（tiles 不再随密度重建），最终数值归人类真机 |
| T8 真机确认 | 本轮已把「物理上不可能出图」的根因拆掉，但**必须真机复验**才算过 |
