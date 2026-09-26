# 支线：网格载入/空态水印 + 前端文案入语言包

> 状态：**待评审**（四个口径已由人类 2026-09-17 定；Pencil 阶段明确跳过）
> 关联：`AGENTS.md` §2.7（实施记录）、§2.8（E2E 归人类）、§5.3（质量门）；
> `DESIGN.md` §11（i18n）、§12.9；`design/main.md` §3.2 / §9.5；`FUTURE.md` G6
> 不属于 `PLAN.md` 主线（M2-W1）—— 是穿插的两个小优化。

---

## 1. Context（为什么做）

三件事，来自人类 2026-09-17 的口述：

1. **载入态太弱**：打开一个库外目录（导入工作流的中列）时，「扫描目录 + 把文件基本信息
   （宽高 / 方向）读进会话级缓存」需要几秒。这期间中列只有一行小字 `加载中`
   —— 既不像「正在忙」，也不像有意为之。
   → 要求：**整个列表区域**铺一层不抢眼的载入表现：**大图标 + 文字**，
   做成「印在背景底纹上」的观感，带**缓慢渐变闪烁**；**等缓存建立后再显示照片**。
2. **空态太弱**：目录里没有照片时只有一行小字；浏览网格**根本没有空态**（一片空白）。
   → 要求：同款的「印在底纹上」空态（大图标 + 文字）。
3. **文案没有全进语言包**：语言包本身是满的（见 §3.1 的核实），
   问题在**包外散落的文案** —— 前端还有约 15 处硬编码，其中几处会真的显示给用户。

**产出**：两个照片网格的载入/空态统一成一套「水印」表现；前端可见文案零硬编码，并加守门脚本。

---

## 2. 已定的四个口径（人类 2026-09-17）

| # | 问题 | 决定 |
| --- | --- | --- |
| 1 | 载入动画覆盖到哪一刻 | **扫描 + 头部缓存都铺完**再一次性显示照片（比例一次到位，不再「长大」）。缩略图仍逐张到位 |
| 2 | 空态/载入态铺到哪些地方 | **只两个照片网格**。导入网格：载入 / 目录为空 / 未选目录；浏览网格：载入 / 库内无照片（其余面板维持现状） |
| 3 | i18n 范围 | **前端（`src/`）可见文案 + `pnpm lint:i18n` 守门**；Rust 错误串登记 `FUTURE.md`；`src/dev/**` 与 CLI 脚本明确排除 |
| 4 | 设计稿 | **跳过 Pencil 阶段**：`.pen` 一个字节都不动；只在 `design/main.md` §9.5 补一条「画布无帧、待补」的登记（将来要补也是**新增帧**，不是改既有帧） |

---

## 3. 探查结论（已核实，含可直接复用的东西）

### 3.1 语言包是满的 —— 问题在包外

* `zh-CN.ts`（234 行）与 `en-US.ts`（227 行）**逐键对应**：
  `MessageKey = keyof typeof zhCN`，`enUS: Record<MessageKey, string>`（**漏译是编译错误**）；
  `locale-parity.test.ts` 再加双向断言 + 空文案 + 占位符一致性 + 双反斜杠四道检查。
* 所以「语言包没填内容」不成立。真正的漏网之鱼是 **`t()` 之外的字符串**（见 §5）。

### 3.2 两个网格、八处状态（导入 4 + 浏览 4）

| 位置 | 现在 | 文件 |
| --- | --- | --- |
| 导入网格·未选目录 | `Hint text={t("grid.pick_dir")}` | `src/features/photo-grid/PhotoGrid.tsx:199` |
| 导入网格·准备中 | `Hint text={t("common.loading")}` | 同文件 `:207` |
| 导入网格·目录为空 | `Hint text={t("grid.empty_dir")}` | 同文件 `:208` |
| 导入网格·读不了 | `Hint tone="error"` + 重试 | 同文件 `:200` |
| 浏览网格 | **载入/空态/错误态都没有**，`total()===0` 时一片空白 | `src/features/browse/BrowseGrid.tsx` |

