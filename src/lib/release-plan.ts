/**
 * 发版计划（纯函数，可单测）。
 *
 * 思路参照用户指定的参考实现（`/home/andares/repos/neblor/mds` 的 `client-package-version.ts`）：
 * **把「这次打包是什么」先算成一个计划对象**，再由脚本去执行。
 * 好处是版本语义与提示文案全部可测 —— 版本号算错是不可原谅的错。
 *
 * 三条边界（`AGENTS.md` §2.1）：脚本**只准备**，不推送、不打 tag、不上传。
 * 所以计划里专门有一栏 `humanCommands` —— 明确告诉人「剩下这几条你来」。
 *
 * bump 语义：
 *   `test`  版本号**不变**，通道 = test（自测包，装在同一版本上反复验证）
 *   `patch` / `minor` / `major`  升版；默认出正式版（release）
 *   若指定 `--channel beta`：升版后加 `-beta.N`（N 递增）
 */

import type { BuildChannel } from "./build-info.ts";

export const BUMP_TYPES = ["major", "minor", "patch", "test"] as const;
export type BumpType = (typeof BUMP_TYPES)[number];

export interface ParsedVersion {
 major: number;
 minor: number;
 patch: number;
 /** 预发布标识（如 `beta.3`）；稳定版为 undefined */
 prerelease?: string;
}

const VERSION_RE = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

export function parseVersion(version: string): ParsedVersion {
 const match = VERSION_RE.exec(String(version).trim());
 if (!match) {
  throw new Error(`版本号必须是 x.y.z 或 x.y.z-<预发布>：${version}`);
 }
 const [, major, minor, patch, prerelease] = match;
 return {
  major: Number(major),
  minor: Number(minor),
  patch: Number(patch),
  prerelease: prerelease || undefined,
 };
}

/** 去掉预发布标识（`0.2.0-beta.3` → `0.2.0`） */
export function stableVersion(version: string): string {
 const { major, minor, patch } = parseVersion(version);
 return `${major}.${minor}.${patch}`;
}

/**
 * 按 bump 类型算出下一个版本号。
 *
 * 预发布版本的处理是刻意的：`0.2.0-beta.3` 上做 `patch` → `0.2.0`
 * （把正在测的那个版本定稿），而不是 `0.2.1` —— 否则每发一次正式版都会凭空多出一个版本号。
 */
export function bumpVersion(version: string, bump: BumpType): string {
 const { major, minor, patch, prerelease } = parseVersion(version);

 if (bump === "test")
  return `${major}.${minor}.${patch}${prerelease ? `-${prerelease}` : ""}`;

 if (prerelease) {
  // 从预发布升到稳定版：直接定稿，不再往上跳
  return `${major}.${minor}.${patch}`;
 }

 switch (bump) {
  case "major":
   return `${major + 1}.0.0`;
  case "minor":
   return `${major}.${minor + 1}.0`;
  case "patch":
   return `${major}.${minor}.${patch + 1}`;
  default:
   // 类型上跑不到这里（test 与预发布已在上面处理），但允许 JS 侧传进意外值 ——
   // 宁可直接报错，也不要静默返回 undefined 变成一个坏版本号
   throw new Error(`未处理的版本升迁类型：${String(bump)}`);
 }
}

/** 从 `beta.3` 里取出 3；标签不匹配或不是正整数则返回 0 */
function prereleaseCounter(
 prerelease: string | undefined,
 label: string,
): number {
 if (!prerelease) return 0;
 const prefix = `${label}.`;
 if (!prerelease.startsWith(prefix)) return 0;
 const rest = prerelease.slice(prefix.length);
 const counter = Number(rest);
 return Number.isInteger(counter) && counter > 0 && String(counter) === rest
  ? counter
  : 0;
}

/**
 * 预发布号：`0.2.0` + beta → `0.2.0-beta.1`；`0.2.0-beta.3` + beta → `0.2.0-beta.4`。
 * 换了不同的预发布名（beta → rc）则重新从 1 开始。
 *
 * 注意这里**不用正则**：标签是参数，把它拼进 `new RegExp` 会得到一个
 * 「模式可注入」的写法（更别说还要担心回溯）。字符串前缀比较既精确又快。
 */
export function nextPrerelease(version: string, label = "beta"): string {
 const stable = stableVersion(version);
 const counter = prereleaseCounter(parseVersion(version).prerelease, label);
 return `${stable}-${label}.${counter + 1}`;
}

export interface ReleaseRequest {
 bump: BumpType;
 /** 显式指定通道；省略时由 bump 推导（test → test，其余 → release） */
 channel?: BuildChannel;
 dryRun: boolean;
 allowDirty: boolean;
 /** 只出前端产物（默认 `pnpm build`）；要装包由人按 `AGENTS.md` §5.3 走 */
 skipBuild: boolean;
}

