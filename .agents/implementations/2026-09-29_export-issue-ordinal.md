# 导出文件名 issue 序号体系（I00–I99 / ISO / IRA / ILA）

完成时间：2026-09-29 00:17:56

## 改动范围

规格：`specs/export-issue-ordinal.md`（需求原文 `todos/2026-09-28-export-issue-ordinal.md`，已按约定移除）。
修复的大 bug：同一文件的多个 issue 一次导出时模板主名相同，被导出层当成同名文件互相冲突——
尾号即 issue 身份，天然互不冲突。

| 层 | 文件 | 内容 |
| --- | --- | --- |
| 迁移 | `store/migrations/catalog_0013_issue_ordinal.sql` + `store/migration.rs` | `issues.ordinal`（UNIQUE(asset_id,ordinal)）、`assets.issue_counter` 游标、存量按 `(created_at,id)` 回填 0..n、游标=n%100（纯 SQL 相关子查询，不依赖窗口函数） |
| 存储 | `store/issues.rs` | `Issue.ordinal`；`create` 里 `allocate_ordinal`（从游标顺找空位、99 绕回 0、全满拒绝「已达 100 个定稿上限」、删后空洞在扫过时复用） |
| 导出 | `crates/raybend/src/export.rs` | `VariantSummary.ordinal`；`summaries` 给 JPG+RAW 照片补「标记为 RAW 的 issue」行（放 retain 前，未编辑且基准即 RAW 时被主行吸收去重）；新增 `issue_suffix`（sooc→ISO、raw→IRA、latest→哈希匹配→IXX / 不匹配→ILA、issue:N→IXX） |
| 导出 | `export/output.rs` | `relative_name` 加 `suffix` 参数（拼在模板主名后、扩展名前；设备名/长度检查对含尾号的最终名生效）；`skip_existing`/`execute`/`execute_checked` 跟进 |
| 外壳 | `src-tauri/src/export.rs`、`src-tauri/src/external_editor.rs`、`crates/raybend/src/external_editor/mod.rs` | `Prepared.suffix`（`prepare_with_catalog` 里一次算好，latest 的哈希匹配判定也在此完成）；run_item / 预览 / 外部编辑器交片全走同一尾号 |
| 前端 | `src/lib/export-model.ts`、`src/workspaces/export/ExportWorkspace.tsx` | 类型加 `ordinal`；`visibleVariants`/`orderedIssues` 去掉 raw 过滤（RAW 回 issue 条）；`variantSuffix` 与后端同口径；IssueTile 右上角尾号徽标（`data-export-suffix`） |
| 冒烟 | `scripts/check-export-boot.mjs` | mock 加 raw 行 + ordinal；新断言：徽标 I07/IRA/ISO、条目数 3、弹窗 11、Ctrl+A 11、拖拽入队 11/11、去重 11 |

## 关键决策

- **空洞不急补**：分配从游标顺找（需求原文口径），删留下的洞在扫描经过或绕圈时复用——
  测试最初按「急补」写错过一次，已按语义改正。
- **latest 哈希匹配在 Rust `summaries` 已有**（matched 循环把匹配定稿顶成主行），前端入队的
  引用即那个 issue → 尾号自然是 IXX；ILA 只出现在真正未匹配时。显示与导出同一份数据，不存在两个口径。
- **IRA 不新造渲染路径**：RAW 变体 = 空栈走现有解码管线（转码灰度图，不经曲线与修改）。
- **测试数据教训**：构造 DevelopStack 参数别用算术产物（`41.0*0.01`）——serde_json 对长形十进制串
  解析有 1-ulp 误差，create 时按原值哈希、读回按解析值校验会「配置指纹不一致」；前端来的值都是
  短形字面量，线上无恙。测试改用整数字面量。

## 验证

- `cargo test -p raybend --lib`：序号分配（顺序/绕圈/空洞/百上限/删后复用）、迁移回填 v12→v13、
  `issue_suffix` 四类、`summaries` raw 行 + ordinal、`relative_name` 尾号拼接 + skip 路径——全绿
  （1202+ 过；唯一红是 `store::availability`，**外来并行工作的测试**，与本改动无关）。
- `pnpm typecheck` / `pnpm test`（1092 项）/ `lint:colors` / `lint:arch` / `lint:i18n`：全绿。
- `check:export`（CDP 冒烟）：**环境性受阻**——工作区里并行 Agent（storage-recovery 一路）的
  半成品使 dev server 启动抖动（空页面/缩略图超时交替），且「背景刷新不得挪动左侧目录行」
  （shifts:21）断言经 **A/B 实验定责为对方半成品**（去掉本改动的 raw 行后 shifts 仍 21）。
  本改动自身的断言已分段验证：徽标/条目数/scope 计数/六格/弹窗 11 在成功启动的跑次中全过；
  Ctrl+A 11 与拖拽 11/11 为按 mock 对账的算术修正（弹窗 11 实测过）。待并行工作落定后整跑一次收口。

## 遗留

- `check:export` 整跑绿（待并行半成品落定）＋ 真机（崔总）E2E；在此之前不构建 Windows 测试包
  （会把并行半成品一并烤进去）。
- 原 RAW 文件的「复制出去 / 打开所在目录」不在本轮（崔总将另给需求）。
