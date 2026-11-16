# M1-1 基础组件 16/16 + 通用范式 + 陈列室 + UI 冒烟

完成时间：2026-09-15 19:50:12 CST

---

## 1. 本次改动的范围

`plans/M1.md` §4.4 的最后五项（Step 13 的剩余部分、14、15、18、19）：

1. **11 个基础组件**（`DESIGN.md` §10.1 清单里剩下的）：`SegmentedControl`、`Tooltip`、`Menu`、`Dialog`、
   `PathText`、`Tile`、`TreeNode`、`Panel`、`SplitHandle`、`ScrollBar`、`Badge`
2. **`easy copy` 范式**（§12.1）：悬停细边框 + `点击复制` / `已复制` 状态翻转 + 由下向上动画
3. **`easy destroy` 范式**（§12.2）：默认确认框，**按住 `Shift` 跳过**
4. **陈列室** `src/dev/KitchenSink.tsx`：全状态 × 两主题 × 两密度，仅开发期可达
5. **冒烟**：`typecheck` / `test` / `build` / `lint:colors` / `cargo check` 全绿 + 新增 `pnpm smoke:ui`

期间还补了两个**清单外但正文直接引用**的组件：`Switch`（属性开关，§12.2/§12.4）与
`ToggleBlock`（按下式模式按钮，§12.8），以及 `RemoveButton`（§11.3 的禁行图标 + §8.3 的 ≥22×22 点击区）。

## 2. 涉及文件

**新增组件**（`src/components/ui/`）：
`Badge.tsx`（Badge/CountBadge）、`Dialog.tsx`（Dialog/ConfirmDialog）、`EasyCopy.tsx`、`EasyDestroy.tsx`、
`Menu.tsx`、`Panel.tsx`、`PathText.tsx`、`RemoveButton.tsx`（含禁行图标）、`ScrollBar.tsx`、
`SegmentedControl.tsx`、`SplitHandle.tsx`、`Tile.tsx`、`ToggleBlock.tsx`、`Tooltip.tsx`、`TreeNode.tsx`

**新增逻辑层（带单元测试）**：
`src/lib/tree.ts` / `format.ts` / `easy-destroy.ts` / `clipboard.ts` / `appearance.ts` 以及各自的 `*.test.ts`

**新增样式**：`src/styles/scrollbar.css`（细滚动条）、`src/styles/motion.css`（两个语义化动画）

**新增工具**：`scripts/ui-smoke.mjs`（`pnpm smoke:ui`）、`src/dev/KitchenSink.tsx`

**修改**：`src/styles/tokens.css`（派生令牌 + 四个密度令牌映射）、`src/index.css`（引入顺序）、
`src/i18n/{zh-CN,en-US}.ts`（补 `common.close`）、`src/components/ui/Form.tsx`（勾选框改用密度令牌）、
`src/App.tsx`（开发期入口）、`src/index.tsx`（路由）、`vite.config.ts`、`package.json`、`.gitignore`、
`DESIGN.md`（登记派生令牌）、`plans/M1.md`（勾选）

## 3. 关键决策与理由

### 3.1 令牌：新增的全部是**派生值**，没有引入新的字面色

`--scrim`（浮层遮罩）、`--ring-easy-copy`（§12.1 规定的「品牌主色约 40%」细边框）、
`--focus-ring`、`--scrollbar-thumb*` 一律用 `color-mix()` 从池内颜色派生，
所以「主色后面要微调」时它们自动跟着走（§5.2 的同一思路）。

两主题的遮罩都必须是**压暗**：dark 用最深的 `surface-track` 派生，light 用 `深色`（`fg-1`）派生 ——
浅色主题若用浅面派生会把背景「洗白」，看起来不像弹窗出来。

### 3.2 逐组件的选型（对照 §4.2.2 的表）

