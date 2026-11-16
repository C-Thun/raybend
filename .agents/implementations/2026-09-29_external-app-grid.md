# 导出至外部：应用选择器改「手机应用网格」

完成时间：2026-09-29 15:58:00 CST

## 需求（崔总 2026-09-29）

「导出至外部的窗口美化，在窗口上半部分是应用选择器，这里不要用黑底框，就是直接在
窗口的底色上加方形图标块，把已经登记的应用像手机上的应用列表网格那样排出来，末尾
加一个空格子里面有个+号用于新增应用，如果没登记那就只有+号格子，其他逻辑应该很
容易完善了」

## 改动

| 文件 | 改了什么 |
| --- | --- |
| `src/features/external-editor/ExternalEditorDialog.tsx` | 应用选择器从「下拉框（黑底 `bg-surface-track`）+ 添加按钮」一行，改为**直接铺在窗口底色上的 5 列方形图标网格**（`data-external-application`，`role="list"`）：每格 = 方形图标块（`aspect-square`）+ 下方名称标签；末尾固定一个**虚线 + 格**（沿用 `RepositoryList` 建库入口的同一套 `border-dashed border-fg-3` 表达）用于新增；没登记应用时只有 + 格 + 一行「请先添加应用」提示。点格选中（`aria-pressed`，选中 = `bg-state-selected` + 主色描边 `outline-brand`），hover 走全局 `bg-state-hover`；loading/running/busy 时禁用（与旧下拉一致） |
| `src/i18n/zh-CN.ts` / `en-US.ts` | 新键 `external.addApplicationShort`（「添加」/「Add」，+ 格的标签；旧键 `external.addApplication` 继续用作 + 格的 aria-label） |

**没动的**：发现/手动添加弹窗（第二个 Dialog）、目录选择、任务状态/启动逻辑、
store 与 Rust 侧 —— 崔总原话「其他逻辑应该很容易完善了」= 只换选择器表达。

## 关键取舍

- **没有真 exe 图标**：登记模型只有 `{name, path}`（`lib/external-editor.ts`），
  真图标要 Windows API 抽取 + 新 IPC（`SHGetFileInfo` / exe 资源 → PNG），属于独立
  工程。v1 用**应用名首字方砖**（`app.name[0].toUpperCase()`）代位——手机网格的
  布局语义先立住，图标抽取登记为后续项（见下）。
- 5 列：Dialog 默认 `max-w-md`（≈448px），5 列 × 72px 方砖 + gap 正好铺满，一行
  放得下 4 个应用 + 1 个 + 格。

## 验证（干净 worktree：`c65f0a7` + 仅本三文件）

- `pnpm typecheck` / `pnpm test`（1116，含 locale 奇偶）/ `lint:i18n` / `pnpm build`：全绿。
- 无既有 smoke 断言引用旧下拉（查过 `ui-smoke` / `check-browse`），无需迁移。

## 未由 Agent 声称验证的 E2E

- 真机观感：方砖网格与窗口底色的融合、选中描边的一眼可辨性、长名称截断；
- 真实登记多个应用（Photoshop / Affinity 等）后的网格排布。

## 遗留问题（后续项，需要时另开工）

- **真 exe 图标抽取**（Windows API + IPC + 缓存）——首字方砖是代位方案；
- 应用格的右键/长按删除（当前删除只靠「应用不可用时自动剪枝」与候选弹窗里的已登记标记）。

## 并行会话说明

与前两条记录同期：i18n 两文件含并行会话未提交的 migration 键改动，本次提交用
「干净基线 + 仅本改动」的纯净版本进 index（`git update-index --cacheinfo`），
他们的改动原样留在工作区。
