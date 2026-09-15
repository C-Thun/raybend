#!/usr/bin/env node
/**
 * 发版脚本（用户 2026-09-15 的评审意见 4；思路参照 `/home/andares/repos/neblor/mds`）。
 *
 * 用法：
 *   pnpm release test                    # 自测包：版本号不变，通道 test，产物在 dist/test-build
 *   pnpm release patch                   # 正式包：升补丁号
 *   pnpm release minor --channel beta    # 公测包：升次版本号并带 -beta.N
 *   pnpm release patch --dry-run         # 只打印计划，什么都不改
 *
 * 它做三件事：
 *   1. 把「这次打包是什么」算成一个计划（纯逻辑在 `src/lib/release-plan.ts`，有单测）
 *   2. 写回 `package.json` 版本号（只有正式发布才写），并把版本/通道/构建时间/git 状态
 *      通过环境变量传给 Vite（`vite.config.ts` 读它们注入 `src/lib/build-info.ts`）
 *   3. 跑前端构建；**然后停下**
 *
 * 它**不做**三件事（`AGENTS.md` §2.1：发布与推送必须由人类执行）：
 *   ✗ 不 commit、不 tag、不 push
 *   ✗ 不生成安装包（Windows 侧要按 `AGENTS.md` §5.3 的命令构建，那条路径由人跑）
 *   ✗ 不碰任何注册表 / 发布页
 * 计划里的 `humanCommands` 会把剩下该由人做的事逐条打印出来。
 *
 * 退出码：0 正常；1 参数或前置条件不对（例如正式包在脏树上且没给 --allow-dirty）。
 */

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  createReleasePlan,
  parseReleaseArgs,
} from "../src/lib/release-plan.ts";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packagePath = join(projectRoot, "package.json");

function fail(message) {
  console.error(`✗ ${message}`);
  process.exit(1);
}

function git(args) {
  try {
    return (
      execFileSync("git", args, {
        cwd: projectRoot,
        encoding: "utf8",
      }).trim() || undefined
    );
  } catch {
    return undefined;
  }
}

/** 读 package.json；读不通就直接停（发版不能靠猜版本号） */
function readPackage() {
  try {
    return JSON.parse(readFileSync(packagePath, "utf8"));
  } catch (error) {
    fail(
      `读 package.json 失败：${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

let request;
try {
  request = parseReleaseArgs(process.argv.slice(2));
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}

const pkg = readPackage();
const dirty = (git(["status", "--porcelain"]) ?? "").length > 0;
const gitHash = git(["rev-parse", "--short", "HEAD"]);

const plan = createReleasePlan({
  version: pkg.version,
  request,
  dirty,
  gitHash,
});

/* ── 打印计划（人要先看清「这一下会打出什么包」）─────────────── */

console.log("发版计划");
console.log("─".repeat(52));
console.log(`  当前版本   ${plan.currentVersion}`);
console.log(
  `  目标版本   ${plan.targetVersion}${plan.changesVersion ? "" : "（不变）"}`,
);
console.log(`  构建通道   ${plan.channel}`);
console.log(`  产物目录   ${plan.outputDir}`);
console.log(`  工作树     ${dirty ? "脏（有未提交改动）" : "干净"}`);
console.log(`  commit     ${gitHash ?? "（拿不到）"}`);
console.log("─".repeat(52));

for (const warning of plan.warnings) {
  console.warn(`⚠ ${warning}`);
}

const blocking = plan.warnings.some((warning) => /工作树是脏的/.test(warning));
if (blocking && !request.dryRun) {
  fail("正式包不带着未提交改动打。先提交，或明确用 --allow-dirty 接受。");
}

if (request.dryRun) {
  console.log("\n（--dry-run：没有改任何文件，也没有构建）");
} else {
  if (plan.changesVersion) {
    writeFileSync(
      packagePath,
      `${JSON.stringify({ ...pkg, version: plan.targetVersion }, null, 2)}\n`,
    );
    console.log(`\n✓ package.json 版本号 → ${plan.targetVersion}`);
  }

  if (request.skipBuild) {
    console.log("（--skip-build：跳过构建）");
  } else {
    console.log("\n开始构建（构建信息会被打进产物）…");
    const buildEnv = {
      ...process.env,
      RAYBEND_VERSION: plan.targetVersion,
      RAYBEND_CHANNEL: plan.channel,
      RAYBEND_BUILD_TIME: new Date().toISOString(),
    };
    if (gitHash) buildEnv.RAYBEND_GIT_HASH = gitHash;
    if (dirty) buildEnv.RAYBEND_DIRTY = "1";

    try {
      execFileSync("pnpm", ["build"], {
        cwd: projectRoot,
        stdio: "inherit",
        env: buildEnv,
      });
    } catch (error) {
      fail(
        `构建失败：${error instanceof Error ? error.message : String(error)}`,
      );
    }
    console.log(
      `✓ 前端产物在 ${plan.outputDir === "dist" ? "dist/" : "dist/"}（Vite 输出目录固定为 dist）`,
    );
  }
}

/* ── 该人做的事：逐条打印，不代劳 ──────────────────────────── */

console.log("\n接下来由你执行（脚本不会碰这些，AGENTS.md §2.1）：");
for (const command of plan.humanCommands) {
  console.log(`  $ ${command}`);
}

if (plan.channel === "test") {
  console.log(
    "\n提示：测试包的版本号没变，安装前建议先卸载旧包，避免版本号相同的两包混淆。",
  );
} else {
  console.log(
    "\n提示：Windows 安装包请按 AGENTS.md §5.3 的命令构建（前端在 WSL 出、Rust 在 Windows 编）。",
  );
}
