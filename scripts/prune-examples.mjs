#!/usr/bin/env node
/**
 * **擦 examples 的屁股** —— 每个 example 只留「规范名」那一份，删掉历史 hash 版本。
 *
 * # 为什么单独做这一件事
 *
 * `cargo` 每构建一次 example，就在 `target/<profile>/examples/` 里留一份
 * `name-<16位hash>`（+ 同名 `.d`），并把「规范名」`name` 更新成指向它的硬链接。
 * **旧 hash 版本 cargo 永不回收** —— 2026-10-05 实测积到 43 GB，2026-11-05 又长回
 * 23.6 GB（895 个条目 / 27 个 example，单个二进制 ~220 MB）。
 *
 * `clean-target.mjs` 能按「构建图 + 宽限期」清它，但 WSL 侧要走一遍
 * `cargo build` + `cargo test --no-run` 重建清单（几十秒起），所以日常没人跑，
 * 于是就长回来了。本脚本**不重建清单、不编译**，规则只有一条：
 *
 *   * 文件名带 `-<16位十六进制>` 后缀的 → 历史版本，删；
 *   * 不带 hash 的「规范名」→ 当前版本，留。
 *
 * 删掉的代价：下次构建要重编这些 example（1–2 分钟）。保留的那一份**当前有效**，
 * 所以「跑过的诊断立刻再跑一次」不会触发重编。
 *
 * # 用法
 *
 * ```bash
 * pnpm prune:examples            # 清 target/<profile>/examples 里的历史版本
 * pnpm prune:examples --dry-run  # 只看会删什么
 * ```
 *
 * 什么时候跑：**每次 `cargo test --workspace` / `--all-targets` 之后**
 * （这两条会把 examples 整套重建，见 `memory/ARCHITECTURE.md` §8 第 10 条）。
 */

import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

const args = process.argv.slice(2);
const DRY_RUN = args.includes("--dry-run");
const targetDirAt = args.indexOf("--target-dir");
const TARGET_DIR =
  targetDirAt >= 0 && args[targetDirAt + 1] && !args[targetDirAt + 1].startsWith("--")
    ? resolve(args[targetDirAt + 1])
    : join(ROOT, "target");

/** `thumb_bench-6f3c1a521889d5d2` / `...-6f3c1a521889d5d2.d`。 */
const HASH_SUFFIX = /-[0-9a-f]{16}(?:\.[A-Za-z0-9._]+)?$/;

function mb(bytes) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** 只扫 `target/<profile>/examples/` 这一层（不递归，成本与文件数线性）。 */
function examplesDirs() {
  if (!existsSync(TARGET_DIR)) return [];
  const dirs = [];
  for (const entry of readdirSync(TARGET_DIR, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const dir = join(TARGET_DIR, entry.name, "examples");
    if (existsSync(dir)) dirs.push(dir);
  }
  return dirs;
}

let files = 0;
let bytes = 0;
const kept = new Set();
const dirs = examplesDirs();

for (const dir of dirs) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) continue; // rustc 中断残留（rustcXXXX）交给 clean-target 的宽限期
    if (!HASH_SUFFIX.test(entry.name)) {
      kept.add(full);
      continue;
    }
    const size = statSync(full, { throwIfNoEntry: false })?.size ?? 0;
    if (!DRY_RUN) rmSync(full, { force: true });
    files += 1;
    bytes += size;
  }
}

const scope = dirs.map((d) => d.replace(ROOT, ".").replaceAll("\\", "/"));
if (scope.length === 0) {
  console.log(`ℹ ${TARGET_DIR} 下没有 examples/，无事可做`);
  process.exit(0);
}

console.log(
  `${DRY_RUN ? "（dry-run）" : ""}examples: 删 ${files} 个历史版本、${mb(bytes)}；` +
    `保留 ${kept.size} 个规范名`,
);
for (const dir of scope) console.log(`  ${dir}`);