export function parseReleaseArgs(argv: readonly string[]): ReleaseRequest {
 const positional = argv.filter((arg) => !arg.startsWith("--"));
 if (positional.length !== 1) {
  throw new Error(
   "用法：pnpm release <test|patch|minor|major> [--channel beta|test|release] [--dry-run] [--allow-dirty] [--skip-build]",
  );
 }
 const bump = positional[0] as BumpType;
 if (!(BUMP_TYPES as readonly string[]).includes(bump)) {
  throw new Error(
   `未知的版本升迁类型：${bump}（可选：${BUMP_TYPES.join(" / ")}）`,
  );
 }

 const channelFlag = argv
  .find((arg) => arg.startsWith("--channel="))
  ?.split("=")[1];
 const channel = channelFlag as BuildChannel | undefined;

 return {
  bump,
  channel,
  dryRun: argv.includes("--dry-run"),
  allowDirty: argv.includes("--allow-dirty"),
  skipBuild: argv.includes("--skip-build"),
 };
}

/**
 * 算出这次要出的版本号。
 *
 * beta 通道下要区分两件事（这是实测踩出来的）：
 *   已在同一条 beta 线上（`0.2.0-beta.2`）→ **只递增预发布号**，不再升版
 *   否则（稳定版 / 别的标签）→ 先按 bump 升版，再从 `-beta.1` 开始
 * 不区分的话，对 `0.2.0-beta.2` 跑 `patch --channel=beta` 会得到
 * `bumpVersion("0.2.0-beta.2","patch") = "0.2.0"` 再加 `.1` —— **同一个版本号印两遍**。
 */
function resolveTargetVersion(
 current: string,
 channel: BuildChannel,
 bumped: string,
): string {
 if (channel !== "beta") return bumped;
 const base = inBetaSeries(current) ? current : bumped;
 return nextPrerelease(base, PRERELEASE_LABEL);
}

export interface ReleasePlan {
 currentVersion: string;
 targetVersion: string;
 channel: BuildChannel;
 /** 是否要改 package.json 的版本号 */
 changesVersion: boolean;
 /** 构建产物目录（测试包与正式包分开，避免覆盖） */
 outputDir: string;
 /** 构建后由**人类**执行的命令（Agent/脚本都不许代劳，`AGENTS.md` §2.1） */
 humanCommands: string[];
 warnings: string[];
}

/** 预发布标签（目前只有 beta；要加 rc 时改这里 + `parseReleaseArgs`） */
export const PRERELEASE_LABEL = "beta";

/** 当前版本是否已在同一条 beta 线上（`x.y.z-beta.N`） */
export function inBetaSeries(version: string): boolean {
 const prerelease = parseVersion(version).prerelease;
 return (
  prerelease !== undefined && prerelease.startsWith(`${PRERELEASE_LABEL}.`)
 );
}

export interface ReleasePlanInput {
 version: string;
 request: ReleaseRequest;
 /** 构建时工作树是否脏 */
 dirty: boolean;
 gitHash?: string;
}

export function createReleasePlan(input: ReleasePlanInput): ReleasePlan {
 const { request, dirty } = input;
 const channel: BuildChannel =
  request.channel ?? (request.bump === "test" ? "test" : "release");

 const bumped = bumpVersion(input.version, request.bump);
 /*
  * beta 通道下要区分两件事：
  *   已在同一条 beta 线上（`0.2.0-beta.2`）→ **只递增预发布号**，不再升版
  *   否则（稳定版 / 别的标签）→ 先按 bump 升版，再从 `-beta.1` 开始
  * 不这样区分的话，对 `0.2.0-beta.2` 跑 `patch --channel=beta` 会得到
  * `bumpVersion("0.2.0-beta.2","patch") = "0.2.0"` 再加 `.1` —— **印出同一个版本号两遍**。
  */
 const targetVersion = resolveTargetVersion(input.version, channel, bumped);

 const changesVersion = targetVersion !== input.version;
 const warnings: string[] = [];

 if (channel === "release" && dirty && !request.allowDirty) {
  warnings.push(
   "工作树是脏的，而这是正式包 —— 正式包必须能对回某个 commit（用 --allow-dirty 明确接受）",
  );
 }
 if (!input.gitHash) {
  warnings.push(
   "拿不到 git commit（构建机没装 git 或不在仓库里）—— 包装好后无法回溯来源",
  );
 }
 if (channel === "release" && /-/.test(targetVersion)) {
  warnings.push(`正式包不该带预发布标识：${targetVersion}`);
 }

 const humanCommands = [
  `git add -A && git commit -m "chore(release): ${targetVersion}"`,
  `git tag -a v${targetVersion} -m "v${targetVersion}"`,
  "git push --follow-tags   # 推送与发布一律由人执行（AGENTS.md §2.1）",
 ];

 return {
  currentVersion: input.version,
  targetVersion,
  channel,
  changesVersion,
  outputDir: channel === "test" ? "dist/test-build" : "dist",
  humanCommands,
  warnings,
 };
}
