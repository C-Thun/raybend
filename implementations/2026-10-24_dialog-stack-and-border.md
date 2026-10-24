# 弹窗叠放与边界线（dialog-stack-and-border）

> 2026-10-24 19:00:39 ｜ 评审修订 2026-10-25 04:01:18

## 改动范围

把「弹窗叠弹窗」从调用方手工约定改成机制：

- 新增 `DialogFrame`，成为唯一的 Ark 三件套（Root / Backdrop / Positioner / Content）实现；
  命令面板不再直接使用 `ArkDialog`，与 `Dialog` 共用同一套层级、遮罩与边框。
- 新增 `src/lib/dialog-stack.ts`：按打开顺序维护弹窗栈；只有最底层 `paintsScrim`，
  其余层 backdrop 透明；`depth` 写进 `--dialog-layer`，供 `index.css` 计算双层 z。
- 删除 `DialogProps.scrim` / `ConfirmDialogProps.scrim` 以及 7 处调用方 `scrim={false}`，
  从 API 上杜绝「忘记传导致双重压暗」。
- 所有弹窗内容面加 1px `--brand-2` inner 描边（新令牌 `--dialog-ring`）；
  `MigrationGate` 手写面板也补同一条边。
- `--z-popover` 40 → 88：弹窗内 Tooltip/Menu 与标题栏菜单不再被弹窗本体盖住；
  `tokens.test.ts` 的层级断言同步加入 popover。
- 标题栏加 `data-app-titlebar`，并通过 Ark 的 `persistentElements` 在弹窗打开时保持
  `pointer-events: auto`：窗口仍可拖/关，标题栏菜单也开在弹窗之上，且点击标题栏不算「外点」。
- 设计稿：`design/main.pen` 的 28 个弹窗帧（14 个 `Dialog /` + 6 个 `Global Settings /` + 8 个库设置/导入进度的状态变体）在当前 Pencil 会话加
  `$brand-2` / 1px / inner 描边；`design/main.md`、`memory/DESIGN.md`、`memory/PLAN.md` 同步规则与
  其余 `.pen` 未同步的待办。

## 涉及文件

- 新增：`src/lib/dialog-stack.ts`、`src/lib/dialog-stack.test.ts`、
  `specs/dialog-stack-and-border.md`、本记录。
- 核心：`src/components/ui/Dialog.tsx`、`src/features/commands/CommandPalette.tsx`、
  `src/features/migration/MigrationGate.tsx`、`src/shell/TitleBar.tsx`。
- 令牌/层级：`src/styles/tokens.css`、`src/index.css`、`src/styles/tokens.test.ts`。
- 调用点清理：`src/shell/UpdateDialog.tsx`、`src/features/repositories/LibrarySettingsDialog.tsx`、
  `src/features/import/ImportProgressDialog.tsx`、`src/features/external-editor/ExternalEditorDialog.tsx`。
- 文档/设计：`AGENTS.md` §12、`design/main.md`、`memory/DESIGN.md`、`memory/PLAN.md`。

## 关键决策与理由

1. **遮罩由栈决定，而不是调用方传参**：7 处已有 `scrim={false}`，但根层叠放（更新提示、崩溃提示）
   没有传，实际会双重压暗。栈的 `depth === 1` 是唯一判据，任何新弹窗无脑调用即可。
2. **不重复实现「下层不可点」**：Ark/Zag 的 modal 已经把 body 设为 `pointer-events: none`，
   把下层 content 设为内联 `none`，并按栈暂停焦点陷阱；我们只补视觉与 z。
3. **栈深 z 每层 +2**：Portal 容器在组件挂载时就存在，单纯靠 DOM 顺序时，
   「声明早、打开晚」的弹窗（例如崩溃提示晚于设置弹窗出现）可能被压在下面。
   `--dialog-layer` 让后开的永远在上。深度 1–4 的 content 为 80/82/84/86，低于 popover 88。
4. **popover 抬到 88**：Tooltip/Menu 走独立 Portal，原本固定 40，低于弹窗 80；
   弹窗内关闭叉的提示会完全被盖住。抬高后仍低于 easy-copy tip（90）与 message（100）。
   背景浮层本来就被 modal 的 pointer-events 挡住，不存在正常路径下「背景菜单压弹窗」。
