# 剔除 plannotator：项目记忆与全局 AGENTS.md 的流程移除

完成时间：2026-09-27 17:29:44 CST

## 改动范围

按崔总 2026-09-27 指示：把 plannotator 相关信息从本项目中剔除，同时移除全局
`~/.pi/agent/AGENTS.md` 中的 plannotator 描述。工作流自此**只保留一套**：进度用 pi 的 `todo`，
规格用 `specs/*.md`（勾选项 = 验收清单），不再有 plan/progress 双轨与 review 期禁 commit 规则。

## 项目内（raybend）

| 文件 | 改动 |
| --- | --- |
| `AGENTS.md` §2.2 | 删除「plannotator review 流程期间禁止 commit」例外条款 |
| `AGENTS.md` §5.4 | 「走 plannotator 评审」移除；勾选项改述为**验收清单**（做完即勾、状态如实），进度唯一事实来源 = `todo`；并注明**历史文件中的 plannotator 提及是当时流程的记录，不回改** |
| `AGENTS.md` §5.5 | 「plannotator plan 状态下只用 progress」→「规划任务列表用 pi 的 `todo` 工具（单份事实来源）」 |
| `memory/PLAN.md` §0 | 「提交 plannotator 评审」移除 |
| `memory/REVIEW.md` §0 | 第 2 条改述（勾选项语义属 specs 验收清单）；第 6 条 `todos / plannotator progress` → `todo` |
| `memory/FUTURE.md` | 「先出方案走 plannotator」→「先另立工作单元出方案」 |
| `specs/color-management.md` | 两处「走 plannotator 评审」→「开工前单独规划」（该规格属未开工的三条主线，必须改） |
| `specs/HANDOFF-2026-09-24-w3-bugs.md` | 两处指令性引用改写（另写 specs 规划 / 进度只用 todo） |
| `specs/HANDOFF-2026-09-27-m5.md` | 「review 期间不 commit」等失效指令改为指向 `AGENTS.md` §5.5 的 todo 纪律 |

**保留未改**：specs/ 中 19 处与 implementations/ 中的历史事实记录（如「本波不走 plannotator（人类定）」
「当前环境没有 plannotator 工具，未虚构评审」）——它们是当时真实情况的记录，改写即伪造历史；
AGENTS.md §5.4 已加注说明。

## 全局（~/.pi/agent/AGENTS.md，仓外文件）

- 「任务清单纪律（progress / todos 二选一）」整节重写为「任务清单纪律（todo）」：
  删除双模式表与 plannotator 分支，保留核心（单一事实来源）与四条纪律（不许双写 / 一次登记完 /
  新需求即时登记 / 完成立即勾选）。
- 「Plannotator Workflow」整节删除（含 STRONG CONSTRAINT 的 review 期禁 commit 规则）。
- 复核：全局文件 plannotator 出现次数 = 0。

## 验证方式

- `grep -i plannotator`：全局 AGENTS.md = 0；本项目 AGENTS.md + memory/ = 1（即 §5.4 的说明性注记，
  有意保留）；specs/ = 19（全部为历史事实记录，已逐条人工核对非指令性）。
- markdown 校验通过（各编辑文件）。

## 遗留问题

- 本仓库存在**并行会话**的未提交改动（`Cargo.toml` / `Cargo.lock` / `package.json` /
  `scripts/lib/release-*` / `src/lib/release-plan.ts` / `public/legal/third-party.json`），
  本次 commit 未包含它们。
- pi 侧如有 plannotator 扩展/模板本体（非 AGENTS.md），不在本次范围内；`~/.pi/agent/` 下未发现
  plannotator 模板文件。
