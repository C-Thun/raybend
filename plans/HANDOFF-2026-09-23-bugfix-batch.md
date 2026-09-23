# 换手文档：全屏 / 载入 / 直方图 / 密度 / 缩放 —— 一批 bug 与体验问题（2026-09-23 晚）

> 写给**下一个 session**。本批**没有改任何代码**，只做了排查取证。
> 请先按 §2 建 todos（**一条问题一条 todo**），逐条修、逐条勾 —— 见 `~/.pi/agent/AGENTS.md` 的「任务清单纪律」。

---

## 0. 开工前必读

* `AGENTS.md`：§2.14（一轮清单要连续做完）、§2.15（新增功能必须同时决定热键）、
  §2.17（`untrack` 必须写在插入点，两面的教训）、§11.4/§11.5（术语与结构红线）。
* `DESIGN.md`：§8.8 / §8.9（滚动条占位与预留空间）、§12.10（状态水印与流光）、§13.1（直方图合成）。
* 本批已落地的改动与理由：`implementations/2026-09-23_ui-batch-fullscreen-loading-density-zoom.md`。
* 质量门（改完必跑，全绿才算完）：

  ```bash
  npx tsc --noEmit
  pnpm test                                   # 单测（秒级）
  pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
  pnpm build
  pnpm smoke:ui                               # 需另开终端 pnpm dev
  pnpm check:browse                           # 网格/胶片带/直方图的几何回归（需 dev server）
  cargo test -p raybend --lib && cargo test -p raybend-desktop --lib
  cargo clippy -p raybend-desktop
  ```

* 提交纪律（仓库 `AGENTS.md` §2.2）：实现完成后可自行 `git commit`（中文 `<type>: <subject>`）；
  **push / tag / 发布一律归人类**。处于 plannotator review 流程时**禁止** commit。

---

## 1. 目前进度

| 范围 | 状态 |
| --- | --- |
| **M3-W1 / W2**（编辑器视口：透明挖洞 + wgpu 直绘 + 真实照片 + 渲染线程与监督器） | ⚠️ W2 的「人类已验」不成立 —— 崔总澄清「GPU 视图出图从来没 work 过」。**2026-09-24 已定位并修复根因**（`body` 不透明底色挡死 wgpu），真机复验归人类，见 `implementations/2026-09-24_bugfix-batch-t1-t8.md` §8 |
| **M3-W3**（显影管线位置、RAW 线性源、编辑栈/issue 落库） | ⬜ **未开工** —— 规划与必答问题在 `plans/HANDOFF-2026-09-23-w3.md`；崔总 2026-09-23 定：本轮只做 bug 修复，**未得明确指令不进 W3** |
| 今天下午/晚上这批（全屏看图、载入流光、直方图固定色、密度与滚动条、17 档重排） | ✅ 完成并提交（`7ff75fb` / `8acb352` / `6b7f76c` / `1df90d1` / `472322d`） |
| **本文档 §2 的 8 条问题** | ✅ **T1–T8 全部落地**（含崔总四条澄清的重做）—— 实施记录见 `implementations/2026-09-24_bugfix-batch-t1-t8.md`，真机验收清单在那一份 §10 |

---

## 2. 任务清单（请建成 todos）

> 每条都给了「现象 / 已查到的证据 / 建议修法 / 验收」。**证据里带文件与行号**，直接去看那一处。

### T1 `fullscreen` 打开的是**上一次**那张图（复用窗口这条链不可靠）

* **现象**：选 A 全屏 → 退出 → 选 B 全屏 → 打开看到的是 **A**。
* **人类口径**（重要，直接决定修法）：**「怎么可能这个窗口还能复用啊？优化的点一定是载入速度而不是想着复用」**
  —— 即：**放弃窗口复用**，每次重新建窗；速度靠别的办法（见 T2 与 §T1 的加速项）。
* **已查到的机制**：为了「秒开」我把 `Esc` 做成了 `hide()`（`src-tauri/src/fullscreen.rs` 的 `fullscreen_close`），
  重开走 `fullscreen_open` 的复用分支（`emit(FULLSCREEN_EVENT)` + `show()` + `set_focus()`）。
  页面侧靠两条路拿新清单：事件监听 + `visibilitychange` 重读（`src/features/fullscreen/FullscreenViewer.tsx`）。
  **原生窗口的 hide/show 不保证触发页面的 `visibilitychange`，隐藏期间 WebView 也可能被挂起、事件收不到** ——
  于是页面始终停在旧清单上，正是「打开看到 A」。
