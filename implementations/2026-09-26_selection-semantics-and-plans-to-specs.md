# 选择语义收敛（Shift 区间 + 整段开关）+ plans/ → specs/ 改名 + 色彩管理方案入库

完成时间：2026-09-26 16:55:14 CST

## 范围

人类 2026-09-26 16:35 的口述（原文已按纪律追加进 `docs/user-requirements.md`）三件事：

1. **Shift 选择语义修订**：`Shift` 区间由「翻转」改为「**置为选中**」；`Shift + Ctrl` 同按**退化成单击**；
   **不提供区间取消选中**；主选目标（锚点）与**两端算法固定不变**。
2. **日组 / 时间片整段开关**：点一下全段选中，再点一下全段取消；全段已选中时点其它段 = 增加选中。
3. **`plans/` → `specs/` 改名** + 全部引用跟改 + 一次「以前是 plans、现在是 specs」的说明。
4. 另外：**色彩管理方案**落成可排期的执行方案（进 `specs/`）。
5. 约束：**另一边有 agent 在做 M4，避免冲突**。

## 涉及文件

### A. 选择语义（9 个文件，+265 / −83）

| 文件 | 改动 |
| --- | --- |
| `src/lib/selection.ts` | `applySelection` 的 `range` 分支：翻转 → **置为选中**（两端取法一字未动）；`clickMode` 收口「同按 → 退化为单击」并写清 `invertedCtrl` 的两侧语义；新增 `toggleGroupSelection`；文件头补「主选目标 / 两端算法固定不变」一节 |
| `src/components/ui/tiles/source.ts` | `selectGroupRange(start, count, additive?)` → **去掉 `additive`**，文档改成「整段开关」 |
| `src/features/photo-grid/PhotoGrid.tsx` | 分组标题那颗药丸不再传修饰键 |
| `src/features/photo-grid/source.ts` | 改调 `store.toggleGroup(ids)` |
| `src/features/photo-grid/store.ts` | `selectGroup(ids, additive?)` → `toggleGroup(ids)`（语义=开关） |
| `src/features/browse/store.ts` | 新增 `toggleGroup(ids)`（此前浏览侧根本没有这个能力） |
| `src/features/browse/grid-source.ts` | 改调 `store.toggleGroup(...)` |
| `src/lib/selection.test.ts` | 区间用例重写；`clickMode` 同按用例改为 `replace`；新增 `toggleGroupSelection` 3 条；**补回**被并发编辑弄丢的「导出反 Ctrl」用例（按新语义） |
| `src/features/photo-grid/store.test.ts` | 整段用例重写（含「部分选中是补齐、不是逐项反转」） |

### B. `plans/` → `specs/`（105 个文件被内容替换 + 41 个文件改名）

| 项 | 做法 |
| --- | --- |
| 改名 | `git mv plans specs`（35 个重命名 + 6 个原未跟踪文件随之搬走，41 = 41 ✓） |
| 引用替换 | 105 个活文件里 `plans/` → `specs/`（文档、代码注释、脚本） |
| **保护** | `git show cfb84a2:plans/M0.md` 这类**历史 git 路径不改**（2 处：`PLAN.md:753`、`specs/M0-1.md:7`） |
| **不改写历史** | `implementations/` 里的记录**保持原样**（那时路径确实叫 `plans/`）—— 在 `AGENTS.md` §5.4 写明了这条 |
| 说明 | `AGENTS.md` §4 目录条目 + **新增 §5.4 子节「关于目录名：以前是 `plans/`，现在是 `specs/`」** + §10 索引行 |

`.pen` 画布里**没有** `plans/` 引用（已 grep 确认），所以改名不产生画布同步项。

### C. 新增 `specs/color-management.md`（主题级规格）

按人类要求把色彩管理方案写成**可排期的执行方案**：目标/不做、现状（带源码证据）、
L1–L4 分级 + L5/L6 登记、资源与许可（lcms2 / CC0 的 Compact-ICC-Profiles / `img-parts` /
`dng` / ArgyllCMS）、架构落点（三条硬纪律 + 呈现方案 A + 覆盖层 + WebView 一致性）、
界面方案、**4 个波次的验收判据（分「Agent 可验」与「人类真机」两栏）**、8 条风险、7 项待拍板。

## 关键决策与理由

### 1. 两端算法**一字未动**（人类明确要求）

`range` 分支仍从 `state.anchor` 取起点、到 `target` 取终点、**跳过起点、含终点**。
只改了「区间内每一项怎么处理」（翻转 → 加进去）。文件头专门写了「主选目标 = 最后一次鼠标点到的那个，
不管有没有按修饰键；区间两端永远是『前一个主选 → 当前主选』，与『当前所有选中范围』无关，
**以后不管加多少种新模式这两端的取法都不变**」，并用**一条新测试**钉住了那个能看出差别的情形
（主选被 `Ctrl` 取消后再 `Shift` 点别处）。

### 2. `Shift + Ctrl` 同按 → 退化为单击，收在 `clickMode` 一处

