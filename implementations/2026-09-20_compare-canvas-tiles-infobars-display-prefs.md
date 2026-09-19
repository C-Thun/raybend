# 对比虚拟画布、tiles 信息条、赞踩取消、显示偏好持久化

完成时间：2026-09-20 03:16:48 CST

## 范围（人类 2026-09-20 第二批，原文见 `docs/user-requirements.md`）

1. **对比改成「虚拟画布」**：比例不同的图放在一起时旧口径会把后面的图裁成第一张的比例
   ——「一缩就露馅」。新口径：所有图按**原尺寸居中**贴进一张画布（最大宽 × 最大高），
   统一倍率、按画布位移；双击 = 适合窗口（**按被双击的那张算**）↔ 100%；
   进入/换图集时重算画布、重新居中、按**像素数最小**的那张算适合窗口。
2. **双击 100% ↔ 适合窗口**（上一版有 bug，重做时定位到真因）。
3. **tiles 顶部标记条整条没了**（含底部文件名条的半透背景）。
4. **信息档位**（含第 1 级）无效；并按新口径重做强制显示形态。
5. **赞 / 踩取消不掉**（重复点 → 「没有需要改动的照片」）。
6. **持久化**：「按时间」在 import 与 browse 都要持久化（浏览侧以前每次进重置），
   信息显示级别也要持久化。
7. 直方图尖刺（上一轮遗留，本轮一并收口）。

## 涉及文件

- 对比：`src/lib/viewer-compare.ts`（+ 测试）、`src/components/ui/viewer/CompareView.tsx`、
  `scripts/check-browse-boot.mjs`
- tiles：`src/components/ui/Tile.tsx`、`src/components/ui/tile-info.ts`（+ 新测试）、
  `src/features/photo-grid/PhotoGrid.tsx`、`src/workspaces/browse/BrowseWorkspace.tsx`、
  `src/workspaces/import/ImportWorkspace.tsx`
- 赞/踩：`src/features/browse/BrowseToolbar.tsx`
- 持久化：`src/lib/display-prefs.ts`（新，+ 新测试）、`src/features/photo-grid/store.ts`
  （+ 测试）、`src/workspaces/browse/BrowseWorkspace.tsx`
- 直方图：`src/lib/histogram.ts`（+ 测试）
- 规范：`BROWSE.md` §3.2 / §3.2.1 / §5.7、`DESIGN.md` §13.1 / §13.3 / §13.3.1、
  `plans/M2-W2.md` 2.3

## 关键决策与理由

### 1. 对比：一张虚拟画布，谁都不裁

* 画布 = **各图最大宽 × 最大高**（原图像素）；每张图按自己的原尺寸**居中**贴进去。
  人类点名的例子：3:2 的 6000×4000 + 4:3 的 3750×5000 ⇒ 画布 6000×5000，
  竖图左右留 1125、横图上下留 500，两张都完整落在画布内。
* **栏区（窗口）是可见边界**：`overflow: hidden` 在分到的那一格上，放大后画布铺满整格。
  画布尺寸与「栏区里那个 contain 布局盒」的关系用一条 `transform: scale(zoom / layoutBase)`
  表达 —— 缩放只改 transform，不触发四幅图重排。
* **倍率变成绝对倍率**（画布像素 → CSS 像素，1 = 100% = 1:1）：这样「统一倍率」才有确定含义，
  不再需要上一版的「相对适配倍数」。
* 旧的 `cropToAspect` / `baselineAspect` / `compareGeometry` / `panPercent` **全部删除**
  （裁剪口径作废，`plan/W2` 的 2.3 同步改写）。

### 2. 双击的真 bug：effect 把刚设好的缩放又按回去了

上一版的「双击 → 100%」写的是 `setZoom(1)`，而另有一个 effect 负责「保持适合窗口」
（盯着 `fitId`，尺寸/栏区一变就重算 zoom）。Solid 的 setter 在事件处理器里**不是批处理的**，
于是 `setZoom(1)` 立刻唤醒那个 effect，它读到「还在适合窗口状态」就把 zoom 又按回适配值 ——
双击看起来「没反应」（本次专门留了探针复现：日志里 `zoom=1` 紧跟着一条
`fitEffect … next=0.0801`）。

修法不是加 `batch()` 了事（那只是盖住），而是**把双向写改成派生**：
`manualZoom`（用户自己给的）与 `fitId`（适配以哪张为准）是状态，
**生效倍率 = memo**（`fitId` 非空就按那张图算，否则用 `manualZoom`）。
这样「谁最后写谁赢」的路径根本不存在，尺寸晚到、栏区被拉大也自动重算。

### 3. tiles 顶部条：`inLibrary()` 一直没人满足

`Tile` 的顶部标记条写在 `<Show when={inLibrary()}>` 里，而 `inLibrary()` 要求
`context === "library"` —— 但**两个工作区的网格都没传这个 prop**，
于是那条从组件重做（a8013fa）起就**从来没渲染过**。人类报的「顶部信息条没了」
不是「被改坏」，是从来没上过。修法：`PhotoGrid` 传 `context="library"`。

### 4. 信息档位：强制显示只属于「未选中、未悬浮」

新口径（人类 2026-09-20）：

| 情形 | 顶部（标记） | 底部（文件名） |
| --- | --- | --- |
| `off` 档 | 指向/聚焦/选中才出现（半透底 + `fg-1`） | 同左 |
| `marks` 档 · 未选中未悬浮 | **强制显示**：无底纹 + 反色勾边 | 不显示 |
| `marks-name` 档 · 未选中未悬浮 | 同上 | **强制显示**：无底纹 + 反色勾边 |
| 任意档 · 选中 / 悬浮 | **一律回到标准方案**（半透底，**不要勾边**） | 同左 |

