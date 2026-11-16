# 导出拖放：队列区也是落点、幽灵贴近指针、出窗即中止

完成时间：2026-09-28 02:16:00 CST

## 范围

崔总口述（本轮）三件事，全部落在导出工作区的「定稿批量拖放」上（界面上就是 mid 上部定稿 tiles →
右侧导出预设卡片那套拖放；代码里它是 `workspaces/export`，不是 `workspaces/editor`）：

1. **下方队列区也能接落**：把选中的定稿拖到 mid 下方的导出队列范围内松手，等效于 toolsbar 的
   「送入队列」——入队到**当前选中的预设**（同一个 store 动作、同一套去重与计数动画）。
2. **进范围要看得出来**：拖进队列范围内时，队列区边框转**主色**（`--brand` 薄荷绿），离开即恢复。
3. **手感两处**：半透明拖动预览离指针太远 → 收成「整叠中心就在指针上」；鼠标移出窗口 → 拖动中止。

拖到预设卡片那条既有行为未改动（辅色边框、显式目标预设、拖后保留选择都照旧）。

## 行为细节

- **落点判定**（`ExportWorkspace.tsx::hitTarget`）：`document.elementFromPoint` 上找
  `[data-export-preset]` → 预设；否则找 `[data-export-area=queue]` → 队列区，但**只在有选中预设时**
  才认（与「送入队列」按钮同一门槛：没有预设就无处可放，此时不亮框也不接落）。
- **队列落点 = 当前选中预设**：`dropIntoQueue` 取 `store.selectedPreset().id` 后走既有的
  `dropIntoPreset`，因此去重（库/相片/定稿哈希）、`preserveSelection`、预设卡片上的计数动画全部复用，
  没有第二套入队实现。
- **高亮范围 = 命中范围**：高亮画在 `[data-export-area=queue]` 那一个元素上（含它的状态条），
  与 `elementFromPoint` 的判据同一个节点 —— 亮起来的地方就是能接住的地方。
  颜色取**主色**（与「当前操作的面板」用的辅色 `--brand-2` 区分开）；同时加 `data-export-drop="queue"`
  作为可测的标记。
- **预览几何**：整叠卡片（132×132，最多四张扇形叠放）中心对齐指针，再偏 `+8px` 作「捏着」的手感；
  张数角标贴到卡片叠右下角。**去掉**了原来的窗口内夹取（`Math.min(x+14, innerWidth-205)`）——
  夹取会在指针出窗时把幽灵钉在窗口边缘，看着像拖动卡死。
- **出窗中止**：`lib/pointer-drag.ts` 新增 `cancelOutsideWindow` 选项，判据是**指针坐标越出视口**
  （`x<0 || y<0 || x>innerWidth || y>innerHeight`，松手同理）；只有导出拖放开启它。
  选坐标而不选 `pointerout` 的理由：按住键拖出窗口时浏览器会隐式捕获指针，越界后坐标照常上报，
  而「离开文档」的事件在松开之前不会来（真的拿 CDP 试过：越界坐标的合成鼠标事件被浏览器直接丢弃，
  页面收不到 —— 所以冒烟里改用同 pointerId 的合成 `pointermove` 走真实代码路径）。

## 涉及文件

- `src/lib/pointer-drag.ts`：`PointerDragOptions.cancelOutsideWindow`、`PointerDragEnv.viewport`
  （视口尺寸可注入，测试不需要真 `window`）；`pointerup` 也补判一次越界（可能没有中间的 move 事件）。
- `src/lib/pointer-drag.test.ts`：+3 条单测（越界即中止且此后不再上报、边界内不误判 / 窗口外松手不算投放 /
  阈值之前就出界只结束不回调）。
- `src/workspaces/export/ExportWorkspace.tsx`：`DragTarget`（preset | queue）、`hitTarget`、
  `presetTarget` / `queueTarget`、`dropIntoQueue`、队列区 `border-brand` + `data-export-drop`、
  预览几何与张数角标、`cancelOutsideWindow:true`。
- `scripts/check-export-boot.mjs`：`drag()` 助手支持三种落点（预设 id / `'queue'` / `'outside'`），
  新增幽灵居中、队列高亮、队列入队计数、出窗中止的断言。