* **修法**：`fullscreen_close` 改回**真销毁**（`destroy()` 或 `close()`），每次打开都新建窗口 + 页面挂载时主动读一次 payload（已有）。
  保留已经做好的加速项：① 建窗即带目标屏幕几何；② 窗口底色用前端报的 `--surface-track`（防白闪）；
  ③ Rust 侧图像缓存（见 T2 与下面的更正）。
* **验收**：选 A → 全屏看 A → `Esc` → 选 B → 全屏**必须是 B**；连做三次不错位；多屏下仍落在主窗口那块屏。

### T2 邻图预载「没生效」，每切一张仍要 1 秒多的「载入中」

* **现象**：过程流畅（有遮罩），但每张都要等 1 秒多。
* **已查到的两处根因（都要看）**：
  1. **单图路径根本不读预载写下的缓存**：`src/components/ui/viewer/store.ts:387` 的 `loadFor()`
     每次都直接 `await deps.loadScreen(photo.path)`，然后 `replaceUrl(makeUrl(bytes))`；
     而预载走的是 `ensureImage()` → 写进 `imageUrls`（那张表是给**对比视图**用的，`imageUrlFor` 才读它）。
     → 预载只可能热到 **Rust 侧的磁盘缓存**，热不到前端这张 URL。
  2. **RAW 解码 worker 是串行单例**（`crates/raybend/src/raw/worker.rs`，全 RAW 解码共用一个互斥）：
     我一次预载**两张**邻居，用户真正要的那张会**排在预载后面** —— 越预载越慢。
* **建议**：① 让 `loadFor` 先查 `imageUrls`（命中就直接用，不再走 IPC）；② 预载**一次只预一张**（或给队列加优先级：
  用户请求优先、预载可丢弃）；③ 步进时把在途的预载请求作废（`ensureImage` 已有 generation 机制，看看能否复用）。
* **更重要的排查方向**：先量清那 1 秒花在哪 —— Rust 侧 `display_image` 的 Screen 档**是否有磁盘缓存命中**、
  还是每次都在重新解码。日志/计时入口：`src-tauri/src/thumbs.rs::view_image` → `display::display_image`。
* **人类补充口径**（原话）：「以后加了图片编辑后载入的确实是编辑后的图片副本（那实际上会生成缓存，本质全屏模式也不会每次实时从 RAW 走管线渲染）」
  —— 也就是说**缓存命中是设计前提**，1 秒多本身就是问题。
* **验收**：连续 `→` 翻 20 张，除第一张外每张的「载入中」应 ≤ 150ms（或直接不出遮罩）。

### T3 `Ctrl+K` 面板：方向键移动光标时列表不滚动，选中行会跑到可视区外

* **证据**：`src/features/commands/CommandPalette.tsx` **完全没有** `scrollIntoView` / `scrollTop` 相关代码
  （光标只由 `move()` 改 `cursor()`；列表是 `ranked()`）。
* **修法**：给行元素挂 ref，`cursor()` 变化时把当前行 `scrollIntoView({ block: "nearest" })`（`nearest` 才不会整屏跳）。
* **验收**：命令数超过一屏时，按住 `↓` 到底、再 `↑` 回顶，选中行**始终在可视区内**。

### T4 `F11` / 「全屏看图」在 `Ctrl+K` 里搜不到（`Ctrl+,` 快捷键设置里有）

* **证据**：`CommandPalette.tsx` 的 `rows()` 有一句
  `.filter((command) => command.when?.() !== false)` —— **`when` 为假的命令直接从列表里消失**。
  而 `viewer.fullscreen` 的 `when` 是「真的能开」（工作区给了清单 = 有当前照片），
  在**没选中照片**时打开面板就找不到它 ✗。（`ShortcutSettingsDialog` 不过滤，所以 `Ctrl+,` 里能看到 ✓。）
  另外搜索只匹配 `id / title / group`（`lib/command-match.ts`），中文界面下搜英文 "full screen" 只能靠 id 命中。
