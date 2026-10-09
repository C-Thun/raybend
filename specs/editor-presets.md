# specs/editor-presets.md — 编辑预设面板（Presets）

> **工作单元**：editor 右栏第 3 组新增「预设」页签（面板 + 存储 + 应用）+ 第 1 组三页签等高。
> **定案**：崔总 2026-09-30 审阅设计稿后「可以」；需求原文 `todos/2026-09-30-editor-presets.md`；
> 设计定案 `design/editor.md` §3.10 / §3.11，画稿 `design/editor.pen`
> （`e4WYe` 面板本体 / `nxMA2` 浮层 / `WCT5O` 等高说明 / 主稿与三派生帧页签条）。
> **纪律**：复用优先（§2.12）——结构照抄 LUT 面板、弹窗照抄 Dialog、删除照抄 EasyDestroy、
> 提示照抄 Tooltip、拖拽复用 `lib/pointer-drag.ts`；不新造第二份。
> **2026-10-07 审计修复**：`implementations/2026-10-04_editor-presets-audit.md` 的发现已逐条修复
> （等高容器、选中收敛、名称上限、库串行、版本契约、错误区分、rev 单次、应用前重读 LUT），
> 记录见 `implementations/2026-10-07_editor-presets-audit-fixes.md`；真机验收仍待崔总。

---

## 1. 范围

**做**：

1. 右栏第 3 组页签条由「曲线」单项变为「曲线 · 预设」两项，内容区随页签切换；**两页签同高**（同容器，切换不伸缩）。
2. 预设面板：工具行（应用 / 新建目录 / 新建预设）+ 一级目录树（可折叠、可拖拽移动预设、悬浮删除）。
3. 新建目录 / 新建预设弹窗（名称输入；预设另按大类勾选，chip 行）。
4. 预设应用：按保存的大类**整体覆盖**当前照片参数；LUT 丢失忽略。
5. 存储：设备级 `app.db`（照 LUT 分类口径——预设是应用级资产，换库可用）。
6. 悬浮信息：列出预设包含的大类（复用 Tooltip）。
7. 右栏第 1 组（总览 / 定稿 / 信息）默认等高（H = 总览在 3:2 画幅下的自然高度，见 §6）。

**不做**：预设重命名（删除后重建即可，YAGNI）；预设排序拖拽（本轮不做）；跨库同步语义；导入/导出预设文件（远期）。

---

## 2. 数据模型（`app.db`，迁移 `app_0008_presets.sql`）

```sql
CREATE TABLE preset_dirs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE presets (
    id TEXT PRIMARY KEY,
    directory_id TEXT NOT NULL REFERENCES preset_dirs(id) ON DELETE RESTRICT,
    name TEXT NOT NULL COLLATE NOCASE,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_presets_dir_name ON presets(directory_id, name);
```

- 迁移登记在 `crates/raybend/src/store/migration.rs`（`include_str!`，`APP_MIGRATIONS` 追加 `app_0008`），**不许另造通道**（§2.16）。
- **`默认` 目录**：`id = "default"`，名称**走语言包**（前端对 `id === "default"` 显示 `t("editor.preset.defaultDir")`，不显示 DB `name`；DB 存 `"Default"` 占位）。后端保证：`preset_library` 初始化时若目录表为空则建它；`preset_delete_directory("default")` 一律拒绝。
- 名称限制（照 `store/luts.rs` 口径）：目录名 `1..=40` chars、去首尾空格、无控制字符；预设名 `1..=80` chars、同规则。预设名在同目录内唯一（大小写不敏感）；**移动**到已有同名的目录时自动加后缀 ` 2`、` 3`…（不打断拖拽流程）。
- 排序：目录 `ORDER BY sort_order, created_at, id`；预设 `ORDER BY created_at, id`（升序）。
- id 生成照 LUT 的 `newLutCategoryId` 模式（前端生成、后端校验 `[A-Za-z0-9-]{1,128}`）；预设 id 前缀 `preset-`。

---

## 3. payload 契约（大类快照）

`payload` 是 JSON 对象（`TEXT` 存字符串；后端只做基本校验——可解析 + **受支持版本**（`version === 1`，或
色彩管理接入后的 `version === 2` 且带 `colorManagement`）——语义归前端）：