* `Hint` 是 `PhotoGrid.tsx` 里的**局部组件** → 两处共用必须提升到 `components/ui/`。
* **「等缓存建立」目前并不成立**：`store.ts` 的 `load()` 扫完就 `setStatus("ready")`，
  `loadPhotoMeta()` 是 `void` 后台跑（`store.ts:198`）——
  照片先按占位比例 3:2 铺出来，头缓存到了各自「长大」。这正是人类点名的现象。
* `store.status()` 的 `loading` 窗口 = 扫描；**改法见 §4.A**。

### 3.3 可复用 + 约束（照抄，别自己发明）

| 需要 | 现成的 | 备注 |
| --- | --- | --- |
| 动画的归属地 | `src/styles/motion.css` | 现有 `rb-pop-up` / `rb-fade-in` + `prefers-reduced-motion` 兜底 |
| 「低存在感」的基准 | `Tile.tsx` 的加载占位（`animate-pulse`） | 保留不动：**每股 tile 自己的事** |
| 图标 | Tabler（`@tabler/icons-solidjs` 3.46）：`IconPhoto` / `IconPhotoOff` / `IconFolderOpen` / `IconAlbum` / `IconAlbumOff` / `IconAlertTriangle` | 已在用同一套 |
| 令牌 | `--fg-3`（注释/次要）、`--danger`、`--surface-bar`、`--fs-*`、`--gap*` | 色值只许出现在 `tokens.css`（`lint:colors` 盯着） |
| 分层 | `lib` **不能** import i18n（`scripts/check-architecture.mjs` 的 `mayImport` 表）；`api` / `features` / `ui` 可以 | 决定 §4.D 的做法 |
| 虚拟网格 | `VirtualGrid`（`components/ui/`） | 载入/空态时**不渲染它**，换成水印 |

---

## 4. Approach（推荐方案）

### A. 载入门：把「准备中」变成 status 的一部分

`src/features/photo-grid/store.ts` 的 `load()`：

```ts
const scan = await deps.api.scanSourceDir(next);
if (token !== generation) return;
setItems(scan.items);          // 计数先出来（控制条上）
setProblems(scan.problems);
await loadPhotoMeta(next, scan.items, token);   // ← 新增：等头部缓存铺完
if (token !== generation) return;               // ← 迟到就丢掉（用户又换了目录）
setStatus("ready");            // ← 只有到这一步才铺 tile
if (byTime()) void loadTimes(); // 后台补读真实拍摄时间（控制条自己有小提示）
```

* **不加新状态**：`status === "loading"` 自然覆盖「扫描 + 头部缓存」两段，视图侧零改动就换上了水印。
* `loadPhotoMeta()` 保持**吞掉异常**（读不到头不是错误，比例退回占位）→ 失败也照样 `ready`，
  绝不把网格卡死在载入态。
* **必须加一道时限**（重要，不加上就把一个新风险引进来了）：后端命令**panic 时 promise 永远不 settle**
  （`lib/timeout.ts` 文件头记的就是这个坑）—— 今天元信息不阻塞显示，所以没人注意；
  现在网格**依赖它**了，一旦后端挂掉就会永远停在水印上。
  所以：`loadPhotoMeta` 内部的 `dirMetaEnsure` 也包一层 `withTimeout`
  （`DEFAULT_META_TIMEOUT_MS = 20s`，可注入以便测试），超时就走 catch → 占位比例照常铺照片。
  时限的目的不是「限制慢盘」（所以给得宽），而是「把卡死变成能用的界面」。
* 代价（人类已知并接受）：首开大目录要多等那几秒；**同目录二次打开命中会话级缓存 → 瞬间过去**。

### B. `src/components/ui/StateWatermark.tsx`（新组件）

```tsx
export interface StateWatermarkProps {
  icon: JSX.Element;         // 大图标（调用方给，64px / stroke 1）
  text: string;              // 一句话（已翻译）
  animate?: boolean;         // 载入态：慢呼吸 + 高光扫过；默认静态
  tone?: "muted" | "error";  // 错误态用 danger 的弱化浓度
  action?: { label: string; run: () => void };  // 例如「重试」
  class?: string;
}
```