实现：每条信息条拆成**两层**（强制层 / 标准层），两层共用同一个内容子组件
（`TileMarks` / `TileName` —— 一份实现、两处渲染）。hover 的切换交给 CSS
（`group-hover/tile:opacity-0` 淡出强制层、标准层淡入），与 `RAW` / `+RAW` 角标同一套做法：
**没有指向/选中时才加强显示**，一旦选中/悬浮就回到原方案。

### 5. 赞 / 踩：「再点一次」必须发 `null`

`markLike()` 无条件把 `value` 发出去，第二次仍是 `"like"` → 后端判定无改动 →
弹「没有需要改动的照片」，用户永远取消不掉。改成与锁同一条口径：
**已经是这个值就发 `null`**（清空）。

### 6. 显示偏好：一份状态、两处读、设备级落盘

新增 `lib/display-prefs.ts`：`byTime`（按时间）/ `infoMode`（信息档位）/ `tileStep`（格子档位）
三项**模块级单例信号** + `localStorage`（键 `raybend.display.v1`）。

* 以前 `by_time` / `tile_step` 只有**导入侧**读写 `app.db`（浏览侧是本地信号），
  于是浏览里开了「按时间」、切走再回来就重置；信息档位则只是内存单例，关软件就忘。
* 现在两个工作区读同一份：浏览侧的 `grouped` / `tileStep` 直接换成共享偏好，
  导入侧的网格 store 也改成委托（`grid.by_time` / `grid.tile_step` 两个键不再使用）。
* 选 `localStorage` 而不是 `app.db` 的理由与 `appearance` / `layout-prefs` 一致：
  **要在首次渲染前拿到**（异步读库会先画默认值再跳变，分组一跳就是整屏重排）。
* 格子档位仍是「拖动只改内存、松手落盘」（`setDisplayTileStep` / `commitDisplayTileStep`）。

### 7. 直方图尖刺

真因是色带只在「某通道是本列冠军」的列上有值、其余列给 **0**：冠军一换，折线就从曲线上
直直落回基线，画出来是一根细尖刺。改成**每层用自己的边界曲线表达、处处有定义**
（厚度可为 0）；连续化之后「平局规则」也不需要了。详见 `DESIGN.md` §13.1。

## 验证（Agent 侧，冒烟）

- `pnpm test`：**707 通过 / 0 失败**。本批新增/改写：
  `viewer-compare.test.ts`（画布尺寸 / 居中留白 / 谁都不裁 / 像素数最小 / 脏尺寸）、
  `tile-info.test.ts`（档位循环 + `i` 键适用范围）、
  `display-prefs.test.ts`（脏存储值 / 单例共享 / 夹取）、
  `histogram.test.ts`（冠军换人不下基线）、`store.test.ts`（档位与「按时间」改到共享偏好）。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过
- `pnpm build`：通过；`pnpm smoke:ui`：通过（`problems: []`）
- `pnpm check:browse`：通过。新增断言（真浏览器 + 假后端）：
  * 对比：每格画布同尺寸、**混合比例**（横 4000×3000 + 竖 2400×3200）时画布 4000×3200、
    两张都在画布内（**谁都没被裁**）、留白居中、适合窗口按最小的那张算、
    **双击 → 100%**（画布 4000×3000 按原图像素）、再双击回到适合窗口、真鼠标拖动跟手；
  * tiles：顶部标记条在 DOM 里、标准层带半透底、`i` 三档、强制层无底纹+勾边、
    指向时切回标准层（半透底、无勾边）、选中后强制层消失；
  * 显示偏好：点「按时间」后 `localStorage["raybend.display.v1"].byTime === true`、
    信息档位与界面当前档位一致；
  * 赞/踩：选一张已喜欢的照片点「喜欢」→ 请求里 `value === null`（取消）。
  * 顺带修了冒烟环境本身：**无头 Chrome 默认 `(hover: hover) = false`**，
    Tailwind 的 `hover:*` / `group-hover:*` 变体整条失效（怎么移鼠标信息条都不浮出，
    一度误以为是 CSS 写错）——现在用 `--blink-settings=…HoverType=2…PointerType=4`
    把「有鼠标」声明出来，并在断言前先验 `matchMedia("(hover: hover)")`。

## 未经 Agent 验证（需人类真机/目视）

- Windows 上：4 种分栏（2/3/4）里混合比例对比的实际观感（不裁、居中、缩放手感、
  双击切换、拖动范围）；tiles 顶/底信息条与三档信息模式的实际观感；
  赞/踩取消后的图标与 toast 行为；按时间分组的持久化（重启软件后再看）。
- 直方图在真实照片上是否还有尖刺（截图里那类异常只能靠眼睛判定）。

## 遗留问题

- **弹窗层级**（人类 2026-09-20 报，已记进 `docs/user-requirements.md` 与 todo）：
  模态浮层应是全局最高（仅次于 `titlebar` 与右上角 toast），现在 `view` / `film`
  覆盖层在它之上 —— film/view 态下点库齿轮，弹窗被挡、只有周围遮罩变暗。
- **切语言时横向选择器选中底色**（同批新报，已记进需求原文与 todo）：
  切中英文那一瞬间，`titlebar` 的紧凑/宽松与 flow 选择器的选中段底色会变。
- `app.db` 里的 `grid.by_time` / `grid.tile_step` 两个旧键**不再被读写**（偏好搬去了
  `localStorage`），老值不会迁移 —— 首次升级后这两项回到默认，需要重新点一次。
  旧键本身留着不动（`src/api/db.ts` 的 `SETTING_KEYS` 里仍在），要清的话等一次专门的清理。
- 对比的 `zoom` / `pan` / `fitId` 仍住在视图里（纯函数部分都有单测），
  将来接 Rust 原生视口时一并搬走。
