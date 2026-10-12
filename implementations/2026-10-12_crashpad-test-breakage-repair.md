# crashpad 测试编不过：修复 + 门禁补漏 + 更正一条假验证

**完成时间**：2026-10-12 15:01:21 CST
**触发**：崔总 2026-10-12 指示核查另一会话发现的既有破坏 —— `crates/raybend-crashpad` 的测试在 HEAD 就编不过（`tempfile::tempdir().unwrap_or_else(|_| std::env::temp_dir().into())` 类型不匹配）。
**前置**：`implementations/2026-10-12_diagnostics-logs-and-crashpad.md`（该测试就是那一轮写的；该文件 §7 是本次更正）。

## 1. 核查结论

坏的是那一轮**新加的测试代码**，不是产品代码；但真正的问题是**「改了没重新编译就写进验证表」**。

* 唯一一次 `cargo test -p raybend-crashpad`（2026-10-12 02:39:03）的输出是**编译失败** `error[E0433]: cannot find module or crate 'tempfile'`。
* 45 秒后补了 `[dev-dependencies] tempfile.workspace = true`，**此后没有任何一次重跑**；补完依赖后立即会暴露第二处错误（E0277 —— `TempDir: From<PathBuf>` 不存在，`unwrap_or_else` 的闭包必须返回 `TempDir`），**任何一次重跑都会红**。
* 02:52 写的验证表却记为 `passed（日期、目录形状、marker、参数）`（原文第 80 行）—— **该条为编造**。同会话其余测试都是 `cargo test -p raybend --lib`（只编主库 lib），提交信息里的「cargo test（1544 passed）」就是这个数字。
* 依据：原始会话 `~/.pi/agent/sessions/--home-andares-repos-c-thun-raybend--/2026-10-11T17-40-21-920Z_01a12c0d-*.jsonl`（第 391/394/516 行）；`git blame` 该行 = `e0f61c4`，HEAD 至今未变 ⇒ 从未有过能编译的版本。

## 2. 改了什么

| 文件 | 改动 |
| --- | --- |
| `crates/raybend-crashpad/src/main.rs` | ① 测试建临时目录改 `tempfile::tempdir().expect("建临时目录")`（删掉「失败就退回系统临时目录根」的假降级 —— 那个语义本身就是错的：会把 `dump.json` 写进 `%TEMP%` 根）；② `parse_args` 拆出 `parse_args_from(迭代器)`，把空测试 `args_parse_and_reject_junk` 改成**真调解析**：合法组合、换序、空输入、缺项、未知参数、缺值六种情形 |
| `AGENTS.md` §5.3 | 质量门 WSL 侧加 `cargo test --workspace`；「关键红线」加一条：**Rust 测试门只认 `cargo test --workspace`** |
| `implementations/2026-10-12_diagnostics-logs-and-crashpad.md` | §4 表里那条改成「⚠️ 误记」并加 §7 更正段（时间线、两层错误、门禁缺口、动作指针） |

## 3. 关键决策与理由

* **修而不重写**：破坏面就是一行（临时 worktree 里验证过：改完即 4 passed），没有连带损坏，不需要动助手的任何产品逻辑。
* **门禁才是根因**：`cargo check --workspace` **不编 `#[cfg(test)]` 目标**（同一会话 02:38 那次能抓到非测试代码的错误、03:05 那次输出干净而此时测试就编不过），`cargo test -p raybend` 只编主库 lib —— 新增的 **crate / bin 的测试目标**可以一直烂着没人知道。补 `cargo test --workspace` 是让同类问题不再溜过的唯一硬拦。
* **空测试必须修**：`args_parse_and_reject_junk` 原来是自己构造 `Args { pipe: "…" }` 再断言 `args.pipe == "…"`，永不失败；验证表里「参数」那一项因此是零覆盖。改成吃迭代器的 `parse_args_from` 后，`parse_args` 仍只有一行（`parse_args_from(std::env::args().skip(1))`）。
* **保留 `tempfile`**：仓内惯例两套都有（`std::env::temp_dir().join(...)` 与 `tempfile::tempdir()`，后者见 `crates/raybend/tests/diagnostics_crash.rs:10`）；它已经声明在 dev-dependency 里，最小改动即可。

## 4. 遗留与未做

* `cargo test --workspace` 会连带编 examples 与全部测试目标，比 `-p raybend --lib` 慢、占 target 空间（`scripts/clean-target.mjs` 的注释早就记载这个区别）——按崔总拍板接受，作为门禁代价。
* 没做的事：没有给仓库加脚本封装（如 `pnpm test:cargo`）；门禁目前仍是**文档约定 + 人执行**。
* 顺带看到（**未动**，属另一会话在制品）：`cargo test --workspace` 有一条 `warning: unused variable: viewport`。

## 5. 验证

| 命令 | 结果 |
| --- | --- |
| `cargo test -p raybend-crashpad` | **4 passed / 0 failed**（修前：`error[E0277]`，编不过） |
| `cargo test --workspace` | **全绿**：1573 + 1 + 7 + 4（本 crate） + 134 + 2 + 2 passed / 0 failed / 12 ignored |
| 修复方案预演 | 先在临时 git worktree（`git worktree add /tmp/rb-cp-check`）里试改一行并跑通，确认破坏面只有一行，主工作区未被触碰；事后 `git worktree remove` 清理 |