* 结构：整块居中（`flex flex-col items-center justify-center`），**无卡片、无边框、无底色**
  —— 「印在面上」的关键是**不加容器**，只用低浓度图标与文字。
* 浓度：图标与文字用 `--fg-3`；静态整体不透明度 ≈ 0.55；载入时在 0.34 ↔ 0.62 之间呼吸。
* 图标 64px / `stroke-width={1}`（细笔画像蚀刻）；文字 `text-fs-2`；两者间距 12px。
* 无障碍：载入态 `role="status"` `aria-live="polite"`；装饰性图标 `aria-hidden`；
  只有「重试」按钮吃点击（动画层 `pointer-events-none`）。
* 同一组件承担三个语义（`animate` 控制动效、`tone` 控制颜色），**不按状态拆组件**。

### C. 动画（写在 `motion.css`，带口径说明）

```css
/* 状态指示类动效：与上面的「反馈类 ≤150ms」是两套口径 ——
   反馈要立刻，状态指示要「看得出在忙但不抢眼」，所以是秒级的慢循环。
   仍然只动 opacity / transform（不触发布局与重绘）。 */
@keyframes rb-watermark-breathe { 0%,100% { opacity:.34 } 50% { opacity:.62 } }
@keyframes rb-watermark-sheen   { 0% { transform:translateX(-140%) } 55%,100% { transform:translateX(140%) } }
@keyframes rb-watermark-in      { to { opacity:1 } }   /* 延迟出现用 */

.rb-watermark        { animation: rb-watermark-in 0s linear 120ms forwards,
                                   rb-watermark-breathe 3.6s ease-in-out 120ms infinite; }
.rb-watermark-sheen  { animation: rb-watermark-sheen 5.2s ease-in-out infinite; }
@media (prefers-reduced-motion: reduce) { .rb-watermark, .rb-watermark-sheen { animation: none } }
```

* **延迟 120ms 出现**（`rb-watermark-in`）：命中缓存/小目录时整段跳过，**不闪一下**。
  这是选了「等缓存」之后必要的收尾 —— 否则二次打开同目录会闪一帧水印。
* **高光带**：一条低浓度中性渐变（`color-mix(in oklab, var(--fg-1) 12%, transparent)`），
  只被水印块自身裁住（`overflow:hidden`），靠 `transform: translateX` 扫过 ——
  看起来就是「光缓慢扫过印痕」，而不是一条亮带横穿整个网格。
* 呼吸 3.6 s、扫光 5.2 s（一轮里带停歇）；两者都不使用主色/辅色 —— 不抢眼。
* 具体浓度与时长是**可调参数**，真机观感由人类定（`AGENTS.md` §2.8）。

### D. 两个网格接线

**导入网格**（`PhotoGrid.tsx`）：删掉局部 `Hint`，四处状态全部走 `StateWatermark`：

| 状态 | 图标 | 文案 key | 动效 |
| --- | --- | --- | --- |
| 未选目录 | `IconFolderOpen` | `grid.pick_dir` | 静态 |
| 准备中（扫描+缓存） | `IconPhoto` | `grid.loading_dir`（新） | **animate** |
| 目录为空 | `IconPhotoOff` | `grid.empty_dir` | 静态 |
| 读不了 | `IconAlertTriangle` | `grid.load_error`（带 `{message}`） | 静态 + `tone="error"` + 重试 |

**浏览网格**（`BrowseGrid.tsx`）—— 按优先级判断，命中即渲染水印、不渲染 `VirtualGrid`：

| 条件 | 图标 | 文案 key |
| --- | --- | --- |
| `repositoryId() === null` | `IconAlbumOff` | `browse.noRepository`（复用现有） |
| `error() !== null && total() === 0` | `IconAlertTriangle` | `browse.load_error`（新）+ 重试 |
| `loading() && total() === 0` | `IconPhoto` | `browse.loading`（新）· **animate** |
| `!loading() && error() === null && total() === 0` | `IconPhotoOff` | `browse.empty_lib`（新） |

> 二义性检查：`ensureRange` 翻页**不置** `loading()`，所以滚动补页时不会突然变成水印；
> 换库/换范围走 `reload()` 才置 —— 那正是该显示载入的时刻。

