/** 发布计划的唯一版本/通道规则；脚本只执行该计划，不重复计算。 */
import type { BuildChannel } from "./build-info.ts";
export const BUMP_TYPES = ["major", "minor", "patch", "test"] as const;
export type BumpType = (typeof BUMP_TYPES)[number];
export const PRERELEASE_LABEL = "beta";
export interface ParsedVersion { major: number; minor: number; patch: number; prerelease?: string }
const VERSION_RE = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
/** 把用户输入的目标版本规范化：接受前导 `v`/`V`，其余交给 parseVersion 校验。 */
export function normalizeTargetVersion(input: string): string {
  const text = String(input).trim();
  const stripped = /^[vV]/.test(text) ? text.slice(1) : text;
  parseVersion(stripped);
  return stripped;
}
/** 语义化版本比较（含预发布规则）：a < b 返回 -1，相等 0，a > b 返回 1。 */
export function compareVersions(a: string, b: string): number {
  const left = parseVersion(a), right = parseVersion(b);
  for (const key of ["major", "minor", "patch"] as const) {
    if (left[key] !== right[key]) return left[key] < right[key] ? -1 : 1;
  }
  if (left.prerelease === right.prerelease) return 0;
  if (!left.prerelease) return 1;
  if (!right.prerelease) return -1;
  const leftParts = left.prerelease.split("."), rightParts = right.prerelease.split(".");
  for (let i = 0; i < Math.max(leftParts.length, rightParts.length); i++) {
    const l = leftParts[i], r = rightParts[i];
    if (l === undefined) return -1;
    if (r === undefined) return 1;
    const lNum = /^\d+$/.test(l), rNum = /^\d+$/.test(r);
    if (lNum && rNum) { if (Number(l) !== Number(r)) return Number(l) < Number(r) ? -1 : 1; }
    else if (lNum !== rNum) return lNum ? -1 : 1;
    else if (l !== r) return l < r ? -1 : 1;
  }
  return 0;
}
export function parseVersion(version: string): ParsedVersion {
  const match = VERSION_RE.exec(String(version).trim());
  if (!match || match.slice(1, 4).some(v => !Number.isSafeInteger(Number(v))) ||
      match[4]?.split(".").some(v => /^\d+$/.test(v) && v.length > 1 && v.startsWith("0"))) {
    throw new Error(`版本号必须是 x.y.z 或 x.y.z-<预发布>，分段须为安全整数且不含前导零：${version}`);
  }
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]), prerelease: match[4] };
}
export function stableVersion(version: string): string {
  const { major, minor, patch } = parseVersion(version);
  return `${major}.${minor}.${patch}`;
}
export function bumpVersion(version: string, bump: BumpType): string {
  if (!(BUMP_TYPES as readonly string[]).includes(bump)) throw new Error(`未处理的版本升迁类型：${String(bump)}`);
  const { major, minor, patch, prerelease } = parseVersion(version);
  if (bump === "test") return version.trim();
  if (prerelease) return stableVersion(version);
  const next = bump === "major" ? `${major + 1}.0.0` : bump === "minor" ? `${major}.${minor + 1}.0` : `${major}.${minor}.${patch + 1}`;
  parseVersion(next);
  return next;
}
export function nextPrerelease(version: string, label = PRERELEASE_LABEL): string {
  if (!/^[A-Za-z][0-9A-Za-z-]*$/.test(label)) throw new Error("非法预发布标签");
  const pre = parseVersion(version).prerelease;
  const tail = pre?.startsWith(`${label}.`) ? pre.slice(label.length + 1) : "0";
  const counter = tail && /^[1-9]\d*$/.test(tail) ? Number(tail) : 0;
  if (!Number.isSafeInteger(counter + 1)) throw new Error("预发布计数溢出");
  return `${stableVersion(version)}-${label}.${counter + 1}`;
}
export function inBetaSeries(version: string): boolean {
  return /^beta\.[1-9]\d*$/.test(parseVersion(version).prerelease ?? "");
}
export interface ReleaseRequest {
  bump?: BumpType; channel?: BuildChannel; dryRun: boolean; allowDirty: boolean; skipBuild: boolean;
  /** 显式指定的目标版本（与 bump 互斥；接受前导 `v`）—— 定版本走这条，自增走 bump */
  explicitVersion?: string;
  /** 准备完成后直接执行发布（提交/tag/push/上传/公开），给人类的一条命令入口 */
  publish?: boolean;
  windowsTargets: WindowsInstaller[]; windowsDir?: string; unsigned?: boolean; withUpdater?: boolean;
  /** 与 --publish 搭配：跳过 crates.io 同步 */
  noCrates?: boolean;
}
export type WindowsInstaller = "nsis" | "msi";
const USAGE = "用法：pnpm release <test|patch|minor|major> 或 pnpm release <x.y.z>（亦可 --version x.y.z） [--channel beta|test|release] [--dry-run] [--allow-dirty] [--skip-build] [--publish [--no-crates]] [--win-msi] [--win-nsis] [--win-dir <Windows 或 WSL 本地盘目录>] [--unsigned] [--with-updater]";
export function parseReleaseArgs(argv: readonly string[]): ReleaseRequest {
  let bump: BumpType | undefined, explicitVersion: string | undefined, channel: BuildChannel | undefined, windowsDir: string | undefined;
  const flags = new Set<string>();
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--version" || arg.startsWith("--version=")) {
      if (explicitVersion !== undefined) throw new Error("重复的 --version");
      const value = arg === "--version" ? argv[++i] : arg.slice(10);
      if (!value?.trim() || value.startsWith("--")) throw new Error("--version 缺少版本号");
      explicitVersion = normalizeTargetVersion(value);
    } else if (arg === "--channel" || arg.startsWith("--channel=")) {
      if (channel !== undefined) throw new Error("重复的 --channel");
      const value = arg === "--channel" ? argv[++i] : arg.slice(10);
      if (!["beta", "test", "release"].includes(value)) throw new Error(`非法通道：${value ?? "（缺失）"}`);
      channel = value as BuildChannel;
    } else if (arg === "--win-dir" || arg.startsWith("--win-dir=")) {
      if (windowsDir !== undefined) throw new Error("重复的 --win-dir");
      windowsDir = arg === "--win-dir" ? argv[++i] : arg.slice(10);
      if (!windowsDir?.trim() || windowsDir.startsWith("--")) throw new Error("--win-dir 缺少构建目录");
    } else if (arg.startsWith("--")) {
      if (arg === "--windows") throw new Error("--windows 已改为 --win-msi / --win-nsis；两个开关可同时指定");
      if (!["--dry-run", "--allow-dirty", "--skip-build", "--publish", "--no-crates", "--win-msi", "--win-nsis", "--unsigned", "--with-updater"].includes(arg)) throw new Error(`未知参数：${arg}`);
      if (flags.has(arg)) throw new Error(`重复参数：${arg}`);
      flags.add(arg);
    } else {
      if (bump !== undefined || explicitVersion !== undefined) throw new Error(USAGE);
      if ((BUMP_TYPES as readonly string[]).includes(arg)) bump = arg as BumpType;
      else if (/^[vV]?\d/.test(arg)) explicitVersion = normalizeTargetVersion(arg);
      else throw new Error(`未知的版本升迁类型或版本号：${arg}`);
    }
  }
  if (!bump && !explicitVersion) throw new Error(USAGE);
  if (bump === "test" && explicitVersion) throw new Error("test 通道不升版，不能指定版本号");
  if (explicitVersion && channel === "test") throw new Error("test 通道不升版，不能指定版本号");
  if (bump === "test" && channel && channel !== "test") throw new Error("test 只能使用 test 通道");
  if (bump !== "test" && channel === "test") throw new Error("test 通道不能升版");
  if (flags.has("--dry-run") && flags.has("--publish")) throw new Error("--dry-run 与 --publish 冲突：预览不做任何写入或发布");
  if (flags.has("--publish") && flags.has("--skip-build")) throw new Error("--publish 需要构建产物：不能跳过构建");
  if (flags.has("--publish") && bump === "test") throw new Error("--publish 不能用于 test 通道：test 包不对外公开上传");
  if (flags.has("--publish") && !flags.has("--win-msi") && !flags.has("--win-nsis")) throw new Error("--publish 需要安装器：请同时指定 --win-msi 或 --win-nsis");
  if (flags.has("--no-crates") && !flags.has("--publish")) throw new Error("--no-crates 只在 --publish 时有意义");
  const windowsTargets = (["nsis", "msi"] as const).filter(target => flags.has(`--win-${target}`));
  if (windowsTargets.length && flags.has("--skip-build")) throw new Error("Windows 打包不能跳过前端构建");
  if ((flags.has("--unsigned") || flags.has("--with-updater")) && !windowsTargets.length) throw new Error("--unsigned / --with-updater 需要 --win-msi 或 --win-nsis");
  if (windowsDir !== undefined && !windowsTargets.length) throw new Error("--win-dir 需要 --win-msi 或 --win-nsis");
  return { bump, explicitVersion, publish: flags.has("--publish"), noCrates: flags.has("--no-crates"), channel, dryRun: flags.has("--dry-run"), allowDirty: flags.has("--allow-dirty"), skipBuild: flags.has("--skip-build"), windowsTargets, windowsDir, unsigned: flags.has("--unsigned"), withUpdater: flags.has("--with-updater") };
}
/** 两类安装器共享版本线，均遵守 MSI 数值版本上限。 */
export function windowsVersion(version: string): string {
  const v = parseVersion(version);
  if (v.major > 255 || v.minor > 255 || v.patch > 65535) throw new Error("版本超出 Windows MSI 限制（255.255.65535）");
  return stableVersion(version);
}
export interface ReleasePlan {
  currentVersion: string; targetVersion: string; channel: BuildChannel; changesVersion: boolean;
  outputDir: string; humanCommands: string[]; warnings: string[]; blockers: string[];
  windowsTargets: WindowsInstaller[];
}
export interface ReleasePlanInput { version: string; request: ReleaseRequest; dirty: boolean; gitHash?: string; gitAvailable?: boolean }
export function createReleasePlan(input: ReleasePlanInput): ReleasePlan {
  const { request, dirty } = input;
  parseVersion(input.version);
  const channel = request.channel ?? (request.bump === "test" ? "test" : "release");
  if (!["test", "beta", "release"].includes(channel)) throw new Error("非法发行通道");
  if ((request.bump === "test") !== (channel === "test")) throw new Error("test 版本与通道必须一致");
  let targetVersion: string;
  if (request.explicitVersion) {
    targetVersion = normalizeTargetVersion(request.explicitVersion);
    if (compareVersions(targetVersion, input.version) <= 0) throw new Error(`指定版本必须大于当前版本 ${input.version}：${targetVersion}`);
    if (channel === "beta" && !parseVersion(targetVersion).prerelease) throw new Error(`beta 通道的目标版本要带预发布标签：${targetVersion}`);
  } else {
    if (!request.bump) throw new Error("缺少版本升迁类型或指定版本号");
    const bumped = bumpVersion(input.version, request.bump);
    targetVersion = channel === "beta" ? nextPrerelease(inBetaSeries(input.version) ? input.version : bumped) : bumped;
  }
  if (request.windowsTargets.length) windowsVersion(targetVersion);
  if (request.windowsTargets.includes("msi") && (channel === "beta" || parseVersion(targetVersion).prerelease)) {
    throw new Error("MSI 不支持预发布版本；beta 或带预发布标签的 test 请用 --win-nsis");
  }
  const blockers: string[] = [], warnings: string[] = [];
  if (channel !== "test" && dirty && !request.allowDirty) blockers.push("工作树是脏的 —— 发行必须对应 commit（--allow-dirty 明确接受）");
  if (!input.gitHash || input.gitAvailable === false) {
    const message = "拿不到 git commit / 工作树状态，无法回溯来源";
    (channel === "test" ? warnings : blockers).push(message);
  }
  if (channel === "release" && /-/.test(targetVersion)) blockers.push(`正式包不该带预发布标识：${targetVersion}`);
  if (request.unsigned) warnings.push("显式选择无 Authenticode 签名；发布页必须如实标明，Windows 信任提示由真机确认");
  const humanCommands = channel === "test" ? [] : [
    `git add package.json Cargo.toml Cargo.lock public/legal/third-party.json && git commit -m "chore: 发布版本 ${targetVersion}"`,
    `git tag -a v${targetVersion} -m "v${targetVersion}"`,
    "git push --follow-tags   # 由崔总执行；安装包/更新清单核对后再上传",
  ];
  return { currentVersion: input.version, targetVersion, channel, changesVersion: targetVersion !== input.version, outputDir: channel === "test" ? "dist/test-build" : "dist", windowsTargets: [...request.windowsTargets], humanCommands, warnings: [...warnings, ...blockers], blockers };
}
