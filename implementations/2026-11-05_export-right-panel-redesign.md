# 导出右栏改版：段标题、预设命名、字段行紧凑化

> 完成时间：**2026-11-05 04:12:30 +08:00**
> 需求（崔总原话）：`修改预设`/`新建预设` 改名 `当前预设`；`变更`/`新增` 统一 `保存` 并始终辅色；
> `预设列表` 与 `当前预设` 要有标题感（加粗 + 左侧主色竖条）；名称与 input 同一行、格式与四选项同一行、
> 「导出文件已经存在」更名 `文件同名` 与三选项同一行；界面优化同步到 Pencil。

## 1. 代码改动

| 位置 | 改了什么 |
| --- | --- |
| `src/components/ui/SectionTitle.tsx`（新） | 段标题原语：左侧 3px 主色竖条（`aria-hidden`）+ 加粗 `<h3>` |
| `src/workspaces/export/ExportWorkspace.tsx` | 段标题换成 `SectionTitle`；新增本地 `FieldRow`（字段名列 `w-16` + 控件占余宽，`stretch` 给输入框）；名称/格式/文件同名三行改单行；「保存」按钮 `variant` 恒为 `accent` 且文案 `common.save`；表单纵向 `gap-2 → gap-1.5` |
| `src/i18n/{zh-CN,en-US}.ts` | 新增 `export.currentPreset`；`export.format` 改「格式」；`export.existingFile` 改「文件同名」／"Same file name"；删除 `export.editPreset` / `export.newPreset` / `export.update` / `export.add` |
| `scripts/check-export-boot.mjs` | 断言改为「标题恒为 当前预设、按钮恒为 保存（含 `bg-brand-2`）」，并新增：两段标题 `h3` 有主色竖条且 `font-weight ≥ 600`；名称/格式/文件同名三行「同一行且不溢出」 |

## 2. 关键决策

1. **`当前预设` 是恒定的段名**：原先标题随「名称是否匹配到现有预设」在 `修改预设 / 新建预设` 之间跳，
   用户得先读懂名字才知道自己在改谁；按钮同理，一颗 `保存` 内部处理新建/更新。
2. **辅色恒亮**：保存从「新建=辅色 / 更新=深色」改成**始终辅色**（崔总），避免同一动作两种颜色。
3. **标题竖条做成共享原语**而不是在导出里手写：`AGENTS.md` §12 与 `memory/COMPONENTS.md` §2 已登记，
   其它面板要标题感时直接用它（禁止再手写「色值 + 字重」）。
4. 文案常量集中改语言包，未新增任何界面硬编码（`pnpm lint:i18n` 通过）。

## 3. Pencil 同步（`design/export.pen`）

- 全文档文本改名（25 个文本节点，0 残留）：`修改预设`/`新建预设` → `当前预设`；`变更`/`新增` → `保存`；
  `导出格式` → `格式`；`导出文件已经存在` → `文件同名`；预设列表标题 `导出预设` → `预设列表`。
- 主稿 `HYI2F` 右栏：
  - 「预设列表」「当前预设」两处段标题加 **3px 主色竖条**（`SectionBar`），标题 `font-weight 600`；
  - 删掉与标题重复的 `SectionHint` 与「点选后可在下方修改」那句静态说明（违反 `AGENTS.md` §2.20）；
  - 名称 / 格式 / 文件同名 三处由「标签在上、控件在下」改成**横向单行**（新建 `Row / …` 三个 flex 行容器，
    控件 `fill_container`）；保存按钮 `zxKBB` 填充改 `$brand-2`。
- `design/export.md` §5 同步文字规格（含改版说明与 i18n 主表）。
- **未做**：`States / Export` 系列派生帧（最小窗 / 宽松 / 新建预设表单 / 空态）的**几何**仍保持旧的两行式，
  文案已随全文改名；派生帧的排版等主稿定案后一起刷（一次全部重排的风险高于收益）。

## 4. 验证

- `pnpm typecheck` 通过；`pnpm test` **1420 passed**；`lint:colors` / `lint:arch` / `lint:i18n` 通过；`pnpm build` 通过。
- `pnpm smoke:ui`：**problems: []**（导出右栏挂载正常）。
- `pnpm check:export`：**在本工作树里跑不完** —— 卡在上一轮遗留的「浏览器 → 导出」工作流切换（点 chip 无反应）
  与后面的拖动 ghost 断言；这两处与本轮改动无关（本轮只改右栏排版与文案）。
  绕过那两段单独跑时，本轮新增的四组断言（标题竖条 / 三行单行 / 保存辅色 / 文案）全部通过。
  该红灯已记入遗留，需要单独排查（`scripts/check-export-boot.mjs` 的 clickChip 与 drag 段）。

## 5. 顺带修掉的一个真 bug：`filterTagMenu(null)` 带崩响应式根

排查 `check:export` 的「浏览器→导出 点 chip 没反应」时定位到：`TagFilterMenu` 的 `createMemo`
直接用 `store.tags()`（词典还没拉回来时是 `null`）调 `filterTagMenu`，在那里 `tags.filter` 抛异常 ——
而它发生在 **Solid 的 computation 里**，一次未捕获异常就让整个响应式根停止刷新：
信号照常变、DOM 不再更新（`elementFromPoint` 仍命中 chip、合成 click 也无反应、控制台无 error，
正好对上现场）。

- 修法：`filterTagMenu` 接受 `readonly Tag[] | null`（`null` → 空菜单），并补单测
  「词典还没拉回来（null）时给空菜单，而不是抛异常」。
- 效果：`pnpm check:export` 从「切不回导出工作流」前进到**拖动 ghost** 那一段
  （`ghost centred on the pointer`），说明工作流切换恢复；拖动那段仍是既有红灯。

## 6. 遗留

1. `check:export` 还剩**拖动 ghost** 一段红灯（`ghost centred on the pointer`）。它在本轮之前就存在
   （`specs/export-issue-ordinal.md` §6 已记「check:export 环境性受阻」），需要单独排查
   `scripts/check-export-boot.mjs` 的 drag 段与 `DragPreview` 的定位；不在本轮范围。
2. `States / Export` 派生帧排版未刷（见 §3）。
3. 其它面板（browse 右栏、编辑左列）的段标题仍各自手写 `<h3 class="…">`，后续改动时可顺手换 `SectionTitle`。