### E. i18n 普查（前端约 15 处 —— 含 6 个超时动词，逐条有归属）

**新增 key（中英各一条）**：

| key | 中文 | 用途 |
| --- | --- | --- |
| `grid.loading_dir` | 正在读取目录… | 导入网格载入水印 |
| `browse.loading` | 正在加载库… | 浏览网格载入水印 |
| `browse.empty_lib` | 这个库里还没有照片 | 浏览网格空态 |
| `browse.load_error` | 读不了这个库：{message} | 浏览网格错误态 |
| `common.timeout` | {what}没有在 {seconds} 秒内回应（后端可能已经挂了） | 超时句（`{what}` 是动作） |
| `common.desktop_only` | 这个功能要在桌面应用里运行（当前是浏览器预览） | 浏览器降级路径 |
| `import.no_backend` | 当前环境没有导入后端（开发预览里只有界面） | 浏览器降级路径 |
| `import.timeout.command` / `.start` / `.subscribe` / `.status` / `.export` / `.precheck` | 导入命令 / 启动导入 / 订阅导入进度 / 读导入状态 / 导出错误清单 / 空间预检 | 超时句里的 `{what}` |
| `grid.timeout.meta` | 读取照片信息 | 等头部缓存的时限（§4.A） |

**改法（逐处）**：

1. `src/features/browse/rows.ts` —— `UNKNOWN_LABEL = "未知时间"` 是**直接显示**的分组标题。
   改成结构化标记：`BrowseGroupBoundary` / `BrowseGroupRow` 加 `unknown: boolean`（`label` 仍存日键），
   视图渲染 `row.unknown ? t("grid.unknown_time") : row.label`；删掉 `UNKNOWN_LABEL` 导出
   （同步 `features/browse/index.ts` 与 `rows.test.ts`）。口径与**导入网格既有做法一致**（那边就是 `unknown` 标志）。
2. `src/lib/timeout.ts` —— `lib` 不能 import i18n（架构检查器盯着）。所以改**契约**：
   `withTimeout(task, ms, message)` 的第三个参数改成**已经翻好的整句**（`TimeoutError.what` → `message`），
   调用方用 `t("common.timeout", { what: t(key), seconds: Math.round(ms/1000) })` 拼。
   调用点只有两处：`features/import/store.ts`（5 处）与 `workspaces/import/ImportWorkspace.tsx`（1 处）。
   在导入 store 里加一个 3 行的局部 `guard(ms, key)` 助手，避免 6 处重复拼。
3. `src/features/repositories/state.ts` —— `` `未找到该库（已试过 ${n} 处已登记路径）` ``。
   **store 不引 i18n**（保持与今天一致：没有任何 store/state 模块引 i18n）→
   把 `remountErrors` 的值从 `string` 改成
   `{ kind: "not_found"; tried: number } | { kind: "message"; text: string }`，
   在 `RepositoryList.tsx` 渲染时翻译（`repo.remount_failed` 这个 key **已经存在**、目前无人用）。
   这个值会**穿过两层透传**（`workspaces/import/store.ts` → `ImportWorkspace.tsx` → `RepositoryList.tsx`），
   类型签名要一起改（一共 4 个文件 + 1 个测试）。
4. `src/api/db.ts`（2 处）与 `src/api/import.ts`（4 处）—— 换成 `t("common.desktop_only")`
   （`api` 层允许 import i18n）。
5. `src/features/import/store.ts:206` —— 换成 `t("import.no_backend")`。
6. `src/lib/marking-state.ts` 的 `COLOR_LABELS`（红/黄/绿/蓝/紫）——
   **整个模块目前无人引用、也没有配套测试**（`triState` / `ratingDisplay` 等一个调用者都没有）。
   本轮**只删掉 `COLOR_LABELS`**（死代码 + 唯一的中文），其余保留给 W2；
   W2 接线时色名走 `t("grid.color.red")` 一类 key。**不新增当前用不到的 key**。
7. `index.html` 的 `<noscript>需要启用 JavaScript…</noscript>` —— 静态 HTML，没有 i18n 运行时
   （Tauri 里 JS 永远可用，这页只在浏览器禁 JS 时出现）。**保留中文**，在守门脚本里显式豁免并注明理由。
