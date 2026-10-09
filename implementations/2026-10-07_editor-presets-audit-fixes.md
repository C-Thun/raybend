完成时间：2026-10-07 01:24:12 +0800

# Editor Presets 审计问题修复

## 范围与依据

依据 `implementations/2026-10-04_editor-presets-audit.md` 的发现（P1 一条、P2 六条、P3 一条）与规格
`specs/editor-presets.md`，逐条修复并补回归。本轮只动预设相关路径；仓库里另有其它会话的未提交改动
（`scripts/clean-target.mjs`、`memory/ARCHITECTURE.md`、`implementations/2026-10-05_clean-target.md`），未触碰。

审计报告之后色彩管理已接入预设（payload v2 + 第三个页签），本轮的版本契约与布局修复按**当前代码**核对，
不以审计时的旧行号为准。

## 逐条修复

1. **[P1] 应用预设前重读 LUT 库（丢失保底失效）**：`EditorWorkspace` 的 `onApplyPreset` 在应用前
   `await getLutLibrary(pendingLegacyLutCategories())` 并回写 store，再用**此刻磁盘**的 `available`
   跑 `planPresetApply`。旧行为只信进入编辑器时的那份缓存，文件后来被删仍会被预设覆盖上去。
   未改 Rust 缺失资源错误契约（issue/latest 的报错路径不动）。
2. **[P2] 第 3 组等高容器 + 预设树内滚**：曲线与预设改挂同一个 `data-editor-advanced-body` 容器 ——
   曲线在流内定高（切到预设时 `invisible` 留位），预设绝对定位填满同一格；预设树 `flex-1 min-h-0
   overflow-y-auto` 内部滚动，去掉原 `min-height:224px` 的估算常量。曲线自然高度随「RAW 基础曲线块 /
   机型档案」变化，所以用结构留位而不是硬编码高度（原 224 是旧 CurveEditor 尺寸的残留估算）。
3. **[P2] 整库回写收敛选中 / 折叠记录**：`store.setPresetLibrary` 现在按有效目录 / 预设 id 收敛
   `presetSelection` 与 `presetCollapsed`（纯函数 `prunePresetSelection` / `prunePresetCollapsed`）；
   `movePresetsW` 落点再过滤一次不存在条目。修「删除后选中仍含幽灵 id、拖拽把已删项当成员」。
4. **[P2] 移动撞名后缀不越名长**：Rust `store::presets::move_to` 生成后缀时给后缀留位、从基名尾部按
   **字符**截短（`PRESET_NAME_MAX = 80`），搬完仍是合法名称；前端清洗层的名称计数从 UTF-16 单元改为
   Unicode 字符（`Array.from`），目录 40 / 预设 80 与 Rust `valid_name` 完全同口径，并同样拒绝控制字符。
5. **[P2] 预设库操作串行**：`EditorWorkspace` 新增 `presetCall` 串行链（复用文件内 `persist` 的既有模式），
   初次读取与全部新建 / 删除 / 移动都排队执行，晚回的旧整库不再覆盖新状态。
6. **[P2] payload 版本契约 + 坏行诊断**：Rust `parse_payload` 只接受 v1 与「带 `colorManagement` 的 v2」
   （与前端 `sanitizePresetSnapshot` 同一集合），写入拒绝、读取跳过并 `eprintln` 记一行诊断；
   原先只校验「可解析的对象」，会存下前端永远丢弃、用户又删不掉的记录。
7. **[P2] 新建失败区分重名与其它错误**：新建目录 / 预设的前端校验（空、超长、控制字符）先行，
   重名显示本地化提示；后端失败改为 `PresetCreateOutcome`（`{ok:true}` / `{ok:false,message}`），
   把**真实原因**显示在当前弹窗内（浏览器无 Tauri 时显示「当前环境无法保存预设」）。