* **修法（两条都要，属于产品口径，实施时在记录里写清）**：
  ① 命令面板**不要用 `when` 过滤**，改成**列出但禁用**（灰掉 + 说明为什么不生效）——
     这样「找不到命令」不会再发生；`run()` 里已经会挡 `enabled?.() === false`。
  ② 搜索关键字补 `command.id`（已有）与**英文别名/键名**（例如把 `F11`、`fullscreen` 也纳入匹配）。
* **验收**：没选中任何照片时打开 `Ctrl+K`，搜「全屏 / fullscreen / F11」都能看到它（禁用态）；选中照片后可执行。

### T5 载入流光的**轮廓与内容错位**（比字与图标高几个像素）

* **证据**：`src/components/ui/StateWatermark.tsx` —— 底层内容是普通流（父级有 `px-8 py-1`），
  高光层是 `absolute inset-0`（**不含** `px-8 py-1`）→ 高光整体偏移 `py-1 = 4px`（水平方向因为居中所以看不出来）。
  「高了几个像素」正好对上。
* **修法**：给高光层同样的内边距（或让两层共用一个内层容器，`inset-0` 相对**内容盒**）——
  关键要求：两层几何**逐像素一致**，只有颜色与遮罩不同。
* **验收**：流光扫过时，字与图标的每一笔都在原位变亮，没有位移；冒烟已有「流光层里必须有图标」的断言，可再加一条几何断言
  （两层的 `getBoundingClientRect()` 必须相同）。

### T6 直方图的**3 竖 + 1 横等分虚线看不到**（只有顶端/两端各 2px 的小段）

* **根因（已定位，证据确凿）**：`src/components/ui/Histogram.tsx:97` 与 `:102` 的类：
  `bg-[linear-gradient(to_bottom,var(--hist-grid)_0_2px,transparent_2px_5px)] bg-repeat-y`
  —— **没有给 `background-size`**。渐变的 `auto` 尺寸 = 整个元素，于是那条「2px 亮 + 3px 空」的图案
  **只画了一次**（剩下全是最后一段的 `transparent`），`repeat-y` 无从重复 →
  看到的正是**最上面那 2px**（横线同理：只有最左 2px）。人类描述「像用尺画线，两头画了短线、中间那条直线一直没画出来」完全吻合。
* **修法**：给渐变一个**可平铺的尺寸**：竖线 `[background-size:100%_5px]`（或 `1px 5px`）、横线 `[background-size:5px_100%]`；
  或者干脆改用 `border-image`/`repeating-linear-gradient`（更直白：`repeating-linear-gradient(to bottom, var(--hist-grid) 0 2px, transparent 2px 5px)`，
  但注意色值只能来自令牌 —— `lint:colors` 会拦裸色值）。
* **验收**：三条竖线、一条横线**整条可见**（细密虚线）。冒烟补一条**可见性**断言：
  量 `getComputedStyle(el).backgroundSize`（必须含 `5px` 之类的平铺尺寸），
  或直接量元素上某一点的像素（现有断言只数了「4 根 span 存在」，所以这个 bug 一直没被发现 ✗）。

### T7 切「紧凑 / 宽松」时界面卡 ~1.5 秒

* **已查**：浏览器（空库）里实测切换只花 **16–33ms、无 long task**（测量脚本思路：`PerformanceObserver(["longtask"])` +
  两次 `requestAnimationFrame` 包住 `dataset.density` 的切换）→ **与照片数量相关**，真机 + 真实库才会复现。
* **已排除**：`measureScrollbarWidth()` 只在 `PhotoGrid` 创建时调一次（`src/features/photo-grid/PhotoGrid.tsx:153`）；
  行模型是纯数学（`src/lib/tile-flow.ts:265` 的 `tileRowHeight`）；令牌读取走 MutationObserver 而不是每次 `getComputedStyle`
  （`src/components/ui/tokens.ts`）。
* **下一步（用 profile 说话，别猜）**：真机上开 Performance / `longtask` 记录，切一次密度，看时间落在哪一类：
  ① **脚本期**：`createTokenPx` 有 N 个实例，每个都在 `data-density` 变化时 `getComputedStyle(document.documentElement)`
     —— 每次读计算样式都会**刷新全文档样式**，N 次 = N 次全量样式重算（大 DOM 下这是经典秒级开销）；
  ② **布局期**：本次新加的 `.scroll-y-reserved { scrollbar-gutter: stable }`（`src/styles/scrollbar.css`）
     与右列 padding 变化（`--panel-pad-scroll`）会让滚动容器整棵子树重排 —— 若布局占大头，先做 A/B（临时去掉 `scrollbar-gutter` 再量）；
  ③ **网格重建**：密度会改 `--gap`，行模型会整体重算（`buildGridRows`）—— 看它是否随照片数线性变慢。
