# 定稿面板：「最近编辑」按哈希隐藏 + 每张照片定稿上限 32

> 完成时间：2026-10-31 02:40:14

## 一、起因（崔总 2026-10-31 口述）

调好照片 → 点「定稿」保存之后，latest 与刚生成的定稿**是同一份画面**，于是「最近编辑」那一行变得尴尬：
用户选它等于选那条定稿，高亮也分不清谁是谁。崔总给的口径不是「强行让 latest 可选中」，
而是**干脆不显示**：

> 当图片在调节后，如果 latest 哈希值与所有定稿都不相同时，「最近编辑」这个内置的 latest 定稿才会显示出来，
> 否则根本不显示。

## 二、改动

### 1. 「最近编辑」行的显示判据

| 层 | 改动 |
| --- | --- |
| `crates/raybend/src/store/issues.rs` | 新增 `latest_matches_issue(stack, issues)`：与 `can_finalize` 同一个 `matching_issue`（**指纹 + 逐项调整**），不另算一套 |
| `src-tauri/src/issues.rs` | `IssueLibraryDto` 新增 `latest_matches_issue`（camelCase → `latestMatchesIssue`） |
| `src/api/issues.ts` | 镜像该字段并写明判据来源 |
| `src/lib/issue-visibility.ts`（新增） | `showsLatestRow(library)`：`null`（列表还没回来）时**显示**，其余按后端判据；**前端不自己算哈希** |
| `src/features/editor/left-panel.tsx` | 新增 `showLatest` 属性，`<Show>` 包住「最近编辑」行 |
| `src/workspaces/editor/EditorWorkspace.tsx` | 传 `showLatest={showsLatestRow(issueLibrary())}` |

判据是**及时**的：`issue_library` 每次都以**当前编辑栈**（`currentDevelopStack()`）入参
（effect 读 `developRev()`，60ms 防抖），所以松手改一格参数 → 哈希变了 → 该行下一次取列表就回来；
定稿成功那条响应本身就是用库里的栈现算的，所以**确认完立刻隐藏**，不会闪。

### 2. 同哈希保存的行为（复核结论）

**存不进去，而且是刻意的**：`store::issues::create` 在 `can_finalize` 为假时返回
`不支持：当前配置与现有定稿或原始源相同`，前端按钮此时也是灰的。本轮补了回归测试
`duplicate_profile_is_rejected_and_latest_is_hidden_for_it` 把这两件事钉在**同一个判据**上
（能隐藏「最近编辑」的那份配置，必然也存不进去）。

### 3. 每张照片定稿上限 = 32

* 新增 `MAX_ISSUES_PER_ASSET: usize = 32`（`store::issues`）。
* `can_finalize`：到上限返回假 → 界面按钮变灰（与「配置重复」同一入口，不再出现「点了没反应」）。
* `create`：**先**判上限并报独立文案「这张照片已有 N 个定稿，已达上限 32 个；删掉一个再存」
  —— 不与「配置相同」混成一句，用户才知道下一步该干什么。
* 序号（`ordinal`，导出尾号 I00–I99 那 100 个位置）是**另一个口径**，不再充当业务上限；
  `allocate_ordinal` 原逻辑不动。删掉一份后序号从游标继续（不复用被删的那个号）。
* XMP：`rb:profiles` 因此有确定上界，已补进 `specs/xmp-sidecar.md` 的映射说明。

## 三、验证

* `cargo test --workspace` → **1595 + 1 + 7 + 4 + 169 + 2 + 2 passed / 0 failed**（11 + 2 ignored）。
  新增/改写：
  - `issue_count_cap_rejects_new_issues_and_frees_a_slot`（32 个后拒绝、文案含 32 且不含「相同」、
    删一个立刻能存、序号继续）
  - `duplicate_profile_is_rejected_and_latest_is_hidden_for_it`（同哈希拒绝 + 隐藏判据 + 空栈）
  - 原文里「序号用满 100 个才拒绝」的旧测试按新口径改写
* `pnpm test` → **1346 passed / 0 failed**（新增 `src/lib/issue-visibility.test.ts` 3 条）。
* `pnpm typecheck`、`pnpm build` 通过；`dist/` 已重建，等 `pnpm debug:win` 编进 exe。

## 四、影响面核对（`AGENTS.md` §2.19 / §13）

* **XMP**：映射与形状**不变**（`rb:profiles` 仍是同结构），只是条数有了上界；已更新
  `specs/xmp-sidecar.md`。
* **数据库 schema**：无变化（不动表、不动迁移）。
* **命令体系**：没有新增用户可触发动作（「最近编辑」是面板内的选择行，与定稿行同层级）。
* **缓存**：不涉及。定稿快照键仍按 `issue-<id>-<hash>`。
* **旧数据**：>32 条的旧库不会被删；只是不能再新增（`can_finalize` 变灰）。
  现有库里最多的一张是 2 条，无迁移需求。

## 五、待人类确认

* 重新 `pnpm debug:win` 后真机看一眼：定稿成功 → 「最近编辑」应立刻消失；再动一格参数 → 应回来。
* 到 32 张时的表现（按钮变灰 + 无法再存）本轮只有单测，没有真机造 32 条的验证。
