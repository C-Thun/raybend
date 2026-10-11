完成时间：2026-10-10 18:43:20 +0800

# 编辑会话管道 W3：参照源按 asset 解析、对比真值上屏、两路判据统一

## 依据

`specs/editor-session-pipeline.md` §5（W3），服务现象 1（对比参照标签写 SOOC、实际比对 RAW）。

## 一、SOOC 参照源按 asset 解析（§5.1）

- 新 `develop::ReferenceSources { sooc, bitmap_available }` + `ReferenceSources::resolve(app, asset)`：
  位图侧文件必须**存在且确实是位图**（`EditBase::of_file == Sooc`），否则 `bitmap_available = false`。
- `RenderCommand::SetPhoto` 携带 `Arc<ReferenceSources>`（命令层 `editor_set_photo` 解析一次）；
  显影线程把它存成 `wanted_sources`，**跨缓存重建保留** —— 这就是现象 1 的根因：
  以前这份事实只挂在过渡帧计划上，无计划的缓存重建（切对比参照触发的
  `queue_geometry_job` 正是这种）就静默丢掉，参照退化成 RAW。
- 删除 `CachedSource.sooc` / `working_editor::Cached.sooc` / `TransitionPlan.sooc`；
  两路共用 `editor::reference_sooc_path(sources, source_path)`：
  - 有解析结果且 `bitmap_available == false` → **SOOC 参照不可用**（不允许 RAW 冒充）；
  - 不在库里（没有解析结果）→ 位图文件自己就是 SOOC 侧，RAW 没有。
- 编辑基准的兜底语义（`edit_target(Sooc)` 的 `bitmap.or(raw)`）不动，只把「参照可不可用」显式化。

## 二、对比参照真值上屏（§5.2）

- `src/api/types.ts`：`EditorRenderState.referenceBase` 放宽为
  `"sooc" | "raw" | \`issue:${number}\` | null`。
- `EditorWorkspace.tsx` 新增 `compareReferenceLabel()`：**菜单选中态看用户的选择，
  标签看后端真值**（`RenderState.referenceBase`，`referenceReady` 时才认）。
  降级时显示「选择 → 实际」，例如 `SOOC → RAW`；真值未就绪时先显示选择本身（不闪）。
- 无位图照片：SOOC 项保持禁用（`editBaseAvailable().bitmap`）。
- **同一项再点一次 = 强制重发**：`onReferenceChange` 收到与当前选择相同的值时，
  清掉 `lastCompareReferenceKey` 并抬一次 `compareReferenceRevision`（信号值不变也能重跑 effect）
  —— 「点了没反应」的兜底。

## 三、两路判据统一（§5.3）

- `should_reload_source` → `source_needs_reload(..., color_changed)`（legacy / V2 共用一份）；
  V2 的重建条件对齐 legacy 并补上色彩状态维度（详见 W2 记录 §四）。
- 单测在 W2 记录里列出；本条只补参照侧的 `reference_sooc_path_never_masquerades_a_raw_as_sooc`。

## 验证

- `cargo test --workspace` 全绿；`pnpm typecheck && pnpm test && pnpm lint:*` 全绿。
- **未经人类验证**：真机上「切 SOOC → 进裁切 → 退出，参照仍是 SOOC」、
  无位图照片的状态栏显示 `SOOC → RAW`、同项再点强制重发 —— 归崔总真机验收（§10.3）。

## 遗留

- 定稿参照（`issue:N`）的标签依赖 `issueLibrary` 里的名字；定稿被删的瞬间会退回
  「最近编辑」字样（既有行为，未改）。
- 真值标签只在 `referenceReady` 时替换用户选择：切换瞬间仍显示用户的选择，
  若后端随后降级，标签会在真帧到达时变成「选择 → 实际」。
