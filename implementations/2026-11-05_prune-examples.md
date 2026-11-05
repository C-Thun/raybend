# examples 只留一份：`pnpm prune:examples`

**完成时间：2026-11-05 17:00:46**

---

## 1. 起点

崔总问：`cargo test --workspace` 会重建 examples 并占回 ~20 GB —— 这 20 GB 是「一份完整必要的中间态」还是「同一份复制几十上百份」？

实测答案：**后者**。

| 证据 | 数值 |
| --- | --- |
| `target/debug/examples` 条目数 | **895** |
| 单个二进制大小 | 218–222 MB |
| example 数量 | 27（`memory/ARCHITECTURE.md` §8 记录的「27 个诊断 example」） |
| 单个 example 的历史版本 | `raw-smoke` 3 个、`query_bench` 2 个、`thumb_bench` 2 个（release 侧实测） |
| 硬链接关系 | 规范名 `thumb-bench` 与最新 `thumb_bench-<hash>` 互链（link=2）；旧版本 link=1 |

同类证据（`deps/`，不在本次清理范围）：`libtoml` 有 **21 个 `.rlib` 变体**，集中在 3 天——10-26 五个、10-27 七个、10-31 九个。同一天内 5–9 个变体，说明是不同 feature / profile / 调用方式各编一份，且旧的不回收。

**一次构建的必然产物 ≈ 27 × 220 MB ≈ 6 GB；20 GB 是 3–4 次构建的累积。**

## 2. 根因：规则有，触发点没有

`scripts/clean-target.mjs` 早已包含 `examples/` 规则（2026-10-05 补的），但：

* 唯一调用方是 `scripts/debug-win.mjs:370`（**只打 Windows 侧** `C:\rb-target\raybend`）；
* WSL 侧只有手动命令 `pnpm clean:wsl`，全仓无任何自动调用点（无 `.cargo/`、无 git hook）；
* WSL 的构建图清单**不含 examples**，所以 examples 只能靠 `--keep-days 3` 宽限期幸存——即「3 天内没人跑 clean 就不清」。

2026-10-05 的记录 §5.2 已预言「examples 回潮」，并列为「未采纳 / 待拍板」，之后无人跟进。

## 3. 改动

| 文件 | 改动 |
| --- | --- |
| `scripts/prune-examples.mjs`（新增） | 只删 `name-<16位十六进制>` 的历史版本（含同名 `.d`），保留规范名；不编译、不重建清单；支持 `--dry-run` / `--target-dir` |
| `package.json` | 新增 `"prune:examples": "node --check … && node scripts/prune-examples.mjs"` |
| `AGENTS.md` §5.3 红线区 | 写死纪律：`cargo test --workspace` / `--all-targets` 之后必须 `pnpm prune:examples`；`clean:wsl` 每周一次 |
| `memory/ARCHITECTURE.md` §8 第 10 条 | 补 2026-11-05 实测数据与结论 |

**为什么不用 `clean:wsl` 承担这活**：它要重建构建图清单（`cargo build --workspace --lib --bins` + `cargo test --workspace --lib --bins --tests --no-run`），成本和「每次编译都跑」不匹配；`deps/` 的旧变体必须靠它（构建图比对），所以保留每周一次。

## 4. 验证（实测）

1. `node scripts/prune-examples.mjs --dry-run` → `删 40 个历史版本、74.1 MB；保留 32 个规范名`（release 侧）
2. 实际执行 release 侧 → 条目数 72 → **32**，`query-bench` 从 3 个版本变 1 个
3. `cargo build -q -p raybend --example thumb-probe` → 首次 33 s；生成 `thumb-probe`（规范名）+ `thumb_probe-b64584868eab322c`（hash 版本）
4. `pnpm prune:examples` → 删 2 个、保留规范名
5. **再次 `cargo build --example thumb-probe` → 2.2 秒**，不重编，只恢复硬链接（link=2）

关键结论：**删历史版本不触发重编**。旧版本 link=1，删除即释放；当前版本 link=2，删除后 cargo 2 秒内重建硬链接。

## 5. 遗留

1. `deps/` 的 19.5 GiB（21 个 `libtoml` 变体等）仍需 `pnpm clean:wsl`，建议每周一次；它不会自动跑。
2. 结构性方案仍待拍板：`[profile.dev.package."*"] debug = false`（体积砍 3–5 倍，代价是调试时看不到依赖内部变量）。本次未实施。
3. `target/release/examples` 也已被清理（74 MB），但 release profile 不在日常构建路径上，影响可忽略。

