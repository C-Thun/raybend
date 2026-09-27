# 记忆体重构：memory/ 目录分层、AGENTS.md 瘦身与全仓引用更新

完成时间：2026-09-27 17:09:26 CST

## 改动范围

按崔总 2026-09-27 口述的框架方案，把散在仓库根下的记忆类文件收进 `memory/`，
并重写 AGENTS.md。三个事前拍板（同日确认）：文件名用 **FUNCTION-\*.md**（正确拼写）；
**LICENSE 留在根下**（GitHub/SPDX 识别，legal/ 继续放第三方协议文本）；
**FINISHED.md 用「摘要 + 证据指针」粒度**（细节仍在 specs/implementations）。

## 文件迁移（git mv，历史保留）

| 原路径 | 新路径 |
| --- | --- |
| `PLAN.md` | `memory/PLAN.md`（重写：只留未完成排期） |
| `FUTURE.md` | `memory/FUTURE.md`（仅头部指针微调） |
| `DESIGN.md` | `memory/DESIGN.md` |
| `REVIEW.md` | `memory/REVIEW.md` |
| `ARCHITECTURE.md` | `memory/ARCHITECTURE.md`（扩充，见下） |
| `BROWSE.md` | `memory/FUNCTION-BROWSE.md` |
| `REPOSITORY.md` | `memory/FUNCTION-REPOSITORY.md` |
| `IMAGING.md` | `memory/FUNCTION-IMAGING.md` |
| `docs/issue-xmp-contract.md` | `specs/issue-xmp-contract.md`（按「docs 里给 agent 的记录归 specs」规则） |
| （无） | `memory/FINISHED.md`（新建） |

根下保留：`README.md` / `README.zh-CN.md`（对外）、`AGENTS.md`、`THIRD-PARTY-NOTICES.md`、`LICENSE`。
`legal/` 维持既有内容（第三方协议文本 + overrides），`docs/` 其余文件（用户指南、隐私、发布手册、
坐标契约报告、user-requirements 原文档案）原位不动。

## 关键决策与理由

1. **AGENTS.md 保持 §1–§11 编号骨架**（85KB → 30KB）：全仓有数百处 `AGENTS.md §x.y`
   引用（§6.4×64、§6.1×52、§2.8×40…），全部保号——迁出的章节留短摘要 + 指针，
   旧引用仍能落位（最多多跳一 hop）。§9 初始化状态保留一行（指向 FINISHED）。
2. **memory/ARCHITECTURE.md 扩充为「架构与工程基线」**：吸收 AGENTS §3 版本基线（→§5）、
   §6 架构决定（→§6，子节号 6.1–6.5 原样）、§7 调研真相（→§7，含 7.9 坐标契约）、
   §5.3 构建与排障（→§8，含十条硬规矩/5.3.1–5.3.3）、§2.17 untrack 案例（→§9）；
   原前端分层内容（§0–§4）不变；变更记录合并（§10）。
3. **memory/PLAN.md 只留未完成的事**：规划原则（§0）、路线总览与当前状态（§1）、
   待人类事项汇总（§2，新）、M5 剩余验收（§3）、三条主线（§4）、问题清单（§5，自 AGENTS §8
   迁入并核对状态）、决策记录（§6）、变更记录（§7）。M0–M4 完成内容、设计稿现状、
   路线演变、官网支线、历史附录 A/B → `memory/FINISHED.md`。
4. **FUNCTION-BROWSE.md 新增 §13**：toolsbar 三段式（13.1）与 tiles/view/film 结构红线（13.2）
   完整版自旧 AGENTS §11.1/§11.4 迁入——AGENTS §11 只留核心表，细则归模块记忆。
5. **implementations/ 历史记录一律不改**（沿 plans→specs 先例）；其中旧路径引用按当时真实路径读。
6. **specs/、design/、docs/、website/、crates/、src/、src-tauri/、scripts/、Cargo.toml 的引用
   全部更新**到新路径；`memory/PLAN.md §M<n>` 形式的波次指针定向重写为
   `memory/FINISHED.md §<节>（M<n>…）`（M0→§2、M1→§4、M2→§5、M3→§6、M4→§7；M5 仍指 PLAN §3）。

## 验证方式

- `pnpm lint:arch` ✓（输出已正确显示 `memory/ARCHITECTURE.md §1/§2`）、`pnpm lint:i18n` ✓、
  `pnpm lint:colors` ✓、`pnpm test` ✓（fail 0，2.0s）。
- 残留扫描：crates/src/src-tauri/scripts/specs/design/docs/website/prompts/memory/.github/Cargo.toml
  中旧裸路径引用 = **0**（implementations/ 除外）。
- 改动纯度核查：234 个被 sed 触及的文件中，AGENTS.md 与 memory/ 之外的**全部改动行
  均为纯路径引用替换**（非路径改动行 = 0）。
- 双前缀（memory/memory/）检查 = 0。

## 遗留问题与说明

- **仓库里有并行会话在工作**（本次任务期间观察到：`public/legal/third-party.json` 的既有修改、
  未跟踪的 `scripts/lib/json-file*.mjs` / `release-windows.test.mjs` 被该会话删除重整、
  新增 `implementations/2026-09-27_release-msi-wix-light-path.md`）。本次 commit **不含**上述
  任何非本任务的改动。
- AGENTS.md 瘦身后，旧 `AGENTS.md §6/§7/§5.3` 引用落到「摘要 + 指针」，读完整内容需多跳一 hop
  到 memory/ARCHITECTURE.md；后续新文档应直接引用 memory/ 下的正主。
- `docs/user-requirements.md` 只更新了 agent 书写的前言/补记中的路径指针，崔总原文未动。
