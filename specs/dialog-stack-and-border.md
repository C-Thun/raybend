# 弹窗叠放与边界线（dialog-stack-and-border）

> 2026-10-24 ｜ 崔总定案：所有弹窗统一辅色边框；遮罩由机制保证「只画一次」；
> 2026-10-31 ｜ 边框宽度 1px → **0.5px**（崔总：「淡淡一圈就行」）。
> 弹窗内的提示/菜单、标题栏交互一并收口。

## 1. 目标

1. **无脑调用也不出错**：任何 `<Dialog open>` / `<ConfirmDialog open>` 直接叠在另一个弹窗上，
   不需要调用方声明「第二层不要遮罩」；全应用任何时刻**恰好一层**压暗。
2. **视觉边界**：所有弹窗内容面 0.5px `--brand-2` inner 描边；两层同色面叠放时能一眼分开。
3. **层级可预期**：后开的弹窗永远在视觉与交互的最上层；下层弹窗不可点击、焦点不进入。
4. **收编第二份实现**：命令面板不再直接使用 `ArkDialog`；它和 `Dialog` 共用同一套 Frame。
5. **弹窗内提示可见**：`IconButton` 的 Tooltip（如关闭叉）不再被弹窗自己盖住；
   标题栏菜单在弹窗打开时也能正常出现在弹窗之上。
6. **标题栏可用**：弹窗打开时窗口仍可拖、可关（Ark `persistentElements`）。

## 2. 事实基线（开工前调研）

- 27 个文件、51 处 `<Dialog>/<ConfirmDialog>`；真实结构嵌套 4 个组件、7 个二级弹窗。
- 7 处都手工传了 `scrim={false}`；根层叠放（更新提示、崩溃提示）没有传，会双重压暗。
- Ark/Zag 的 modal 已经负责下层不可点：`body` `pointer-events: none` + 下层 content 内联 `none`
  + 焦点陷阱按栈暂停。**本次不重复实现**。
- 第二层 backdrop 的 z（60）低于第一层 content（80），所以它从来不会压暗第一层弹窗；
  问题只在「背景被压暗两次」与「两层同色面没有边界」。
- `--z-popover` 原为 40，低于弹窗（80）；弹窗内 Tooltip 与标题栏菜单会被盖住。

## 3. 实现契约

### 3.1 `DialogFrame`

`src/components/ui/Dialog.tsx` 导出 `DialogFrame`，是唯一的 Ark 三件套实现：

- `ArkDialog.Root` + `Portal` + `Backdrop` + `Positioner` + `Content`；
- 统一 `onInteractOutside` / `onEscapeKeyDown` → `blurActive()`；
- `persistentElements` 固定包含 `[data-app-titlebar]` 的标题栏；
- Content 基础面：`rounded-ui bg-surface-layer inset-ring-(length:--dialog-ring-w) inset-ring-(--dialog-ring) outline-none`；
- 支持 `positionerClass` / `contentClass` / `contentAttrs`（命令面板用）。

`Dialog` 只负责标题/描述/关闭叉/children/footer；`CommandPalette` 只负责自己的输入与列表。

### 3.2 弹窗栈（`src/lib/dialog-stack.ts`）

- `createDialogLayer(open)` → `{ depth, paintsScrim }`；
- 打开时追加到栈尾，关闭/卸载时出栈；重开也追加到栈尾（= 最上层）；
- `paintsScrim` 当且仅当 `depth === 1`；
- `DialogFrame` 把 `depth` 写进 backdrop / positioner 的 `--dialog-layer`；
- **删除 `DialogProps.scrim` 与 `ConfirmDialogProps.scrim`**，从 API 上杜绝手写错误。

### 3.3 z 阶梯

`src/index.css`：

```css
backdrop:   z-index: calc(var(--z-scrim) + (var(--dialog-layer, 1) - 1) * 2) !important;
positioner: z-index: calc(var(--z-modal) + (var(--dialog-layer, 1) - 1) * 2) !important;
```

深度 1：60 / 80；深度 2：62 / 82；深度 3：64 / 84。后开的永远在上，不再依赖 Portal DOM 顺序。

`--z-popover` 40 → **88**：高于弹窗本体（80）、仍低于 easy-copy tip（90）与 message（100）。
理由：弹窗内 Tooltip/Menu 与标题栏菜单必须开在弹窗之上；背景浮层本来就被 modal 的
`pointer-events: none` 挡住，不存在「背景菜单反过来压弹窗」的正常路径。

### 3.4 边框

`src/styles/tokens.css` 新增基础令牌：

```css
--dialog-ring: var(--brand-2);
--dialog-ring-w: 0.5px;
```

所有 `DialogFrame` 内容面、命令面板内容面、`MigrationGate` 面板使用
`inset-ring-(length:--dialog-ring-w) inset-ring-(--dialog-ring)`。这是「无边线设计」的唯一常驻例外，
理由与浅色主题对比度备注写进 `memory/DESIGN.md` §10.1 #9。

### 3.5 设计稿

- `design/main.pen`：28 个弹窗帧（14 个 `Dialog /` + 6 个 `Global Settings /` + 8 个库设置/导入进度的状态变体）加 `$brand-2` / 0.5px / inner 描边。
- `browse.pen` / `editor.pen` / `export.pen` 的弹窗帧 2026-10-31 已一并同步。

## 4. 验收清单

- [x] `pnpm typecheck`
- [x] `pnpm test`（含新增 `dialog-stack.test.ts`）
- [x] `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`
- [x] `pnpm build`
- [ ] 真机：更新弹窗 → 安装确认，只有一层遮罩、二级有边框、下层点不到、Esc 只关二级
- [ ] 真机：库设置 / 导入取消 / 外部编辑器添加应用，同样检查
- [ ] 真机：弹窗打开时拖标题栏、关窗口、标题栏菜单
- [ ] 真机：弹窗内关闭叉的 Tooltip 显示在弹窗之上
- [ ] 浅色主题：确认 0.5px 辅色边框的实际可见度（是否还够分清两层弹窗）

## 5. 不做

- 不做阴影/毛玻璃；
- 不重做 Ark 的 pointer-events/焦点陷阱；
- 不改弹窗尺寸、padding、按钮顺序；
- 不动 `MigrationGate` 的「无出口」语义；
- XMP / 数据库 / 命令注册表无影响。