| 组件 | 落地方式 | 说明 |
| --- | --- | --- |
| `SegmentedControl` | Ark `SegmentGroup` + `Indicator` | 色块位置与宽度交给 Ark（自己算要处理字体加载、缩放、文案变化后的重测） |
| `Tooltip` / `Menu` / `Dialog` | Ark | 焦点管理、定位、Esc/外点关闭全是「自己写一定漏」的部分 |
| `Panel` / `Tile` / `SplitHandle` / `ScrollBar` / `Badge` / `PathText` / `TreeNode` / `ToggleBlock` / `RemoveButton` | 纯自定义 | 无复杂状态机，自己写更可控 |
| `easy copy` 的载体 | 自建受控 `Tooltip` | Ark 的 `Tooltip` 默认「点击即关」，而范式要求点击后**保持显示并换文案**，所以必须支持受控 `open` |

**Ark Solid 的两个坑（实测，值得记住）**：

1. `Positioner` **不会自动套 `Portal`**。不自己套，气泡/菜单会被父级的 `overflow: hidden` 裁掉 ——
   而面板、列表到处都有这种父级。
2. `asChild` 的参数是**渲染函数**（`(props) => JSX.Element`），不是布尔值。
   触发器类型直接复用 Ark 自己的 `PolymorphicProps<T>["asChild"]`，**不要另写一个等价类型**：
   自己写会把属性退化成 `HTMLAttributes<HTMLElement>`，于是 `<Button {...triggerProps()} />`
   会因为 `ref` 是 `HTMLElement` 而非 `HTMLButtonElement` 编译不过。

### 3.3 禁行图标用 `fill-rule="evenodd"` 真正挖空

「填充圆 + 中间一条横杠」的横杠是**洞**，不是在圆上盖一条背景色的杠 ——
后者在「选中底（主色低透明度）」或「悬停底（辅色底）」上会露出错误颜色。

> 附带说明：lens 的 `hyphenated-svg-attribute` 规则要求写成 `strokeWidth`，**对 SolidJS 是错的**
> （Solid 的 JSX 类型用 `stroke-width`/`fill-rule`，camelCase 会让 `tsc` 直接报错）。以 `tsc` 为准。

### 3.4 滚动条用 CSS 而不是自绘 JS 滚动条

`::-webkit-scrollbar` + 标准 `scrollbar-width/color` 同时写。理由：自绘会丢掉系统的**惯性滚动、
触控板手势、键盘翻页**（相片网格里天天用），还要自己处理 RTL、缩放、内容变化的重测 ——
全是与相片业务无关的坑。代价是 macOS（WKWebView）上观感不同，但那不是第一阶段的目标平台。

### 3.5 `easy destroy` 的判定与呈现分离

判定（要不要弹确认）与待决动作放在 `src/lib/easy-destroy.ts`，可以用纯单元测试覆盖；
组件层只负责「一个禁行按钮 + 一个确认框」。
理由：这条路径**会真的动数据**，「Shift 跳过」一旦写错，人会在东西已经没了之后才发现。
确认按钮用**主色**而不是红色 —— 移除/排除不是破坏性删除（§11.3），
而且 `danger` 令牌尚未定案（`design/main.md` §7 待决项）。

### 3.6 陈列室：开发期可达，且**不能进生产产物**

`import()` 必须留在**模块顶层的 `import.meta.env.DEV` 三元里**。实测两种错法都会让产物里多出
254KB 的 `KitchenSink-*.js`：
把 `lazy(() => import(…))` 提到顶层（调用点仍在），或写进 JSX 内部
（Solid 编译器把 JSX 子节点包成 `get children()` / `$memo()`，`import()` 就落在一个被保留的函数体里）。

### 3.7 `scripts/ui-smoke.mjs`：补上「页面真的画出来了」这一环

`AGENTS.md` §2.8 把 E2E 归人类，但「白屏」既不是编译错误、单元测试也覆盖不到 ——
Windows 版就吃过一次（只看窗口句柄，页面根本没打开）。脚本用 CDP 驱动本机已有的 Chromium
（Playwright 缓存里那个，零新增依赖），断言：页面非空、Ark 部件挂上了、**切主题/密度真的换了
`<html>` 上的 data 属性与令牌值**、控制台无 error/warning。

