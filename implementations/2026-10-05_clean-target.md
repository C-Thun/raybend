# clean-target 扩充：examples/ 规则 + 清单口径 + 深清（WSL target 99 GB → 14 GB）

**完成时间：2026-10-06 02:26:31**

---

## 1. 起点

崔总指出：打包脚本只清了 Windows 侧（`debug:win` 第④步自动带上 `clean-target.mjs`，目标默认
`C:\rb-target\raybend`），**WSL 侧 `./target` 没有任何自动清理入口**。实测当时 `./target` 已 **99 GB**
（WSL 根分区 251 GB 用了 90%）。崔总随后定口径：**为编译速度必要的增量基础可以留，超出部分要清**。

## 2. 改动（`scripts/clean-target.mjs`，共四处）

| # | 改动 | 理由 |
| --- | --- | --- |
| 1 | 清理范围新增 **`examples/`**（与 `deps/` 同一套主干认领：`name` / `name-<hash>` 及 `.d`/`.o`/`.rmeta`；子目录只有 rustc 中断残留的 `rustcXXXX`，一并按宽限期清） | 实测积到 43 GB（每个 example ~200MB 调试信息 × 每个 hash 一份）；旧规则完全不管这个目录 |
| 2 | 清单只在**没显式给 `--json`** 时才自动重建，且**每次都重建**（原来是「不存在才建」） | `target/last-build.json` 会过期：实测拿 10 天前的清单判活。`debug:win` 显式传 `--json` 的路径不受影响 |
| 3 | 清单口径按端分叉；WSL 侧 = **两段拼接**：`build --workspace --lib --bins` + `test --workspace --lib --bins --tests --no-run` | `cargo test` 用 cfg(test) 编 lib/bins，是**另一套单元**（实测漏一段会重编 295 个单元）；两段都进清单，`cargo build` 与 `cargo test` 双环都热。**都不含 examples**（27 个一套 22 GB，远超必要基础） |
| 4 | `collectLive` 的路径前缀匹配加 `/` 边界（`startsWith(DEBUG + "/")`） | 防止 `debug-xxx` 这类同前缀目录被误认领 |

Windows 侧行为不变（仍是产品构建清单，`debug:win` 走显式 `--json`）。

## 3. 关键事实（决定策略的三个实验）

1. **`cargo test` 会重建 examples**：把 `examples/thumb-probe` 挪走后跑 `cargo test -p raybend --no-run`，
   它被原样重建。→ 清掉 examples 不是永久性的，`cargo test --workspace` 会把它们建回来（磁盘也随之回来）。
2. **`-p raybend` 与 `--workspace` 各养一套依赖变体**（实测一次切图重编 107 个单元）→ 清单选 `--workspace` 口径，
   并建议日常用一致的构建调用。
3. **深清（`--keep-days 0`）后测试态重编 295 个单元** → 证明单段 `build --tests` 清单漏了 cfg(test) 单元，
   这就是两段拼接的由来。

## 4. 执行与验证

- 第一轮（新规则 + 当时全目标清单）：`pnpm clean:wsl` 回收 **67.6 GB**（旧 hash、rustc 临时目录、
  源码已删的孤儿二进制、旧 incremental）。
- 第二轮（清单收窄 + `--keep-days 0` 深清，按崔总「清掉超出部分」的口径）：回收 **41.4 GB**
  （examples 舰队 22 GB、各图变体、多余 incremental）。
- 结果：`./target` **99 GB → 14 GB**（deps 6.2G / incremental 2.6G / examples 176K）；
  WSL 盘 90% → **37%**（151 GB 可用）。
- 回归验证：`cargo build --workspace --lib --bins` 与 `cargo test --workspace --lib --bins --tests --no-run`
  均零重编（各余 1 个单元为图切换尾声）；`cargo test --workspace --lib media::tiff` **30 个测试 0.26s**。
  `node --check` 通过；`pnpm typecheck` 通过（pi-lens 报的 vite.config.ts TS7016 为 HEAD 既有状况，
  非本次引入）。

## 5. 遗留 / 供拍板（结构性选项，未实施）

1. **`[profile.dev] debug = "line-tables-only"`**（或对第三方依赖 `[profile.dev.package."*"] debug = false`）：
   调试信息是体积大头（213MB 的 example 二进制 → 预计 40–80MB，整体 3–5 倍缩减，链接也更快）；
   代价是调试器里看不到变量值（回溯行号仍在）。要做的话一行 profile，等崔总点头。
2. **examples 回潮**：只要跑 `cargo test --workspace` / `--all-targets`，examples 就会重建并占回 ~20 GB。
   若接受「按需重编」，日常门禁可改用 `cargo test --workspace --lib --bins --tests`（不含 examples，
   会漏 doctest，需要补 `--doc` 或单独跑）；若不接受，见上一条的 profile 方案才是治本。
3. `target/release`（1.6 GB）不在清理范围（脚本只管 debug profile），量小暂不管。

## 6. 纪律自查

- **XMP 侧影响（`AGENTS.md` §2.19）**：无（构建脚本与缓存清理）。
- **命令体系接入（§2.15）**：不适用（无应用内用户功能；`clean:win` / `clean:wsl` 的 pnpm 入口已存在）。
- 文档同步：`memory/ARCHITECTURE.md` §8 第 10 条已加「2026-10-05 扩充」小节；README 的 clean 描述
  仍准确（「只清理失效 target 产物」），未改。
- 本轮改动**未提交**（等崔总过目；上轮的提交指令是一次性的）。
