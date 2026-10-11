完成时间：2026-10-10 00:57:31 +0800

# 编辑预设面板调整 + 全项目「渐隐截断」收尾纪律

## 范围与依据

崔总 2026-10-10 会话口述（编辑预设面板 6 条 + 标签页 2 条）：

1. 应用按钮不常亮；禁用时悬浮提示「选中预设执行应用」；
2. 目录行：名称超长渐隐（不用省略号）、去掉计数、最右按钮位固定占宽；
3. 空目录可删除；**目录有内容时行尾换成改名**，弹窗打开即聚焦并全选；
4. 新建预设默认**不勾选镜头**（跨照片会把镜头 profile 套错 → 画面变形）；
5. 疑似 bug：创建预设弹窗里切 Lens chip 会不会影响当前照片的镜头校正 —— 排查并保证无联系；
6. 英文 `Color management` 太长导致 tab 错位：精简单词 + tab 超长改为边缘极短渐隐；
7. 「名称超长渐隐、不要三点省略」**推广到全项目**，并在 `AGENTS.md` 落纪律。

## 逐项实现

### 1. 应用按钮（`preset-panel.tsx`）

- `variant="primary"` → `variant="ghost"`，与「新建目录 / 新建预设」同款，不再常亮。
- 禁用时提示：`Tooltip` 内容改为 `canApply() ? 应用 : 选中预设执行应用`（新 key
  `editor.preset.applyDisabled`）。禁用按钮带 `pointer-events-none`（Button 的禁用纪律），
  所以触发器挂在外层 `<span class="inline-flex">` 上，悬停才收得到。

### 2. 目录行（`preset-panel.tsx`）

- 名称：`truncate` → `overflow-hidden whitespace-nowrap fade-x-end`（右缘渐隐）。
- 去掉行尾计数 `<span>`（`editor.preset.count` 两个语言包一并删除）。
- 最右动作位固定占宽（`--remove-hit`）：**默认目录留空占位**、空目录 = `EasyDestroyButton`、
  有内容 = 改名按钮（`IconButton` + `IconPencil`；`Button.tsx` 新增 `size="hit"`，
  与 `RemoveButton` 同一命中尺寸）。三个状态同一横坐标，目录之间不跳。
- 拖动中动作位清空（不闪现按钮，与原先删除按钮的 `!dragging()` 口径一致）。

### 3. 目录改名（新增一条写链路）

- 存储：`crates/raybend/src/store/presets.rs`
  - `rename_directory(conn, id, name)`：`default` 拒绝、空/超 40 字/控制字符拒绝、
    与其它目录重名（`to_lowercase` 口径，**排除自己**）拒绝、不存在拒绝；只改 `name`，
    不动 `sort_order` / 预设关联 / 预设行。同名（含仅大小写变化）视为合法 no-op。
  - 抽了 `directory(conn, id) -> Option<PresetDirectory>`；`directory_name_taken` 增加
    `except_id` 参数（新建传 `None`）。
- IPC：`src-tauri/src/presets.rs::preset_rename_directory`（返回最新整库，模式同其它五个命令），
  注册进 `src-tauri/src/lib.rs`；前端 `src/api/presets.ts::renamePresetDirectory`。
- 接线：`EditorWorkspace.renamePresetDirectoryW`（同一条 `presetCall` 串行链 + 整库回写
  + `PresetMutationOutcome`），经 `EditorPanels` 传到面板。
- 弹窗：与新建同形；前端先过校验/重名（排除自己），名字没变直接关窗；后端错误原样贴出。
  **打开即聚焦并全选原名** —— 新工具 `lib/dom-focus.ts::selectAllOnMount`（ref 回调，
  未挂载则等一帧），并顺手接到既有改名入口：定稿改名、机型基础曲线改名、定稿另存（预填建议名）。
- 类型：`PresetCreateOutcome` 更名 `PresetMutationOutcome`（新建 / 改名 / 移动共用同一形状）。

### 4. 新建预设默认不勾选镜头（`lib/presets.ts`）

- `DEFAULT_PRESET_GROUPS = PRESET_GROUPS - { colorManagement, lens }`（其余五组仍默认勾选）。
- 文案 `editor.preset.includeGroups` 去掉「默认全选 / all by default」的括号说明；
  弹窗注释与 `specs/editor-presets.md` §1/§5.3、`design/editor.md` §3.10 同步。

