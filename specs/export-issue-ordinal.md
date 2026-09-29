# 导出文件名：issue 序号体系（IXX / ISO / IRA / ILA）

> 工作单元：#24 ｜ 状态：**代码完成，收口项待环境**（check:export 整跑 + 真机 E2E）｜ 需求原文已按约定移除（todos/ 做完即删）
> 关联记忆：`memory/FUNCTION-IMAGING.md`（issue 语义）、`memory/FUNCTION-REPOSITORY.md`（命名模版）

## 1. 背景与要修的 bug

- **大 bug**：同一照片的多个 issue 一次导出时，模板展开出的主名相同，导出层把它们当成
  **同名文件冲突**（队列去重键带 profile_hash 所以都能入队；冲突发生在输出文件名层面，
  `output.rs::publish_with_policy` 对第二个及以后的稿按「文件已存在」处理）。
- **解法**：导出文件名 = 模板主名 + **强制尾号**。尾号本身就是 issue 的身份，天然互不冲突。

## 2. 命名契约

- 尾号结构：`I` + 定符；**无其它分隔符**（`I` 即分隔）。例：主名 `PA00034` + 序号 3 → `PA00034I03`。

| 导出的东西 | 尾号 | 说明 |
| --- | --- | --- |
| 原片（`sooc` 变体 / 空栈原片） | `ISO` | 原片直出 |
| RAW 标记 issue（`raw` 变体） | `IRA` | RAW **直接转码**的灰度图，**不经任何曲线与修改**（空栈走现有解码管线即是）；原 RAW 文件本身**没有导出概念**（将来另给「复制出去 / 打开所在目录」，本轮不做） |
| 命名 issue（`issue:N`） | `I00`–`I99` | 该 issue 的序号，2 位十进制 |
| 未匹配 latest（`latest` 变体） | `ILA` | 仅当 latest 的 profile 哈希**对不上任何 issue** 时 |

- **latest 优先按哈希匹配已有 issue**（`export.rs::summaries` 末尾的 matched 循环已有此逻辑）：
  匹配上 ⇒ 前端入队的就是那个 issue 的引用（尾号 `IXX`），export tiles 的显示同样按那个 issue 算；
  哈希对不上 ⇒ 尾号 `ILA`。
- 尾号追加在模板展开结果**之后、扩展名之前**（`relative_name` 里 `format!("{text}{suffix}.{ext}")`）；
  长度/设备名检查对含尾号的最终名生效。

## 3. 序号的生成与分配

- `issues` 表加 `ordinal INTEGER`（0–99，每资产唯一，`UNIQUE(asset_id, ordinal)` 索引兜底）；
  `assets` 表加 `issue_counter INTEGER NOT NULL DEFAULT 0`（**下一候选**的提示值）。
- **分配**（`issues::create`）：从 `issue_counter` 起步向上找第一个空位，99 之后**绕回 0**；
  找到 ⇒ 写入 `ordinal`，`issue_counter` 更新为 `(分配值+1) % 100`；0–99 全满 ⇒ **拒绝保存定稿**
  （错误：「已达 100 个定稿上限」）。删除 issue 留下的空洞可被复用（扫描自然复用）。
- **存量回填**（迁移 `catalog_0013`，纯 SQL，相关子查询不依赖窗口函数）：
  每资产按 `(created_at, id)` 升序 0,1,2,…；`issue_counter` 回填为该资产 issue 数 % 100。

## 4. RAW 回到 export tiles 的 issue 行

- Rust `export.rs::summaries`：照片**同时有** bitmap 与 raw 时，base 仍是 SOOC，但**补一行 raw 变体**
  （`variant:"raw"`、`name:"RAW"`、`source_base:raw`、空哈希语义——它不是编辑稿，是「标记为 RAW 的 issue」）。
- 前端 `src/lib/export-model.ts`：**去掉对 `variant === "raw"` 行的过滤**（variants 列表与 issue 条两处），
  raw 行按「标记为 RAW 的 issue」进 issue 条，导出即 IRA（空栈 → 现有解码管线直出灰度图）。
- 显示尾号徽标：issue 行 `I03`、RAW 行 `IRA`、未匹配 latest `ILA`、原片行 `ISO`
  （`VariantSummary` 增加 `ordinal: number | null`；raw/sooc/latest 为 null，由前端按变体映射固定尾号）。

## 5. 改动落点

| 层 | 文件 | 改什么 |
| --- | --- | --- |
| 存储 | `store/migrations/catalog_0013_issue_ordinal.sql`、`store/migration.rs` | 列 + 唯一索引 + 回填；版本闸门走现有框架（AGENTS §2.16） |
| 存储 | `store/issues.rs` | `Issue.ordinal`；`create` 分配序号；`get/list` 带出；100 上限 |
| 导出 | `crates/raybend/src/export.rs` | `summaries` 补 raw 行 + `VariantSummary.ordinal`；新增 `issue_suffix(conn, &VariantRef) -> String` |
| 导出 | `export/output.rs` | `relative_name` 加 `suffix` 参数（execute / skip_existing 跟着传） |
| 外壳 | `src-tauri/src/export.rs` | `run_item`/`prepare` 算尾号（latest 需查库匹配）；预览 `export_preview` 同口径 |
| 外壳 | `src-tauri/src/external_editor.rs` | 交片文件名接入同一尾号函数（按其 reference） |
| 前端 | `src/lib/export-model.ts`、`src/api/*`（导出类型） | 去 raw 过滤；尾号徽标；ILA 判定展示 |
| 前端 | export tiles issue 条组件 + i18n 两语言 | 徽标文案/样式 |
| 检查 | `scripts/check-export-boot.mjs` | RAW 行出现后的断言更新 + 新断言（尾号可见） |

## 6. 测试与验收（做完即勾）

- [x] Rust 单测：序号分配（顺序 0→99、99 后绕回 0、删后空洞复用、100 个全满拒绝、迁移回填）
- [x] Rust 单测：`relative_name` 尾号拼接（IXX/ISO/IRA/ILA 四类 + 长度/设备名检查仍生效）
- [x] Rust 单测：`summaries` JPG+RAW 照片出现 raw 行；`issue_suffix` 四类映射正确
- [x] 前端单测：export-model raw 行进 issue 条；latest 匹配/未匹配的显示口径
- [ ] `cargo test` / `pnpm test` / 三 lint / `check:export` 全绿 —— 除 `check:export` 外全绿；
      check:export 因并行 Agent 半成品环境性受阻（A/B 已定责，见实施记录），待落定后整跑收口
- [ ] 真机（崔总）：同文件多 issue 一次导出互不冲突、文件名各带尾号；RAW 行导出 IRA；E2E 由人类确认

## 7. 不做的事（本轮明确出界）

- 原 RAW 文件的「复制出去 / 打开所在目录」——将来另给。
- 尾号可配置/可关——不做，强制。
- `:SEQ` 模版占位符与尾号的关系——互不相干，`:SEQ` 照旧按队列序号展开，尾号永远追加。