```jsonc
{
  "version": 1,
  "tone":   { "exposure": 0.35, "contrast": 0, "highlights": 0, "blacks": 0, "dynamicContrast": 0 },
  "color":  { "temperature": 5400, "saturation": 0, "vibrance": 0 },
  "detail": { "lumaNr": 0, "colorNr": 0, "sharpenAmount": 0, "sharpenRadius": 0, "nrMethod": null },
  "lens":   { "distortion": 0, "vignette": 0, "vignetteRange": 0, "chromaticBlue": 0, "chromatic": 0,
              "profile": null, "enabled": null },
  "curve":  { "rgb": [[0,0],[1,1]], "r": [[0,0],[1,1]], "g": [[0,0],[1,1]], "b": [[0,0],[1,1]] },
  "lut":    { "id": null, "enabled": false }
}
```

**规则**：

1. **保存**：用户勾选的大类才写入 `payload`；**每个勾选的大类写全字段**（含等于默认值/为空的项）——这样「覆盖」语义才完整（勾了影调但某项没改过，应用时该项也回到预设保存时的值）。
2. **应用**：勾选的大类各自**整体覆盖**当前编辑栈；未勾选的大类不动。各组落法：
   - `tone` / `color` / `detail`（4 个数值项）/ `lens`（5 个数值项）：逐项 `setParam(id, 值)`（值为 `payload` 缺项时回退 `PARAM_DEFAULTS[id]`）。
   - `detail.nrMethod` → store 的 `setNrMethod`；`lens.profile` / `lens.enabled` → `setLensProfile` / `setLensEnabled`。
   - `curve`：4 个通道全部 `setCurvePoints`（缺通道按恒等曲线 `[[0,0],[1,1]]`）。
   - `lut`：`id === null` → `setLut(null, false)`（清除）；`id` 非空且**在当前 LUT 库中存在** → `setLut(id, enabled)`；`id` 非空但**不存在或不可用** → **忽略该项**（不动当前 LUT，不报错——崔总：LUT 丢失要有保底）。「存在」以**应用那一刻重读的 LUT 库**为准（工作区先 `lut_library` 刷新再跑应用计划），不用进入编辑器时的旧缓存；重读之后文件又被删的窗口仍走渲染层缺失资源错误契约。
3. **参数表派生**：大类的数值参数 id 列表**从 `features/editor/params.ts` 的 `PARAMS`（`group` 字段）派生**，不手写第二份；`detail`/`lens` 的额外字段（`nrMethod` / `profile` / `enabled`）按本契约写死名字。
4. **纯逻辑落点**：payload 的**构建 / 清洗 / 分组名列表 / 应用合并判定**全部放 `src/lib/presets.ts`（`lib` 不许 import `features`——参数 id 由调用方传入或映射表由 `features/editor/params.ts` 侧轻量派生后注入；实现时二选一，**保持分层检查 `pnpm lint:arch` 通过**）。
5. 不含：基础曲线（`baseCurveProfile` / `baseCurvePoints`）、几何（裁切/旋转）、`asShotK`、`autoAdjust`、`sourceBase`。

**大类清单**（顺序即界面顺序）：`tone` 影调 / `color` 色彩 / `detail` 清晰度 / `lens` 镜头 / `curve` 曲线 / `lut` LUT。（未来可能加「色彩管理」，届时在联合类型与 chips 数组各加一项。）

---

## 4. 后端命令（`src-tauri/src/presets.rs`，照 `src-tauri/src/lut.rs` 模式）

统一返回**最新的整库**（`PresetLibraryDto`），前端拿到即 `setPresetLibrary`（与 LUT 的 `lut_library` 返回模式一致）：

```rust
struct PresetDirectoryDto { id: String, name: String, sort_order: i64, created_at: i64 }
struct PresetRecordDto { id: String, directory_id: String, name: String, payload: serde_json::Value, created_at: i64, updated_at: i64 }
struct PresetLibraryDto { directories: Vec<PresetDirectoryDto>, presets: Vec<PresetRecordDto> }
```

| 命令 | 入参 | 语义 / 错误 |
| --- | --- | --- |
| `preset_library` | — | 读全库；目录表为空则建 `default`。payload 以 **JSON 值**返回（解析失败**或版本不受支持**的行跳过并记一行诊断，不让面板起不来） |
| `preset_create_directory` | `id, name` | 重名（NOCASE）→ Err（前端提示） |
| `preset_delete_directory` | `id` | `id == "default"` → Err；**目录非空 → Err**（后端兜底，前端本就不显示按钮） |
| `preset_create` | `id, directory_id, name, payload` | 目录不存在 → Err；同目录重名 → Err；payload 不可解析或版本不受支持 → Err |
| `preset_delete` | `id` | 直删（无关联副作用） |
| `preset_move` | `id, directory_id` | 目标目录不存在 → Err；**目标同名时自动加 ` 2`/` 3` 后缀**（在同目录内循环找第一个可用名；后缀从基名里让位，**搬完仍在 80 字符上限内**） |

