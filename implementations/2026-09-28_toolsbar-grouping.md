# toolsbar 的分组与分隔：删除按钮、锁合并循环键、星距收窄、四个 flow 规整

完成时间：2026-09-28 21:20:00 CST

## 范围

崔总 2026-09-28 口述的一批 toolsbar 规整（四个 flow 的 mid 区域全部按新规矩体检），
外加顺手修的两处 LUT 面板小毛病。

## 行为定案（唯一事实源在 `memory/DESIGN.md` §12.12）

- **新组件 `ToolsSeparator`**（`components/ui/`，全项目唯一一份）：1px 浅竖线 + 两侧各 4px，
  `aria-hidden` 纯装饰。组**内**几乎无间距（`gap-0.5`），组**间**才用它。
- **令牌 `--line-2` 落地**（此前 DESIGN.md 引用但 tokens.css 从未定义，三处现存用法一直靠
  currentColor 兜底）：深 `#3c4149` / 浅 `#ccd1c5`，`@theme` 映射 `--color-line-2`。
- **browse**：`[删除] │ [撤销][重做] │ [筛选] │ [旗标][清空旗标] │ [★★★★★] │ [●×6] │ [赞][踩] │ [标签] │ [锁]`
  - **删除**（新）：接 `Delete` 键同一条命令（`edit.delete` → `browseActions().requestDelete()`，
    含确认弹窗与回收站语义；崔总点名「别光上按钮不绑功能」）。与撤销之间的分隔是**语义性**的
    （不可逆）。没选中时禁用（与命令的 enabled 同源）。
  - **星距收窄**：星标容器 `gap-0` + 按钮自身 `p-0.5` → 星与星 4px，与色标（`gap-1` 裸点）一致。
  - **锁合并为一个循环键**（`ToggleBlock` 加 `tone`：一级=辅色底、二级=主色底；内容「锁/一级锁/
    二级锁」随档位变）。循环：未锁 → 一级 → 二级 → 未锁；筛选态同键：一级 → 二级 →
    **不按锁筛选**（不是「筛未锁」；查询条件不追求覆盖全部组合——崔总原话）。
    档位与循环目标在 `lib/marking-state.ts`（`lockStep`/`lockFilterStep`/`lockCycleTarget`，3 条单测）；
    「解除」走既有 toggle 语义（对二级再发二级 = 清），工具条与命令面板同一套实现。
- **edit**：`[编辑源] │ [撤销][重做] │ [自动调整] │ [裁切/旋转/对比]`。
- **export**：`[送入队列][移出队列] │ [取消选中] │ [重置所有队列]`。
- **import**：单控件（批量排除）自成一组，无需分隔——记录在案即可。
- 旧的 `w-3`/`w-5` 空隙 spacer 全部删除（browse 工具条内）。

## 顺手修的两处（崔总报的）

1. **LUT「应用 LUT」开关**：原生 `<input type=checkbox>` → 全系统统一的 `Switch`
   （`components/ui/Form.tsx`）。崔总提过多次。
2. **LUT 选中勾是竖椭圆**：原实现是文本 `✓` 靠行高（全局 1.45）撑高、宽只有字宽 + `px-1`。
   改为定尺寸 `size-4.5` 圆 + `IconCheck 12`（与全库图标口径一致）。

## 涉及文件

- 新：`src/components/ui/ToolsSeparator.tsx`
- `src/styles/tokens.css`（`--line-2` 两主题 + `@theme` 映射）
- `src/components/ui/ToggleBlock.tsx`（`tone`）、`src/features/browse/BrowseToolbar.tsx`（重排 + 删除 + 锁）、
  `src/App.tsx`（`onDelete` 接线 + 顺手把一处 `throw Error` 改 `throw new Error`）、
  `src/features/editor/toolbar.tsx`、`src/workspaces/export/Toolbar.tsx`（分组）
- `src/features/editor/lut-panel.tsx`（Switch + 圆勾）
- 文档：`memory/DESIGN.md`（§12.12 + 变更记录）、`memory/FUNCTION-BROWSE.md` §3.2、`design/browse.md` §2.1
- 冒烟：`scripts/check-browse-boot.mjs`（新增锁循环三态断言：辅色底/主色底/解除）

## 验证（Agent 冒烟）

- `pnpm typecheck` / `pnpm test`（1078 项）/ `lint:colors`（`--line-2` 只在 tokens.css ✓）/
  `lint:arch` / `lint:i18n`：通过。
- `pnpm check:browse`（含新锁断言，真实 CDP 点击三连）/ `check:flowbar`（38）/ `check:export` /
  `check:import` / `smoke:ui`：通过。
- ⚠️ 过程排查记录：一次 `check:browse` 大面积失败 + 一次 `check:flowbar`「translated labels en-US」
  失败，**都是开发服务器的问题不是代码**——之前一次 `vite --force` 被中途杀掉，留下**半截的
  `node_modules/.vite` 依赖缓存**（Ark UI 的响应式被静默破坏：FlowBar 文案不随 locale 变）。
  `rm -rf node_modules/.vite` 后干净重启，全部复绿。**教训：杀过优化中的 vite 后要清缓存再起。**

## 遗留

- 视觉（分隔线的深浅、辅色/主色锁底的实际观感）归崔总真机；`--line-2` 的三个**旧**引用
  （Toast 中性边框、色标「无色」空心圈、spike 页虚线框）从 currentColor 兜底变成真令牌色，
  观感会有一点变化，请一并看。
- `design/*.pen` 未同步（Pencil 未连接，同前几轮）。