### 5. 「切 Lens chip 影响当前照片」疑似 bug：排查结论 = **无数据通路**

逐条核过从 chip 点击到渲染的每一条边：

| 环节 | 事实 |
| --- | --- |
| chip 点击 | 只写面板本地 `presetGroups` 信号，不碰 store |
| `store.presetSnapshot(groups)` | **纯读**（params / curves / nrMethod / lensSides / colorState / lut），不写入、不抬 rev |
| `createPresetW` | 快照 → IPC `preset_create`（只写 DB）→ `setPresetLibrary`（只写预设库状态）→ `selectPreset` |
| 镜头状态写入点 | 只有 LensPanel（选配置 / 开关）、`applyPresetSnapshot`、`autoAdjust`、`loadDevelop`/reset —— 都不在弹窗路径上 |
| 渲染参数下发 | 依赖 `store.developPayload()`（develop 信号）；弹窗改不了它们 |

结论：创建预设**只记录**当时的 profile / enabled，不会改当前照片；弹窗打开时照片只会被遮罩压暗。
最可能被看到的「变形」是：把**含镜头的预设应用到了别的照片 / 别的焦段**（预设里的 profile 按拍摄焦段解析）
—— 这正是第 4 条默认不勾选镜头要防的事。为避免将来回归，`store.test.ts` 新增
`presetSnapshot 是纯读：勾不勾 Lens 都不动当前照片状态`（rev / 载荷 / 镜头侧逐项断言 + 快照里确实记录了
profile/enabled）。**若崔总仍能复现，请给复现步骤**：这是纯读路径，需要现场证据才能继续定位。

### 6. 标签页：渐隐截断 + 英文精简

- `SegmentedControl`：item 去 `shrink-0` 加 `min-w-0`（超长时允许收缩），`ItemControl` 加 `min-w-0`，
  `ItemText` 用 `fade-x-end fade-x-end-sm fade-x-end-fit`（8px 渐隐 + padding 让位法，
  不破坏 Ark 指示块测量出的 item 宽度）。
- `en-US` 的 `editor.colorManagement.title`：`Color management` → **`Color mgmt`**（mgmt 4 字母）；
  中文保持「色彩管理」。该 key 只用于 tab 与预设 chips，改短不影响面板内标题。

### 7. 全项目收尾纪律 + 存量收敛

- `src/styles/fade.css`：`.fade-x-end`（20px）、`.fade-x-end-sm`（8px）、
  新增 `.fade-x-end-fit`（**内容宽度盒子**配套：让出渐隐长度 + 负外边距抵回布局）。
  文件头补了三类用法与边界（自带底色不能直接上 mask；居中文本左端仍硬裁）。
- `AGENTS.md` §2 新增 **22. 长文本收尾一律渐隐，不用省略号**（工具、用法边界、存量按模块收敛）；
  `memory/DESIGN.md` 新增 §7.3 + §10.1 的「改名弹窗输入框聚焦并全选」注记。
- 存量 `truncate` → 渐隐，**本轮收敛的**（全部全宽盒子或 `fade-x-end-fit`）：
  `Menu` / `Panel` / `PathText` / `RepositoryCard`×2 / `Tile`(TileName) / `TreeNode` /
  `TilesControlBar`×3 / `ViewerControls` / `AiDialog`×2 / `BrowsePanels`×2 / `OrganizationPanel`×3 /
  `TagDialog` / `CommandPalette` / `ShortcutSettingsDialog` / `SliderRow` / `lut-panel`×2 /
  `panels`(预设列表 / 定稿行 / 裁剪比例) / `ImportProgressDialog`×2 / `PhotoGrid`(日组头) /
  `RepositoryFooter` / `RepositoryList` / `AboutDialog`×3 / `GlobalSettingsDialog` /
  `ExternalEditorDialog`×2 / `ExportWorkspace`(预设行×2) / `preset-panel`×2。
- **有意跳过（记录在案）**：
  - `Tile.tsx` 的 `minimal-name` 条、`ExportWorkspace` 的选中 chip —— 元素**自带底色**，
    mask 会把底色一起淡掉（要改成「内部包一层文字 span」）；
  - `ExternalEditorDialog` 的应用名 / 「添加应用」格 —— 居中文本，渐隐只在一侧不对称；
  - `ExifStrip` 注释里提到的 `truncate` 只是说明文字，非实际类名。

## 关键决策与理由