- 注册进 `src-tauri/src/lib.rs` 的 `generate_handler!`（在 `lut::*` 附近）。
- 存储层实现放 `crates/raybend/src/store/presets.rs`（`mod.rs` 挂载），风格照 `store/luts.rs`（`Connection` 函数 + `Error::Unsupported` 校验）。
- 不涉及磁盘文件（纯 DB），全部走 `db.with(...).write/read`。

---

## 5. 前端

### 5.1 文件

| 文件 | 动作 |
| --- | --- |
| `src/lib/presets.ts` | **新建**：类型（`PresetGroup`、`PresetSnapshot`、`PresetRecord`、`PresetDirectory`、`PresetLibrary`）+ 纯函数：`sanitizePresetLibrary`（坏数据丢弃）、`buildPresetSnapshot`（从调用方给的各组值构建）、`mergePresetGroups`（应用合并判定，含 LUT 丢失忽略）、`resolveCreateDirectory`（新建落点：选中目录 → 选中预设所在目录 → `default`）、`uniqueName`（移动去重后缀）、`DEFAULT_DIRECTORY_ID = "default"` |
| `src/lib/presets.test.ts` | **新建**：上述纯函数单测（边界：空库/坏 JSON/缺字段/恒等曲线/同名大小写/LUT 丢失/落点找不到） |
| `src/api/presets.ts` | **新建**：6 个命令的封装（照 `src/api/lut.ts`，`isTauriRuntime()` 守卫） |
| `src/features/editor/preset-panel.tsx` | **新建**：面板组件（§5.3） |
| `src/features/editor/store.ts` | **改**：新增预设状态与 `applyPresetSnapshot`（§5.2） |
| `src/features/editor/panels.tsx` | **改**：第 3 组页签两项 + 切换；第 1 组等高的 min-height（§6） |
| `src/workspaces/editor/EditorWorkspace.tsx` | **改**：`onMount` 拉 `preset_library`；命令接线（新建目录/预设、删除、移动）；`PresetPanel` 的 props 注入 |
| `src/i18n/zh-CN.ts` / `en-US.ts` | **改**：新增 `editor.preset.*`（§5.5），两版同步（`locale-parity.test.ts` 守门） |

### 5.2 store 扩展（`EditorStore`）

```ts
/* 预设（设备级资产，整库放 store：面板切换页签不丢选中/展开态） */
presets: () => readonly PresetRecord[];                 // 由 setPresetLibrary 灌入
presetDirectories: () => readonly PresetDirectory[];
setPresetLibrary: (library: PresetLibrary) => void;
presetSelection: () => PresetSelection;                  // { kind:"directory", id } | { kind:"presets", ids } | null
selectPresetDirectory: (id: string) => void;
selectPreset: (id: string, event: MouseEvent) => void;   // 无 Shift=单选替换；Shift=加/减多选（lib/selection.ts 的 clickMode 口径）
presetExpanded: (id: string) => boolean;                 // 缺省 true（默认展开）
togglePresetDirectory: (id: string) => void;
applyPresetSnapshot: (snapshot: PresetSnapshot, groups: readonly PresetGroup[]) => void;
```

- `applyPresetSnapshot`：在 `batch()` 内按 §3 规则写入（tone/color/detail/lens 数值走 `setParams`；曲线/镜头/LUT/nrMethod 走各自底层 setter），**只 bump 一次 rev**；LUT 存在性用 `lutCategories()` 判定。应用后由工作区 `commitDevelop()` 落库（与 LUT 选择的 `onSelect → setLut + commitDevelop` 同一条路）。
- 换照片/换库不重置选中与展开态（会话内保留）；`resetChrome` 不动它们。

### 5.3 面板组件（`preset-panel.tsx`，结构照 `lut-panel.tsx`）

```text
[曲线][预设]                      ← 页签在 panels.tsx 里（第 3 组）
[▶ 应用] [＋目录] [＋预设]          ← 工具行：3 个图标按钮 + Tooltip
────────────────────────────
▼ 默认 (3)                        ← 目录行：chevron + 名称 + 计数；行尾删除（仅空目录；default 永不）
   □ 柔和胶片                      ← 预设行：图标 + 名称；选中 = bg-state-selected
   □ 街头黑金            ⃠        ← 悬浮行尾删除（EasyDestroyButton，Shift 直删）
   □ 夜景蓝
▶ 人像 (2)
▶ 旅行 (0)               ⃠
```