8. **[P3] 应用预设 rev 只抬一次**：`updateLensSide` 拆出不带 bump 的 `applyLensSide`，预设路径用它，
   由 `applyPresetSnapshot` 末尾统一抬一次 rev（实测 delta 由 2 变 1）。

## 涉及文件

- 前端纯逻辑：`src/lib/presets.ts`（名称校验 / 收敛纯函数 / 字符计数）、`src/lib/presets.test.ts`。
- 状态：`src/features/editor/store.ts`（收敛选中、rev 单次）、`src/features/editor/store.test.ts`。
- 面板与布局：`src/features/editor/panels.tsx`（等高容器）、`src/features/editor/preset-panel.tsx`
  （树内滚、名称校验、错误展示）、`src/i18n/zh-CN.ts` / `en-US.ts` / `index.ts`。
- 工作区接线：`src/workspaces/editor/EditorWorkspace.tsx`（串行链、LUT 重读、移动过滤、结果类型）。
- 存储：`crates/raybend/src/store/presets.rs`（版本契约、坏行警告、后缀截短）。
- 冒烟：`scripts/ui-smoke.mjs`（第 3 组等高 / 树内滚断言，并把 `editorPanel` / `editor` 打进快照输出）。

## 关键决策与理由

- **等高用结构留位，不用计算高度**：曲线页签高度依赖面板宽度与基础曲线块是否出现，硬编码公式会随组件改版漂移；
  「曲线隐藏但留位 + 预设覆盖填满」让两页签在结构上恒等，且预设再多也只让树内滚。副作用是曲线页签不再随
  页签切换卸载（直方图效应继续挂着，无定时器，冒烟无新增控制台输出）。
- **串行链复用 `persist` 模式**：同一文件里已有「IPC 完成顺序 ≠ 发起顺序」的既有解法，不另造版本号协议。
- **版本集合以后端与前端清洗层一致为准**：v1 + 带 `colorManagement` 的 v2；读取侧跳过并记警告而不是静默删记录。
- **名称统一按字符**：Rust `chars()` 与 TS `Array.from` 对齐，避免 80 个 emoji 在 UTF-16 下被误判成 160 字符。

## 验证（Agent 冒烟）

- `pnpm typecheck`：退出码 0（i18n 检查器的「键不存在」为文档已知的陈旧快照误报，另用临时
  `MessageKey[]` 证明文件 + tsc 退出码 0 复核）。
- `pnpm test`：**1216 通过 / 0 失败**（新增 4 条 `lib/presets` 纯逻辑 + 3 条 store 预设回归）。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：全部通过。
- `pnpm build`：通过（7214 modules，3.19s）。
- `cargo check --workspace --offline`：通过。
- `cargo test --workspace --offline`：raybend lib **1446 通过 / 12 忽略 / 0 失败**（含新增
  `move_suffix_keeps_name_within_limit`、`payload_version_contract_matches_frontend_cleaner`，
  以及扩写的坏行跳过），其余 crate 全绿。
- `pnpm smoke:ui`（`CHROME_BIN=/usr/bin/google-chrome`，dev server + Chromium）：`problems` 空；
  第 3 组实测 `curve=335 / preset=335 / back=335`，`treeOverflow="auto"`、`treeInside=true`。
- 一次性浏览器探针：目录名 41 字 → 弹窗内显示「名称最多 40 个字符」（新错误路径可见）。

## 遗留与待真机

- LUT 重读消除了旧缓存问题；「重读之后、渲染之前文件又被删」的窄窗口仍在（Rust 渲染层会报错、画面停最后一帧），
  未改缺失资源错误契约；真实磁盘删除场景待真机确认。
- 浏览器里没有选中照片时 `enabled=false`，新建**预设**弹窗打不开，错误展示只在新建**目录**弹窗做了浏览器验证
  （两者同一套代码）；真机验收时一并确认。
- GUI 拖拽手感、大量预设下的树内滚、Windows DPI / 宽松密度下的等高观感待崔总真机验收。