- **目录「有内容 = 改名」而不是「仍可删除」**：删除非空目录会连带预设，改名是唯一安全的原位操作；
  空目录才给删除。后端 `delete_directory` 的「非空拒绝」兜底保持不变。
- **固定动作位用 `--remove-hit`**：与 `RemoveButton` 同一命中尺寸，行尾动作按钮成对出现时必须同大小；
  `IconButton` 因此加了 `size="hit"`（而不是在调用点覆盖 `size-6`—— Tailwind 冲突类不可靠）。
- **`fade-x-end-fit` 进 CSS 而不是调用点写 `pr-*/ -mr-*`**：内容宽度盒子的渐隐长度是变量
  （20 / 8px 两档），放在 CSS 里一处收口，调用点只写类名。
- **改名命令返回整库**：与其它五个预设命令同模式，前端回写 `setPresetLibrary` 后选中/折叠态自动收敛（审计 2026-10-04 的既有机制）。
- **命令注册表：不接入**（§2.15 的决定）——目录改名是面板内交互，依赖面板选中 / 弹窗语境，
  与预设面板其它动作（应用 / 新建 / 删除）口径一致；无默认热键问题（未新增命令）。

## 涉及文件

- 预设面板与链路：`src/features/editor/preset-panel.tsx`、`panels.tsx`、`store.ts`（未改）、
  `src/workspaces/editor/EditorWorkspace.tsx`、`src/api/presets.ts`、`src/lib/presets.ts`、
  `src-tauri/src/presets.rs`、`src-tauri/src/lib.rs`、`crates/raybend/src/store/presets.rs`。
- 通用件：`src/components/ui/Button.tsx`（IconButton `hit`）、`SegmentedControl.tsx`、
  `src/lib/dom-focus.ts`（`selectAllOnMount`）、`src/styles/fade.css`。
- 存量收敛：上节清单（约 25 个文件，均为类名替换 / 一处 span 内包）。
- i18n：`src/i18n/zh-CN.ts` / `en-US.ts`（新增 3 key、删 1 key、改 1 条文案、改 1 条英文标题）。
- 测试：`src/lib/presets.test.ts`、`src/features/editor/store.test.ts`、`src/styles/tokens.test.ts`
  （`--fade-x-len` 加入 PROVIDED_ELSEWHERE）、`crates/raybend/src/store/presets.rs`（`rename_directory_guards_and_updates`）。
- 文档：`AGENTS.md`、`memory/DESIGN.md`、`specs/editor-presets.md`、`design/editor.md`。

⚠️ 工作区里另有其它会话的未提交改动（XMP 审计 / `memory/PLAN` 等），本轮**未触碰**，也未 `git checkout` 任何东西。

## 验证（Agent 冒烟）

- `pnpm typecheck`：退出码 0。
- `pnpm test`：**1242 通过 / 0 失败**（新增 1 条 `lib/presets` 默认组断言 + 1 条 store 纯读回归）。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：全部通过。
- `pnpm build`：通过（5.78s）。
- `cargo test --workspace --offline`：全部通过（含新增
  `store::presets::tests::rename_directory_guards_and_updates`）。
- `cargo clippy -p raybend --offline --all-targets`：`store/presets.rs` 无告警（其余 38 条为既有模块告警）。
- `pnpm smoke:ui`（`CHROME_BIN=/usr/bin/google-chrome`，dev server + Chromium）：
  - 主外壳（`http://localhost:1420/`）：`problems` 空；
  - 陈列室（默认 kitchen-sink）：`problems` 空（前两次因首帧导航竞态报
    “Inspected target navigated or closed”，第三次起稳定通过，属脚本/环境抖动）。

## 遗留与待真机（未经人类验证）

- 视觉与手感：目录行固定动作位的宽度观感、tab 渐隐长度（8px）在两种密度 / 两种语言下的观感、
  拖动 ghost 与固定位共存时的观感；
- 存量收敛的约 25 处均为「同类替换」，但布局观感仍需真机扫一遍（尤其 `TilesControlBar`
  的 `fade-x-end-fit` 负外边距与会随密度变的 `--seg-chip-pad-x`）；
- 改名弹窗的「打开即全选」在 Windows WebView2 上的实际手感（Ark 焦点陷阱时序）；
- 上面第 5 条：若「切 Lens chip 照片变形」仍可复现，需崔总给复现步骤。