- `design/export.md`：拖放节的几何、队列落点与出窗中止口径。
- `specs/M4-finalization.md`：把那一行验收清单里已经过时的「200×200」改成当前几何，并指向本记录。
- 未改：`store.ts` / `source.ts` / Rust 侧（入队动作本来就够用）、命令注册表。

## 关键决策与理由

- **不新增命令/热键**（AGENTS.md §2.15）：拖放是指针手势，键盘等价物是既有的
  `export.enqueue`（默认键 `Enter`，scope=tiles，与 toolsbar 按钮同一个动作），
  队列落点复用的也是这个动作，故本轮没有新增 command。
- **落点复用 `dropIntoPreset` 而不是调 `exportActions().enqueue()`**：后者按 `chosenVariants()`
  重新取选择，会丢掉拖动开始时捕获的那批（含六张小图之外隐藏的定稿）；前者显式传捕获引用。
- **`cancelOutsideWindow` 做成选项而不是全局默认**：分隔条（`SplitHandle`）用同一个助手但不在本轮范围，
  它的 pointer capture 能在窗口外收到 `pointerup`、本来不会卡住；只给导出拖放开这个行为，
  改动面收在要求之内。
- **队列区高亮用主色**：按崔总原话「队列 tiles 亮起一个主色框」。预设卡片的辅色边框保持原样
  （那是上一轮定的「指向目标 = 辅色」，且卡片选中底本身就是主色底，再用主色边框会糊在一起）。

## 验证（Agent 冒烟）

- `pnpm typecheck`、`pnpm test`：通过（1074 项，含新增 3 条 pointer-drag）。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：通过；`pnpm build`：通过。
- `pnpm check:export`（真实 CDP 鼠标事件）：通过 —— 含幽灵中心距指针 8px（`|Δ|<6`）、幽灵尺寸 132×132/四张、
  拖到队列区时 `data-export-drop=queue` 且类名 `border-brand`（无 `border-surface-layer`）、
  松手后队列数量 = 张数角标值、计数动画到位、高亮随即消失、出窗中止后一条都没入队。
- `pnpm check:flowbar`（38 项）、`pnpm check:browse`：通过（共用分隔条/指针跟踪未受影响）。
- 视觉自查：用一次性探针（`/tmp/shot-drag.mjs`，fixture 直接取 `check-export-boot.mjs` 里那份，
  不复制第二份）拖到队列区截图，见 `/mnt/c/src/tmp/export-drag-queue.png` —— 队列区边框实测
  `rgb(82, 198, 171)`（= `--brand`），幽灵中心 = 指针 + (8, 8)。
- `pnpm debug:win`：成功（两轮；第一轮把队列区「当前面板」辅色边框误改成了主色，改回后重跑）。
  最终产物：主程序 `C:\rb-target\raybend\debug\raybend-desktop.exe` **2026-09-28 02:10:51 CST**、
  dist 02:08:59 CST、worker 02:05:44 CST（worker 本轮无改动），`check:win` 四个引用资源全命中。
  本轮未生成安装包、未 push、未打 tag。
- 过程中一次 `pnpm check:export` 在「空预设」段落超时，是同一时刻 Windows 构建占满 CPU 所致；
  空载重跑通过（那段与本轮改动无关）。

## 遗留

- **归崔总真机确认**：真实照片下的手感（幽灵贴得是否合适、队列区主色框是否够醒目、主色与辅色的分工看着舒不舒服）、
  以及 Windows 真机的「鼠标拖出窗口」是否如预期中止 —— 合成事件只能证明代码路径，证不了 Windows 的指针捕获行为。
  调试版已就绪：`C:\rb-target\raybend\debug\raybend-desktop.exe`（2026-09-28 02:10:51 CST）。
- `design/export.pen` 的「拖放视觉与目标预设」示例仍是旧几何（卡片摆在 200×200 框的右下、无队列落点示例），
  且没有队列高亮样例；Pencil 应用当前没连上（`pencil_get_app_state` 连不上 VS Code），
  待崔总开着 Pencil 时用 MCP 更新并保存。
- 本轮未提交（崔总要求测试通过后再提交）。
