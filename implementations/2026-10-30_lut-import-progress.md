# LUT 导入：进度状态 + 真中止 + 共用进度条

> 完成时间：2026-10-30 03:05:23

## 问题（崔总报）

LUT 导入窗口里**没有任何进度**。目录里 LUT 一多，复制要花几秒，窗口看起来像卡死了；
底栏的「取消」又只是关窗，导入还在后台继续 —— 用户既看不到进展，也没法真的叫停。

## 画稿（先出稿再写码）

* `design/editor.pen` 新增帧 **`States / Editor / 导入 LUT 进度`**（`lTMMb`，深色·紧凑），两个状态并排：
  * **扫描中**：说明行 + **不确定态**进度条（没有总数 → 不编百分比，一道来回跑的亮条）；
  * **复制中**：`7 / 12` + `58%` + 确定进度条（高 6 / 轨道 `$surface-track` / 填充 `$brand`）+ 计数（已导入 / 重复 / 跳过）+ 当前项（文件名 + 「已导入的会保留」）。
* `design/editor.md`：§1 帧表加一行，新增 **§3.1.2** 写清两档状态、禁用分类选择、取消语义与「进度条只有一份」的口径。

## 复用（`AGENTS.md` §2.12）

系统里已有类似物：`ImportProgressDialog`（照片导入）的进度条 + 计数 + 当前项。
所以**先抽原语再写界面**：

| 新/改 | 位置 | 说明 |
| --- | --- | --- |
| 新增 | `src/components/ui/Progress.tsx` | **全项目唯一一份**进度条：确定态按百分比；`value = null` 走不确定态（`.rb-progress-pulse`） |
| 新增 | `src/lib/progress.ts` | `percentOf(done, total)`：没有总数 → `null`；溢出/负数夹到 0–100（有单测） |
| 改 | `src/features/import/ImportProgressDialog.tsx` | 原地那条内联进度条换成 `Progress`；顺带把嵌套三元标题提成 `dialogTitle()` |
| 新增 | `src/features/editor/lut-import-progress.tsx` | LUT 导入的状态块（扫描/复制/中止三态渲染，只此一份；陈列室也用它） |
| 改 | `src/features/editor/lut-panel.tsx` | 弹窗：导入期间换成进度状态、分类选择禁用、「取消」改真中止 |
| 改 | `src/styles/motion.css` | `rb-progress-pulse` 关键帧（状态指示类：1.4s 线性循环、只动 `transform`）；减少动态效果时**停住但不停在看不见的位置** |

## 后端（进度从哪来）

`src-tauri/src/lut.rs` 原来是一条 `blocking` 的整体命令，没有任何中间状态。现在：

* 新增事件 **`lut://import-progress`** + `LutImportProgressDto`
  （`phase` = `scan` / `copy` / `done`，`done` / `total` / `current` / `imported` / `duplicates` / `skipped` / `cancelled`）。
* `discover()` 之后 total 已知 → 每个文件发一条（**80ms 节流**，首条与末条强制发，否则界面会从「没反应」直接跳到 100%）。
* **取消是真中止**：新增 `LutImportCancel`（`AtomicBool`）+ 命令 `lut_import_cancel`；循环在**两张之间**检查，
  当前这张做完再停（硬中断会把半份副本留在库里）；返回的 `LutImportResult` 多一个 `cancelled`，
  摘要前面加一句「导入已中止：已导入的保留。」
* 前端 `src/api/lut.ts` 补 `onLutImportProgress` / `cancelLutImport` / 类型；
  `EditorWorkspace.importDirectory` 的摘要改用 `t(..., params)` 并带上中止提示。

## 交互口径

* 点「导入」→ 原生目录选择器（此时界面「正在准备…」，**不伪造扫描进度**）→ 拿到目录后进入扫描/复制。
* 导入期间：分类选择禁用（这一批往哪儿放已定）、`导入` 按钮禁用；**关窗（Esc / 遮罩）与「取消」同一条口径** —— 都是真中止。
* 取消选择目录 / 出错（返回 `null`）时**不关窗**（以前会关，让人以为点了没反应）。

## 验证

* `pnpm typecheck` / `pnpm test`（1340 passed）/ `pnpm lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build`：全绿。
  （`dist/assets` 里没有陈列室 chunk，dev-only 页面没进产物。）
* `pnpm smoke:ui`：**problems: []**，含新增断言：
  * 扫描态**不出现百分比**，进度条无 `aria-valuenow` 且有流动亮条；
  * 复制态显示 `7 / 12`、`58%`、`已导入 6`、当前文件 `Kodak 2383.cube`，进度条 `aria-valuenow=58` 且有 `aria-label`；
  * 点取消后出现「正在中止…」。
* `cargo test --workspace`：**1594 passed / 0 failed**（另 160 / 7 / 4 / 2 … 各 crate 全绿），含新增 5 条：
  DTO 字段是 camelCase（跨语言契约）、扫描态 `total = 0` 且 `current = null`、节流器首条必发、
  节流窗口有界、取消开关请求后为真且下一次导入才复位。
* 真机（Windows）E2E **未做**：目录选择器、真实复制速度、中止时机都要崔总在真环境确认。

## 影响面核对（`AGENTS.md` §2.23 / §13）

* **XMP（§2.19）**：LUT 导入只动应用内 `luts/` 私有目录与 `luts` 表；编辑栈引用的是 LUT 的 **id**，
  sidecar 里的 `rb:profileJson` 形状未变 → **无需改 XMP**。
* **命令体系（§2.15）**：没有新增用户可触发的独立命令（导入/取消都在弹窗里），默认热键无；
  打开 LUT 面板那个命令原本就在表里。
* **DB schema**：无变化（没有新表/新列），不涉迁移。
* **缓存口径（§6.5）**：不触碰缩略图/真身缓存；封面烘焙路径未改。

## 遗留

* 中止只保证「文件之间」生效：单张 LUT 的复制 + 封面烘焙不可中断（一张最多几十毫秒，可接受）。
* 扫描阶段（`discover`）不发中间进度（最多 1000 个候选，实测瞬时完成）；真要长目录再补。
* 照片导入弹窗的「扫描/规划」阶段现在也走不确定态（原来是空轨道），属于顺手统一，**未经崔总过目**。