- **工具行**：`Button variant="ghost"` + Tabler 图标 + `Tooltip`（照 LUT 标题行）：
  - 应用 = `IconPlayerPlay`，**仅 `selection.kind==="presets" && ids.length===1` 时可用**；点击 → `applyPresetSnapshot` + `onCommit`。
  - 新建目录 = `IconFolderPlus`（与 LUT「新建分类」同图标，同一语义同一图标 §2.12）。
  - 新建预设 = `IconBookmarkPlus`。
- **目录行**（`h-row-h`，`bg-surface-bar`，radius-ui）：点行 = 选中目录（`bg-state-selected`）；chevron 点 = 折叠切换；行尾删除按钮 `EasyDestroyButton` **仅当目录为空且非 default** 时渲染；拖着预设靠近时 = 投放高亮（`ring-1 ring-brand`）。
- **预设行**（`h-row-h`）：点行 = 选中（单选替换 / Shift 多选）；选中 = `bg-state-selected`；悬浮 = `bg-state-hover` + 行尾 `EasyDestroyButton`（`opacity-0 group-hover:opacity-100 group-focus-within:opacity-100`，照 LUT tile）；行上 `Tooltip` 显示「包含：影调 · 色彩 · 曲线」（从 payload 已存大类派生，用 ` · ` 连接；空 payload 显示 `t("editor.preset.containsNone")`）。
- **新建目录弹窗**：`Dialog`，标题 + 名称输入 + 取消/确认（取消左确认右 §11.5）；名称先过前端校验（空 / 超长 40 字 / 控制字符）与重名检查，再调后端；重名与其它失败**分开提示**，后端真实原因显示在弹窗内（`PresetCreateOutcome`，审计 2026-10-07）。
- **新建预设弹窗**：`Dialog`，名称输入 + **大类 chips 行**（六个 chip：选中 = `bg-state-selected` + `font-semibold text-fg-1`，未选 = `bg-surface-bar` + `text-fg-2`，照导入 LUT 弹窗的 `CategoryChips` 语言）+ 提示「未勾选的大类不覆盖照片当前值」；**默认全选**；名称为空或一个大类都没选时禁用「保存」（超长 80 字 / 控制字符同目录弹窗处理）。保存后关闭并选中新预设。
- **拖拽**（复用 `lib/pointer-drag.ts::trackPointerDrag`，阈值 + `cancelOutsideWindow: true`）：
  - 起拖（阈值后）：标记 `dragging`；**所有目录显示为折叠**（临时覆盖，不改展开记录）；ghost = `position: fixed` 的行快照（`surface-layer` 底 + 1px `brand` 描边，照画稿）；多选时拖动任一选中行 = 整组移动。
  - move：命中判定对**可见目录行**做 hit test（`getBoundingClientRect`）→ 高亮目标；指针进入滚动容器上/下缘 24px 内且未到顶/底 → `requestAnimationFrame` 步进滚动（step 8px）。
  - 释放：目标目录 ≠ 任一源目录 → 对每个移动项调 `preset_move`（顺序执行，拿到最新库回写）；**还原拖动前的展开记录**，再把目标目录展开；清除 ghost 与高亮。
  - 取消（拖出窗口/`Escape`）：只还原，不动数据。
- **空态**：预设树空时居中提示（图标 + 一句话，照 LUT 空态）。

### 5.4 命令注册表（§2.15 的决定）

**不接入**，理由（写进实施记录）：应用/新建/删除/拖拽都是**面板内交互**，依赖面板选中态与弹窗/指针语境，命令面板无从表达；与 LUT 面板口径一致（其「新建分类/导入」也未登记命令）。若将来需要「应用上一个预设」这类跨界面动作，再按 §2.15 补命令与热键。

### 5.5 i18n（新增 key，中英同步）