人类原话「没人会这么操作」。原来这一条其实是**两套实现**：`clickMode` 是「Shift 优先（=range）」，
而 `PhotoGrid` 的内联三元在同样情形下退到 `toggle` —— **两边本来就不一致**。
现在函数签名里 `plain` / `withCtrl` 先算出来，同按直接返回 `plain`，
`Shift` 与 `Ctrl` 各自只管一件事。导出侧的 `invertedCtrl` 语义（单击=多选、Ctrl=单选）原样保留，
同按退化成**那一侧的**单击（导出是 `toggle`，不是 `replace`）——测试里两边都钉了。

### 3. 整段开关是**新函数**，不复用 `extendSelection`

`extendSelection` 是**只加不减**，而且导出画廊的 store 还在用它，不能改语义。
故新增 `toggleGroupSelection`（全选中→全取消；否则→全开；**不是逐项反转**），
导入与浏览两个 store 各自转发。顺带修掉 `browse/grid-source.ts` 里
`store.selectAll(additive === true ? ids : ids)` 那个**两支相同的哑三元**（`additive` 一直被丢弃）。

### 4. 批量与「整段」的定位

人类原话「跟鼠标直接点照片逻辑分开两码事，这就跟整体勾选是一个逻辑」——
所以整段开关**不看修饰键**，也因此不再是「Ctrl 加选」。这与 `BROWSE.md` §3.2 的标记批量（带修饰键）
是两套东西，文档里写清了。

### 5. `docs/user-requirements.md` 按它自己的纪律**追加原文**

该文件头部第 2 条要求「人类每次给需求，先把原文整段粘进来，再去动代码」。
本次追加了本节选择语义的四段原话 + 要点，带时间戳。**没有改写该文件里任何既有原文**
（它现有的 15 条 MD028 格式告警属于历史原文的引用块，纪律禁止重排，保持原样）。

### 6. 与 M4 的边界（人类点名要注意）

* **没碰** `src/workspaces/export/**`、`crates/raybend/src/export.rs`、`src-tauri/src/export.rs`、
  `src/api/export.ts`、`src/lib/export-*.ts`、`design/export.*`、`specs/M4-*.md`。
* 只做了两处**必要的交叉改动**：
  1. `PLAN.md` 两处提到 Shift 的句子（原文写着「不自行采纳……Shift 改语义等扩围」，
     现在这一条已被人类批准，不改就变成错误信息）；
  2. `src/lib/selection.test.ts` 末尾补回「导出反 Ctrl」用例 ——
     那条在本次并发编辑窗口里从工作区消失了（`ExportWorkspace.tsx` 于 16:42 被改过），
     `clickMode(…, true)` 仍在 `ExportWorkspace.tsx:335` 使用，这条行为必须有钉子。
* **需要 M4 侧跟进的 1 处（我没有代改）**：`src/workspaces/export/source.ts:87`
  的 `selectGroupRange: (start, count, additive = true) => … store.group(ids, additive)`。
  接口去掉 `additive` 后它**仍能编译**（可选参数），但行为会停在「只加不减」，
  与新的「整段开关」不一致 —— 等 M4 收口后改成 toggle 语义即可（一行）。

## 验证（Agent 冒烟）

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | ✅ 无错误 |
| `pnpm test` | ✅ **1008 通过 / 0 失败**（原 1006，本次净增 2 条用例） |
| `pnpm lint:arch` / `lint:i18n` / `lint:colors` | ✅ 三条全过 |
| `cargo check -p raybend --lib` | ✅ 4.89s（本次 Rust 侧只改了文档注释） |
| `git diff --check` | ✅ 干净（修掉了一处 EOF 多余空行） |
| `git mv` 完整性 | ✅ `plans/` 41 个文件 → `specs/` 41 个文件；无残留 `plans/` 目录；活文件里只剩 2 处受保护的历史 git 路径 |

**未做（归人类 / 后续）**：真机 GUI 验证（Shift 点击的手感、整段开关的可见反馈）。
按 `AGENTS.md` §2.8，Agent 不声称这些已验证。

## 遗留问题

1. **`design/browse.md` §5.1 有一处待同步到画布**：`States / 浏览标记与选择` 里
   「`Shift` 翻转掉的那个」**那一格已不存在**（tile 只剩选中/未选两态），
   下次开 Pencil 时删掉那一格（7 格 → 6 格）。已在 `design/browse.md` 就地标注。
2. **日组/时间片药丸的文案**（`全选当天` / `全选此段`）**保持原样未动** ——
   它现在是「整段开关」，文字描述的是能力名而非当前状态；要不要改成中性说法（或给「已全选」态）
   留待人类定。**没有改 i18n 文件**（避免与 M4 抢同一文件）。
3. **M4 侧 `export/source.ts` 的整段语义**待收口（见上「与 M4 的边界」）。
4. `specs/color-management.md` §9 的 **7 项待拍板**（工作空间选型等）未定，
   **未排期**；每波开工前另写 `specs/CM-W<n>.md`。
5. 未 commit：工作区里有大量其它 agent 的在改内容，不代人类决定提交边界。
   建议本次改动（选择语义 + 改名 + 新规格）单独形成一个提交，不要与 M4 混在一起。