## 6. 纪律自查

- XMP 侧影响（§2.19）：无（构建脚本）。
- 命令体系接入（§2.15）：不适用（pnpm 脚本，不是应用内功能）。
- 磁盘代价：本脚本只 `readdir` 一层 + `stat`，不递归、不编译。

---

## 追加：把清理挂到「所有会拉屎的入口」（同日）

## 7. 崔总的要求

> 「我常跑的只有 `debug:win` 和 `release`，这两个执行完就该自动处理。
> `cargo test` agent 也经常在跑，如果也会产生垃圾，也要同步清理。」

## 8. 真凶：`cargo test --workspace` 本来就是红的

修之前先发现一个更根本的问题——`clean:wsl` 从来跑不起来：

```text
$ cargo test --workspace --lib --bins --tests --no-run
error[E0432]: unresolved import `crate::dirs::DirEmptyView`
   --> src-tauri/src/contract.rs:432:9
```

`DirEmptyView` 在 `0b857d1`（目录树 ⋯ 菜单）时改名为 `DirDeleteCheckView`，前端
（`src/api/types.ts`、`dto-contract.json`）都换了，**只有 `contract.rs` 的契约测试漏改**。
后果：`clean-target.mjs` 的 WSL 清单重建（两段 cargo）直接失败 → 脚本以 1 退出 → **target 从没被清过**。

修复：`contract.rs` 三处改名（字符串 / import / 泛型参数）。修完
`cargo test --workspace --lib --bins --tests --no-run` **0.3 秒**通过（此前 50 秒后报错）。

## 9. 三个入口全部兜底

| 入口 | 改动 | 行为 |
| --- | --- | --- |
| `cargo test`（agent 最常跑） | 新增 `pnpm test:rust`（`scripts/test-rust.mjs`） | `cargo test <args>`，结束后无论成败都跑 `prune-examples`（秒级）+ `clean-target --if-stale-days 7`；退出码 = 测试的退出码 |
| `pnpm debug:win` | `scripts/debug-win.mjs` 新增第 ⑤ 步 | 在原有 ④（Windows 侧瘦身）之后，再清 WSL 的 `./target`；**清理失败不影响构建** |
| `pnpm release` | `scripts/release.mjs` 收尾 | 构建成功后同样清 WSL `./target`；不碰发版目录与日常 debug 目录 |

`clean-target.mjs` 新增 `--if-stale-days N`：距上次成功清理不足 N 天直接跳过（时间戳 `target/.last-clean`）。
理由：WSL 清单要跑两段 cargo（实测 **1 分 20 秒**），每次构建都跑不划算；7 天一次足够。

## 10. 实测数据

| 动作 | 结果 |
| --- | --- |
| 修 `contract.rs` 后跑完整清单重建 | **0.3 秒**（fresh） |
| `clean:wsl` 首次修复后执行 | 删 **4521 项**，回收 **13.4 GiB**（deps 的旧变体为主） |
| WSL 磁盘 | 110 GiB → **97 GiB** |
| `pnpm test:rust -p raybend --lib media::tiff` | 30 个测试通过；`prune-examples` 0 项；闸门提示「距上次清理 0.0 天，跳过」 |
| 清理后 `cargo build --example thumb-probe` | **2.2 秒**（不重编，仅恢复硬链接） |

## 11. 收缩：改成文档，不做脚本（崔总 2026-11-05 定）

临时写过一版 `scripts/compact-wsl.ps1`，**已删除** —— 方案本身不成立：收缩必须在 WSL 关闭时做，
而脚本住在 WSL 的项目里，关了就跑不了。

改为 **`docs/wsl-disk-maintenance.md`**：所有路径写实（`ext4.vhdx` / `docker_data.vhdx` /
Temp 里的 swap 残留 / `wsl-crashes`），步骤可直接照抄，含 diskpart 逐行命令、验证脚本与常见问题。
另外记录了一份 2026-11-05 的完整实测基线（各阶段 C 盘可用 / WSL 内用量 / vhdx 文件大小）。

## 12. 遗留

1. **sparse VHD 按崔总意见不用**（官方不建议）。替代方案就是 `compact-wsl.ps1` 定期跑。
2. `[profile.dev.package."*"] debug = false`（体积砍 3–5 倍）仍未采纳 —— 等崔总拍板。
3. 直接跑裸 `cargo build` / `cargo check` 没有兜底（无法拦截 cargo 内置命令）；纪律写进
   `AGENTS.md` §5.3：Rust 测试一律走 `pnpm test:rust`。