* **验收**：切密度 ≤ 100ms（体感不卡），且右列 padding 仍然跟着变（T7 与密度效果不能二选一）。

### T8 editor 视口「镂空里什么都没有，点图片不显示」

* **先确认进度与前提**（避免白查）：
  * W1/W2 是**已经做完**的：透明挖洞 + wgpu 直绘 + 真实照片 + 渲染线程/监督器（人类验过 W2）；
    **W3（管线、编辑栈、issue、曲线控制点）未开工** —— `src/features/editor/panels.tsx` 里
    「拉杆改数值不改画面（W3）」「issue 落库在 W3」「曲线是恒等曲线（W3）」都是明写的占位 ✓ 这是**预期**，不是 bug。
  * 所以「看不到任何内容」如果指**照片本身也不出来**，那才是问题；如果指**拉杆/曲线不动画面**，那是 W3 的范围。
* **诊断入口（现成、够用）**：
  * IPC `editor_render_state`：`bound / ready / paintedPath / decode / decodeError / tier / wantedTier / lastError / restarts`；
  * IPC `editor_viewport_state`：`holeCss / holePhysical / dpr / backdrop / updates`；
  * 洞口 DOM：`[data-editor-viewport]` 的 `data-viewport-painted`（`on` 才该透明）。
* **排查顺序**：① 用的是不是**最新构建**（今天下午之后 Rust 有改动，`src-tauri/src/fullscreen.rs` 等；
  旧 exe 不含新改动 —— 但编辑器部分今天没动，若旧 exe 上编辑器就不出图，那与今天的改动无关）；
  ② browse 里**确实选中了一张**（编辑器取的是 browse 选择锚点）；③ 渲染线程是否 `bound`（懒启动）；
  ④ `decode` 是否失败（`decodeError`）——RAW 走完整解码，可能慢/失败；⑤ 洞口是否因为 `paintedPath == null` 而保留 DOM 底色。
* **验收**：browse 选一张 → 切编辑 → 洞口出现该照片（`painted=on`），缩放/拖动/双击 100% 可用（W2 的能力）。

---

## 3. 顺带更正一条旧笔记（别再去查）

`implementations/2026-09-23_fullscreen-viewer.md` 的遗留第 2 条说「TS 的 `ImagePurpose` 含 `original` 但 Rust 不认」——
**这是错的**：`crates/raybend/src/display/mod.rs` 的 `ImagePurpose::parse` **支持四个意图**
（`grid / strip / screen / original`，有单测 `purpose_parsing_covers_the_four_intents_and_rejects_junk` 盯着）。
我当时看的是 `thumbnail/render.rs` 里 `SizeClass::parse`（那个只有三档，是**另一套**枚举）。
→ 也就是说：**要「真 1:1 原图像素」可以直接用 `getViewImage(path, "original")`**，不需要在 Rust 加档。
（T2 若要做「更清晰的看图」，这是一个现成的入口。）

## 4. 本轮踩过的坑（写在这里，省下一个 session 的时间）

1. **冒烟脚本里的注释不许有反引号**（`//` 与 `/* */` 都算）—— 它会把 `evaluate` 的模板串截断，
   报出的错与真因无关；脚本自己有自检（`pnpm smoke:ui` 会先跑一遍）。
2. **`evaluate` 里的 `async` IIFE 必须配 `awaitPromise: true`**，否则 CDP 回包是被序列化成 `{}` 的 Promise
   （本轮就因此误判过一次「对象缺字段」）。
3. **`scrollbar-gutter: stable` 的语义**（实测）：滚动条画在 padding **之外**（贴外缘），
   所以 padding 就是「内容 ↔ 滚动条」那道空隙；`stable` 下有无滚动条的 `clientWidth` 完全相同（293/293 vs 308/293）。
4. **改 CSS 色值只能写在 `src/styles/tokens.css`**（`pnpm lint:colors` 会拦，连 mask 里的 `#000` 都拦）。
5. 档位表（tiles / film 的 17 档）改动要顾 `migrateTileStepIndex`：**存的是下标**，
   表变了必须按「尺寸最接近」换算，否则老用户会忽然变小。