它**当场抓到一个真 bug**：`@solidjs/router` 1.0 下把 `RouteDefinition[]` 数组喂给 `<Router>`
会让**所有路由静默不匹配** —— 页面空白、控制台一句报错都没有。写成 JSX 子节点形式即正常。

## 4. 验证方式与结果

| 验证 | 命令 | 结果 |
| --- | --- | --- |
| 类型 | `pnpm typecheck` | ✅ 通过 |
| 单元测试 | `pnpm test` | ✅ **109 个用例全过**（约 0.2–2.6s；本次新增 format / tree / easy-destroy / clipboard / appearance 五组） |
| 生产构建 | `pnpm build` | ✅ 通过；`dist/assets/` 只有一个入口 JS（36.4KB gzip 13.8KB），**无陈列室 chunk** |
| 禁硬编码色值 | `pnpm lint:colors` | ✅ 无违规 |
| Rust 侧 | `cargo check --workspace` | ✅ 通过（3m48s，含首次编译依赖） |
| **运行时冒烟** | `pnpm dev` + `pnpm smoke:ui` | ✅ 陈列室渲染出 **2385 字符 / 16 个区段 / 95 个 Ark 部件**；切主题 → `data-theme=light`，切密度 → `data-density=loose` 且 `--bar-title-h` 由 32px 变 40px；`problems: []` |
| 兜底路由 | `pnpm smoke:ui http://localhost:1420/` | ✅ 外壳正常渲染，无控制台错误 |

### 明确**未经人类验证**的部分（`AGENTS.md` §2.8）

- **视觉正确性**：色彩、间距、圆角、对齐、动画观感 —— 一律由人类在真机目视比对 `design/main.pen`。
- **密度切换手感**、字号在真机上的可读性、字体加载观感。
- **WSL 下截图不可用**：headless Chromium 截出来是纯黑（无 GPU，同 `ASSISTANCE.md` A2），
  所以本次**没有**产出可用截图；这也正是为什么脚本只做 DOM 断言、不做视觉判断。
- Windows 侧（WebView2）**完全没跑过**：`src-tauri` 未改动，但新增的前端代码在 WebView2 上的
  表现（尤其 `color-mix` / `::-webkit-scrollbar` / Ark 浮层）需要人类在 Windows 上确认。

## 5. 遗留问题

1. **`danger` 按钮变体仍未实现** —— 需要 `--danger` 令牌，而「窗口关闭键要不要红」是待决项。
   在它定案前不实现是刻意的（否则必然硬编码色值）。
2. **开发期冷缓存慢**：`@tabler/icons-solidjs` 不能进 `optimizeDeps.include`
   （它的 ESM 产物含未编译 JSX，预打包后 import 分析报 “invalid JS syntax” → 该模块 500 → 白屏）。
   代价是冷缓存下第一次打开陈列室要 ~15 秒（6440 个模块请求）。**生产构建不受影响**。
3. **没有 favicon**：浏览器会 404 一次（与页面无关，冒烟脚本已放行）。
4. **`ToggleBlock` 与 `Switch` 不在 §10.1 的 16 个清单里**，但 `DESIGN.md` 正文直接引用了它们。
   若后续要严格对齐清单，需要把它们补进 `DESIGN.md` §10.1。
5. **`Splitter` 已真跑通**（陈列室里的三段式 + 2 个 `ResizeTrigger`），
   但 **`TreeView` 尚未真正用上** —— 现在只有我们自己的 `TreeNode` 展示行。
   `M1-5` 做 `SourceTree` 时要确认 Ark `TreeView` 与它的组合方式（这是组件库选型的两大理由之一）。
6. 陈列室里的中文**没进语言包**（它是开发期工具页，不是产品界面）——
   产品界面的文案仍然零硬编码。
