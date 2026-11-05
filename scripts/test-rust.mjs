#!/usr/bin/env node
/**
 * **Rust 测试门 + 自动擦屁股** —— 跑 `cargo test`，结束后（无论成败）清理构建垃圾。
 *
 * 用法（AGENTS.md §5.3 的 Rust 测试门就用这条）：
 *
 * ```bash
 * pnpm test:rust                          # = cargo test --workspace
 * pnpm test:rust -p raybend xmp           # 透传任意 cargo test 参数
 * pnpm test:rust -- --all-targets         # `--` 会被剥掉
 * ```
 *
 * # 为什么要有这一层
 *
 * `cargo test --workspace` 会把**全部 example 重建一遍**（2026-10-05 实测），
 * 而 cargo **永不删除**旧的 `name-<hash>` 版本 —— 每次源码改动就多一份 ~220 MB。
 * 实测积到 895 个条目 / 23.6 GiB。清理规则本身在 `prune-examples.mjs`，但**没有人记得跑**，
 * 于是 2026-10-05 清到 14 GB 之后，一个月又长回 59 GB。
 *
 * 本脚本把「测试」和「擦屁股」绑成一条命令：**跑测试的人不可能忘记**。
 *
 * 三步：
 *   1. `cargo test <args>`（默认 `--workspace`）；
 *   2. `prune-examples.mjs` —— 删 examples 的历史 hash 版本，保留规范名（不编译，秒级）；
 *   3. `clean-target.mjs --if-stale-days 7` —— 每周一次完整瘦身（deps/ 的旧变体、
 *      incremental/ 的多余份）。清单重建在刚跑完测试时是 fresh 的，所以这一条也很快；
 *      不满 7 天会直接跳过。
 *
 * 退出码 = `cargo test` 的退出码（测试红了就是红的，不会因为清理脚本而变绿）。
 */

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

const argv = process.argv.slice(2).filter((arg) => arg !== "--");
const testArgs = argv.length > 0 ? argv : ["--workspace"];

/** 跑一条命令，返回退出码（不中断）。 */
function step(title, command, args) {
  console.log(`── ${title}`);
  const result = spawnSync(command, args, { stdio: "inherit", cwd: ROOT });
  return result.status ?? 1;
}

const testStatus = step(`cargo test ${testArgs.join(" ")}`, "cargo", ["test", ...testArgs]);

/*
 * 测试红了也照样擦 —— 垃圾是「跑过」产生的，与结果无关。
 * 清理失败不让整个命令变红（测试结果才是判据），只提示。
 */
step("擦 examples 的历史版本", process.execPath, ["scripts/prune-examples.mjs"]);
step("target 瘦身（每周一次；不满 7 天自动跳过）", process.execPath, [
  "scripts/clean-target.mjs",
  "--target-dir",
  "./target",
  "--if-stale-days",
  "7",
]);

process.exit(testStatus);