8. `src/dev/**`、`scripts/**`、`src/lib/release-plan.ts`、`console.error` 日志 —— **明确排除**
   （开发期陈列室 / CLI 输出 / 开发者诊断），在守门脚本的豁免表里写清理由。

### F. 守门：`pnpm lint:i18n`

新增 `scripts/check-i18n.mjs`（与 `check-hardcoded-colors.mjs` 同一套写法：正则 + 剥离注释 + 逐条报错）：

* 扫描 `src/**/*.{ts,tsx}`；
* 豁免：`src/i18n/**`、`src/dev/**`、`**/*.test.ts`、以及**逐文件豁免表**（每条带理由，见 §4.E 第 7/8 条）；
* 命中：字符串/模板串/JSX 文本里的 **CJK 字符**（项目工作语言是中文，CJK 是最实际的漏译信号）；
* 允许：同一行写 `// i18n-exempt: <理由>` 显式豁免；
* 输出 `文件:行: 片段`，有命中就 `exit 1`。
* 接进 `package.json` 的 `lint:i18n`，并把这条加进 `AGENTS.md` §5.3 的质量门列表。

> 为什么不查英文硬编码：误报太多（`px`、`RAW`、`ISO`、路径示例…），信噪比撑不起一道门。

### G. 顺手修掉的真 bug（与本次区域相邻）

`src/features/browse/BrowsePanels.tsx` 与 `BrowseGrid.tsx` 用了 **`text-2` / `text-3` / `text-4`
这些不存在的类**（令牌是 `--text-fs-*`，对应 `text-fs-*`）。
已在 `dist/assets/index-*.css` 里核实：`.text-3` 等**没有生成任何 CSS** ——
浏览左右列正文一直在用默认 15px，而不是设计里的 13/14px。改为 `text-fs-2/3/4`。

---

## 5. Files to modify

| 文件 | 改什么 |
| --- | --- |
| `src/components/ui/StateWatermark.tsx` | **新增**：水印组件（载入/空态/错误/提示） |
| `src/styles/motion.css` | **新增** `rb-watermark-*` 三个动画 + reduced-motion 兜底 + 口径注释 |
| `src/features/photo-grid/PhotoGrid.tsx` | 删局部 `Hint`，四处状态改走水印组件 |
| `src/features/photo-grid/store.ts` | `load()` 等 `loadPhotoMeta` 再 `ready`（§4.A）+ 给 `dirMetaEnsure` 加时限 |
| `src/features/photo-grid/store.test.ts` | 新增载入门语义的用例 |
| `src/features/browse/BrowseGrid.tsx` | 接四处状态水印 + 修 `text-3` |
| `src/features/browse/BrowsePanels.tsx` | 修 `text-2/3/4` 假类 |
| `src/features/browse/rows.ts` + `rows.test.ts` + `index.ts` | `UNKNOWN_LABEL` → `unknown` 标志 |
| `src/features/repositories/state.ts`（+ `state.test.ts`）、`RepositoryList.tsx`、`workspaces/import/store.ts`、`ImportWorkspace.tsx` | 重挂载失败文案结构化 + 渲染时翻译（值要穿过两层透传） |
| `src/api/db.ts` / `src/api/import.ts` | `common.desktop_only` |
| `src/lib/timeout.ts`（+ `timeout.test.ts`） | 第三个参数改成整句；文档与用例同步 |
| `src/features/import/store.ts` / `workspaces/import/ImportWorkspace.tsx` | 超时句拼装；`import.no_backend` |
| `src/lib/marking-state.ts` | 删 `COLOR_LABELS` |
| `src/i18n/zh-CN.ts` / `en-US.ts` | 新增 §4.E 的 key |
| `scripts/check-i18n.mjs`（新）+ `package.json` | `lint:i18n` |
| `AGENTS.md` §5.3 / `DESIGN.md` §12（新小节）/ `design/main.md` §9.5 / `FUTURE.md` G18 | 文档登记（见 §6） |
| `implementations/2026-09-17_*.md`（新） | 实施记录（精确时间，§2.7 纪律） |