5. **标题栏 persistentElements**：Ark 的 modal 会把 titlebar 一起禁掉；加进持久元素后
   窗口拖拽/三键/菜单可用，标题栏菜单也能按 88 的 popover 层显示在弹窗之上。
6. **边框用 `inset-ring` 而不是 `border/outline`**：不改变布局尺寸、不影响现有的
   `outline-none` 焦点抑制、跟随 `rounded-ui` 圆角；项目里已有 `inset-ring` 用法。
7. **边框浓度按崔总「纯辅色」**：深色 `#33373E` 上约 6.24:1；浅色白底约 1.92:1 偏淡，
   但只影响一个令牌，实际观感不够时改 `--dialog-ring` 即可，不动组件。

## 验证

- `pnpm typecheck` ✅
- `pnpm test` ✅（1317 个测试；新增 `dialog-stack.test.ts`：打开/关闭/重开/三层/卸载后
  任何时刻恰好一个 `paintsScrim`）
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n` ✅
- `pnpm build` ✅；产物 CSS 已确认包含 `inset-ring-1`、`--dialog-ring`
  与 `calc(var(--z-*) + (var(--dialog-layer,1) - 1) * 2)`。
- Pencil：`pencil_execute` 读取 28 个弹窗帧（14 个 `Dialog /` + 6 个 `Global Settings /` + 8 个状态变体），写入
  `stroke: $brand-2` / `strokeWidth: 1` / `strokeAlignment: inner`，并截图复核
  `更新 · 下载中` / `更新就绪 · 安装或取消` 无排版破坏。

## 遗留 / 需要人类确认

- **真机 E2E**（按 `specs/dialog-stack-and-border.md` §4）：更新安装确认、库设置、导入取消、
  外部编辑器添加应用；弹窗打开时拖/关窗口与标题栏菜单；弹窗内关闭叉 Tooltip；浅色主题边框可见度。
- `main.pen` 的修改只在当前打开的 Pencil 会话里，**磁盘保存由崔总执行**。
- `browse.pen` / `editor.pen` / `export.pen` 的弹窗帧**尚未加描边**，
  已登记 `memory/PLAN.md` §5 #11 与 `memory/DESIGN.md` 2026-10-24 条目。
- 本次未触碰工作区里 `docs/release.md`、`memory/FUTURE.md` 与 `specs/website-cn-mirror.md`
  等另一会话的改动。

## 评审修订（第三方评审轮，2026-10-25 03:48:04）

评审确认并修了 3 处：

1. **`dialog-stack.ts` 无变化时仍返回新数组**：Solid 的信号按引用比较，
   每次 `createComputed` 都 `[...without]` 会让所有已打开弹窗重渲染 backdrop 与 z。
   全应用 50 多个 `Dialog` 启动时都要跑一次 `open() === false`，等于启动期几十次无意义传播。
   现在无变化时返回原数组（已用 `createSignal` 实验确认 Solid 对同引用不传播）。
2. **`design/main.pen` 漏了 8 个弹窗面**：`库设置 / 在线备用位置`、`库设置 / 离线可定位`、
   `导入进度 / 来源失联`、`导入进度 / 目标库失联` 等状态变体（504/484 宽、含 `Dialog Title`），
   它们与主帧是同一套弹窗的不同状态，已补 `$brand-2` / 1px / inner。合计 28 个弹窗帧。
3. **`memory/DESIGN.md` §8.5 表格行序**：`--z-popover` 改成 88 后仍排在 60 前面，已按值升序重排。
4. **`scripts/check-browse-boot.mjs:4955` 的层级注释已经过期且写反**：旧注释写
   「弹窗……要在标题栏之下」并按 `toast 80 / titlebar 75 / modal 70 / popover 40` 抄了一遍数字，
   与同段断言 `modal > titlebar > scrim` 及当前令牌全部不符。测试**逻辑**本身是相对断言，没有错；
   但注释会误导下一个人。已改成「弹窗高于标题栏、标题栏在遮罩之上」，只列本条真正断言的
   相对关系（viewer < scrim < titlebar < modal；toast > titlebar），完整阶梯指回 `memory/DESIGN.md` §8.5，
   不再重复抄数字。
