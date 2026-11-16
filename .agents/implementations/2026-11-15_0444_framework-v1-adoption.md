# 2026-11-15T04:44+08:00 — agent-framework v1 采纳：记忆迁 .agents/、计划成形、todos/ 退役、AGENTS.md 变索引

执行 adopt 计划 `.agents/framework/plans/plan-2026-11-15T04-21-15-08-00.json` 交给人做的那六个步骤（survey / edit-agents / edit-plan / triage-todos / budget-debt / coverage）。确定性部分（目录搬迁 + 1988 处路径重写）由工具先跑完，本记录只讲之后的编辑。

## 做了什么

1. **survey（只记证据）**：版本从清单读，不从记忆抄 —— `package.json`（pnpm 12.9.1、node ≥22.12、vite 8.3.7、TypeScript 7.0.2、`@tauri-apps/cli` 2.12.4）、`Cargo.toml`（workspace 0.1.4、edition 2024、rust-version 1.99、AGPL-3.0-only）、`rust-toolchain.toml`（1.99.1）、`LICENSE`（AGPL-3.0）、`git log --oneline -20`（HEAD `a11bac1`「地基升级（TypeScript 7 / Tauri 2.12 / Rust 1.99.1）」）。AGENTS.md 里每条命令都能对到 `package.json` 的 script 或 `scripts/*.mjs`。
2. **AGENTS.md 变索引**（73293 → 69283 bytes）：顶部加三条记忆契约；§3 版本基线、§6 架构决定塌成指针（内容早就在 `ARCHITECTURE.md` §5/§6，§6.5 唯一的两条缓存细节迁进 ARCHITECTURE §6.5）；§8/§9 墓碑删除（正文分别在 `PLAN.md` §5 与 `FINISHED.md` §2，号位留空不重排）；§10 重写成框架索引（每个记忆文件一行 + `.agents/` 子目录 + `specs/PLAN.md`/`specs/<unit>.md`）；目录树改成真实路径。
3. **`specs/PLAN.md` 成形**（40406 → 30495 bytes）：路线表只留未完成、状态列改「还差什么」；M5 四波边界与 DoD 原文迁 `FINISHED.md` §12.1，已销账的四条人类事项迁 §12.2，已关闭的 7 条问题项连结论迁 §12.3；27 条决策记录逐字迁 `ARCHITECTURE.md` §11（新章节）；§3/§6 留指针；新增 §8「规格已就位、尚未开工」与 §9「预算债与延后项」。
4. **`todos/` 退役**：7 份需求原文逐字并入各自规格的 `## Requirement (verbatim)`（标题层级降一级，正文一字未改，已用包含校验逐份核对）；新建 `specs/lut-multilayer-ui.md`；随后删除 `todos/`；29 个文件的路径串随之改写（清单见 rewrite 报告 addendum）。
5. **预算债登记**（`PLAN.md` §9 D1–D9）：AGENTS.md 超预算、DESIGN/FUTURE 超记忆文件预算、六份「本文件沿革」未合流、FINISHED 顺序、记录命名、360 条待验收、3 份规格与 PLAN 的销号口径 —— 都只登记不支付。

## 关键判断

- **版本基线塌成指针安全**：逐格比对两份表，`ARCHITECTURE.md` §5 是超集（多了 rawler/Tauri MSRV、node ≥22 说明），AGENTS 那份没有独有事实。
- **§6.5 迁两条**：只有「`CacheClass::{Preview,Full}` 命名与 5GB/可用空间 5% 上限」「失效连真身档一起 + 回收站清空连带清理」在原 AGENTS 里是独有细节，已写进 ARCHITECTURE §6.5（并指向 `specs/view-true-image.md`）。
- **六份「变更记录」不合并**（与计划建议不同，已在 D4 写明理由）：97 行逐行都是**各自文件**的沿革（大量带日期的崔总拍板），不是同一事实的副本；合到任何一个文件都会让它顶过 80000 预算。改为各自标题加文件作用域 + 一句「跨文件变更事实的唯一家是 `.agents/implementations/`」，审计的重复标题告警随之消失。

## 验证

- `framework_audit`：**0 errors**，覆盖率 **0 OPEN**（517 行，434 found / 83 declared / 0 dropped）；`W-COV-01`、`W-DUP-01`、6 条 `E-LINK-01` 全部清零。
- 剩余 9 条 warning + 1 条 info 均为有意延后，逐条对上 `specs/PLAN.md` §9 的 D1–D9。
- 记忆文件字节数逐份对比快照：全部只增不减（ARCHITECTURE +9270、FINISHED +4173、PLAN 成形后 −9911 是迁移拆分的结果）。
- 未跑产品测试：本轮没有改任何 `src/`、`crates/`、`src-tauri/` 代码。