---

## 6. 文档登记（本轮要写的四处）

1. `DESIGN.md` §12 新增小节「**网格状态水印**」：组件语义（载入/空态/错误/提示）、
   图标 64px stroke 1、浓度、动画时长、`prefers-reduced-motion`，
   以及 **motion.css 的两类时长口径**（反馈 ≤150ms / 状态指示为秒级慢循环）。
2. `design/main.md` §9.5（有意不做、需记一笔）：补一行「网格载入态 / 空态水印：画布无帧，
   本轮按人类指示**跳过 Pencil**；将来补也只是**新增帧**」。
3. `FUTURE.md` **G18**：后端（Rust）错误文案的 i18n —— 现状（`error.rs` 等直接给中文句）、
   做法（错误码 + 参数走 IPC，前端 `error.*` 映射）、触发条件（真要做多语言分发时）。
4. `AGENTS.md` §5.3：质量门列表加 `pnpm lint:i18n`。

---

## 7. Steps

- [x] `StateWatermark` + `motion.css` 动画（先把观感做出来，好让人类尽早看到）
- [x] `PhotoGrid` 四处状态接线
- [x] `store.ts` 载入门 + 时限 + 单测（5 个新用例：等 meta 期间保持 loading / ready 时比例已就位 /
      meta 失败仍 ready / **meta 永不返回→超时后仍 ready** / 换目录后迟到的 meta 不污染新目录）
- [x] `BrowseGrid` 四处状态接线（含 `text-3` 修复）+ `BrowsePanels` 假类修复
- [x] `browse/rows.ts` 的 `unknown` 结构化 + 测试同步
- [x] `timeout.ts` 契约调整 + 调用方改 key + 测试同步
- [x] `repositories/state.ts` 结构化错误 + `RepositoryList` 翻译 + 测试同步
- [x] `api/db.ts` / `api/import.ts` / `import/store.ts` 文案入包
- [x] 语言包补 key（中英同补，`locale-parity` 自动守）
- [x] `scripts/check-i18n.mjs` + `package.json` + 跑一遍（应 0 命中）
- [x] 四处文档登记（§6）+ `implementations/` 实施记录
- [x] 冒烟：`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`
- [x] 交付给人类做观感验收（§8）

## 8. Verification

**Agent 只做冒烟**（`AGENTS.md` §2.8）：

* `pnpm typecheck` / `pnpm test`（含新用例）/ `pnpm lint:colors` / `pnpm lint:arch` / `pnpm lint:i18n` / `pnpm build` 全绿；
* `pnpm smoke:ui http://localhost:1420/dev/kitchen-sink` 起得来、控制台无报错（改动不碰外壳，只作回归）；
* 挑一个**小目录**与一个**大目录**各开一次，确认：水印出现 → 一次性铺出照片（日志/网络面板确认
  `dirMetaEnsure` 在 `scanSourceDir` 之后、`ready` 之前完成）；
* 切中/英两次，确认新文案没有半句中文残留（`locale-parity` + 逐键替换的静态保证）。

**归人类的 E2E**（不由 Agent 代做、不声称验证过）：

* 水印「不抢眼 / 不显得卡」的观感；呼吸与扫光的浓度、速度是否合适（真机、双主题、两档密度）；
* 空态（目录空 / 库空）看着是否合适；
* 打开一个真实大目录（几百上千张）时的等待体感是否可接受 —— 这是 §2 问题 1 那个取舍的验收点。

---

## 9. 明确不做（写下来免得被当成遗漏）

* **不动任何 `.pen` 文件**（人类 2026-09-17 指示）；将来要补也只新增帧。
* 不把 Rust 侧错误文案改成 i18n（体量另算，登记为 `FUTURE.md` G18）。
* 不把 `src/dev/**`、CLI 脚本、`console.*` 诊断日志纳入语言包。
* 不做 tile 级别的载入动画改版（`Tile` 现有的占位脉冲保持不动）。
* 不给「目录树 / 最近 / 已选目录 / 信息栏」等其它面板换空态（人类选了「只两个照片网格」）。
* 不新增当前用不到的 i18n key（例如色名，等 W2 真接线时再加）。