```text
editor.preset.title           = 预设                    Presets
editor.preset.apply           = 应用                    Apply
editor.preset.newDirectory    = 新建目录                New directory
editor.preset.newPreset       = 新建预设                New preset
editor.preset.defaultDir      = 默认                    Default
editor.preset.count           = {n}                     {n}          ← 与 editor.lut.count 同形
editor.preset.empty           = 还没有预设              No presets yet
editor.preset.emptyHint       = 在目录下新建预设，保存当前照片的调整   …
editor.preset.emptyDirectory  = 空目录（可删除）         Empty directory (removable)
editor.preset.newDirTitle     = 新建目录                New directory
editor.preset.newPresetTitle  = 新建预设                New preset
editor.preset.name            = 名称                    Name
editor.preset.dirName         = 目录名称                Directory name
editor.preset.namePlaceholder = 预设名称                Preset name
editor.preset.dirPlaceholder  = 目录名称                Directory name
editor.preset.includeGroups   = 包含大类（默认全选）      Include groups (all by default)
editor.preset.keepHint        = 未勾选的大类不覆盖照片当前值   …
editor.preset.contains        = 包含：{groups}           Includes: {groups}
editor.preset.containsNone    = 不含任何大类             No groups
editor.preset.remove          = 删除预设                Delete preset
editor.preset.removeDir       = 删除目录                Delete directory
editor.preset.removeTitle     = 删除预设                Delete preset
editor.preset.removeDirTitle  = 删除目录                Delete directory
editor.preset.removeConfirm   = 确定删除预设“{name}”？删除后不可恢复。    …
editor.preset.removeDirConfirm= 确定删除目录“{name}”？
editor.preset.exists          = 同名已存在              Name already exists
editor.preset.dirExists       = 同名目录已存在           Directory already exists
editor.preset.group.tone      = 影调                    Tone
editor.preset.group.color     = 色彩                    Color
editor.preset.group.detail    = 清晰度                  Detail
editor.preset.group.lens      = 镜头                    Lens
editor.preset.group.curve     = 曲线                    Curve
editor.preset.group.lut       = LUT                     LUT
```

（`editor.group.preset` 页签名复用 `editor.preset.title`。）

---

## 6. 右栏第 1 组三页签等高（崔总 2026-09-30 定案：按「含缩放行」口径）

- **口径**：`定稿` 与 `信息` 的内容区默认高度 = `总览` 在 **3:2 画幅**下的自然高度 =
  预览框（内容宽 ÷ 1.5）+ 缩放行 + 直方图 + 组内间距。实现在 `EditorPanels` 第 1 组：
  给单图页签（view / issues / info 的 `<Show>` 容器）设 `min-height: calc(...)`，**用现有令牌表达**：

```css
/* 预览 3:2 = 内容宽 / 1.5；缩放行 ≈ 24px；直方图 h-44 = 176px；两个 gap-2 = 16px */
min-height: calc((var(--panel-w-right) - var(--panel-pad) - var(--panel-pad-scroll)) / 1.5 + 216px);
```

（216 = 24 + 176 + 16；实现时若缩放行实际高度不同，以**实测组件高度**回填并写注释。数值写一处、带注释说明来源。）
- 内容不足 → 留白；超出 → 各页签**自身内部滚动**（`InfoTab` 已有 `max-h`，改为 `min-h` 等值 + `overflow-y-auto`；定稿列表同样限高自滚）。
- 总览页签不加 min-height（照片比例 3:1～3:4 自然变化，3:2 时三者恰好等高）。

---

## 7. 测试与验收

**Rust（`cargo test`）**：`store/presets.rs` 的 CRUD 对内存 SQLite 逐条覆盖——建目录/重名/删 default 拒绝/删非空拒绝/建预设/同目录重名/移动/移动自动后缀（含**80 字上限撞名后仍合法**）/payload 非法 JSON 行/版本契约（v1、带 colorManagement 的 v2 接受；缺 version、v3、v2 缺字段拒绝）。命令层薄，无需单独测试。

**前端（`pnpm test`）**：`lib/presets.test.ts` 覆盖 §5.1 纯函数（含 `PARAM_DEFAULTS` 回退、LUT 忽略分支、名称字符计数与收敛纯函数）；`features/editor/store.test.ts` 覆盖库刷新收敛选中/折叠、应用 rev 单次、丢失 LUT 不动当前值；`locale-parity` 自动覆盖 i18n。

**Agent 冒烟**：`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` + `cargo check --workspace` + `pnpm smoke:ui`（第 3 组切页签等高、预设树有界自滚）。**真机验收（人类）**：新建目录/预设（含超长名提示）、应用观感、拖拽（含多选与边缘滚动）、LUT 丢失应用、等高手感。

**验收清单（崔总需求逐条）**：见 `todos/2026-09-30-editor-presets.md` §验收要点（14 条），实现后逐条核对。

---

## 8. 实现顺序建议

1. `app_0008_presets.sql` + `store/presets.rs` + `src-tauri/presets.rs` + `lib.rs` 注册（先跑通后端）。
2. `src/api/presets.ts` + `src/lib/presets.ts` + 单测（纯逻辑先行）。
3. `store.ts` 扩展 + `preset-panel.tsx` + `panels.tsx` 页签 + `EditorWorkspace.tsx` 接线 + i18n。
4. 第 1 组等高（独立小改动）。
5. 质量门 + `implementations/2026-09-30_editor-presets.md`（首行精确到秒）。
