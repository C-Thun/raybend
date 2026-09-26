/** 发布计划的唯一版本/通道规则；脚本只执行该计划，不重复计算。 */
import type { BuildChannel } from "./build-info.ts";
export const BUMP_TYPES = ["major", "minor", "patch", "test"] as const;
export type BumpType = (typeof BUMP_TYPES)[number];
export const PRERELEASE_LABEL = "beta";
export interface ParsedVersion { major: number; minor: number; patch: number; prerelease?: string }
const VERSION_RE = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
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
  bump: BumpType; channel?: BuildChannel; dryRun: boolean; allowDirty: boolean; skipBuild: boolean;
  windows?: boolean; unsigned?: boolean; withUpdater?: boolean;
}
const USAGE = "用法：pnpm release <test|patch|minor|major> [--channel beta|test|release] [--dry-run] [--allow-dirty] [--skip-build] [--windows] [--unsigned] [--with-updater]";
export function parseReleaseArgs(argv: readonly string[]): ReleaseRequest {
  let bump: BumpType | undefined, channel: BuildChannel | undefined;
  const flags = new Set<string>();
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--channel" || arg.startsWith("--channel=")) {
      if (channel !== undefined) throw new Error("重复的 --channel");
      const value = arg === "--channel" ? argv[++i] : arg.slice(10);
      if (!["beta", "test", "release"].includes(value)) throw new Error(`非法通道：${value ?? "（缺失）"}`);
      channel = value as BuildChannel;
    } else if (arg.startsWith("--")) {
      if (!["--dry-run", "--allow-dirty", "--skip-build", "--windows", "--unsigned", "--with-updater"].includes(arg)) throw new Error(`未知参数：${arg}`);
      if (flags.has(arg)) throw new Error(`重复参数：${arg}`);
      flags.add(arg);
    } else {
      if (bump !== undefined) throw new Error(USAGE);
      if (!(BUMP_TYPES as readonly string[]).includes(arg)) throw new Error(`未知的版本升迁类型：${arg}`);
      bump = arg as BumpType;
    }
  }
  if (!bump) throw new Error(USAGE);
  if (bump === "test" && channel && channel !== "test") throw new Error("test 只能使用 test 通道");
  if (bump !== "test" && channel === "test") throw new Error("test 通道不能升版");
  if (flags.has("--windows") && flags.has("--skip-build")) throw new Error("Windows 打包不能跳过前端构建");
  if ((flags.has("--unsigned") || flags.has("--with-updater")) && !flags.has("--windows")) throw new Error("--unsigned / --with-updater 需要 --windows");
  return { bump, channel, dryRun: flags.has("--dry-run"), allowDirty: flags.has("--allow-dirty"), skipBuild: flags.has("--skip-build"), windows: flags.has("--windows"), unsigned: flags.has("--unsigned"), withUpdater: flags.has("--with-updater") };
}
/** MSI 数值版本限制；预览版先只生成 NSIS，避免同一 MSI ProductVersion 冒充升级。 */
export function windowsVersion(version: string): string {
  const v = parseVersion(version);
  if (v.major > 255 || v.minor > 255 || v.patch > 65535) throw new Error("版本超出 Windows MSI 限制（255.255.65535）");
  return stableVersion(version);
}
export interface ReleasePlan {
  currentVersion: string; targetVersion: string; channel: BuildChannel; changesVersion: boolean;
  outputDir: string; humanCommands: string[]; warnings: string[]; blockers: string[];
}
export interface ReleasePlanInput { version: string; request: ReleaseRequest; dirty: boolean; gitHash?: string; gitAvailable?: boolean }
export function createReleasePlan(input: ReleasePlanInput): ReleasePlan {
  const { request, dirty } = input;
  parseVersion(input.version);
  const channel = request.channel ?? (request.bump === "test" ? "test" : "release");
  if (!["test", "beta", "release"].includes(channel)) throw new Error("非法发行通道");
  if ((request.bump === "test") !== (channel === "test")) throw new Error("test 版本与通道必须一致");
  const bumped = bumpVersion(input.version, request.bump);
  const targetVersion = channel === "beta" ? nextPrerelease(inBetaSeries(input.version) ? input.version : bumped) : bumped;
  if (request.windows) windowsVersion(targetVersion);
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
  return { currentVersion: input.version, targetVersion, channel, changesVersion: targetVersion !== input.version, outputDir: channel === "test" ? "dist/test-build" : "dist", humanCommands, warnings: [...warnings, ...blockers], blockers };
}
