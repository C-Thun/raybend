#!/usr/bin/env node
/**
 * **清理 target 目录里的失效产物** —— 删「不再是当前构建图一部分」的东西，
 * **不动目录本身、不动还在用的产物**。
 *
 * 默认打 Windows 侧的 `C:\rb-target\raybend`（`pnpm clean:win`，`debug:win` 会自动带上）；
 * 也能打 WSL 侧那个 `target/`（`pnpm clean:wsl`）—— 两边同一套规则。
 *
 * # 为什么需要它
 *
 * 这个 target 目录**只进不出**：每次依赖升级 / feature 组合变化 / 换构建参数，
 * cargo 就多留一份 artifact，而它**从不回收**（2026-09-25 实测：一个 target 目录
 * 攒到 11.6 GiB —— `deps/` 里 `libtoml` 有 11 份、`libwindows` 4 份（每份 ~90MB），
 * `incremental/` 里 `raybend_desktop_lib-*` 有 10 个不同 unit-hash 目录）。
 * 于是「偶尔 `cargo clean` 一下」变成必需 —— 但那会**连还在用的产物一起删**，
 * 代价是一次十几分钟的冷构建。本脚本要的就是「不付这个代价」。
 *
 * # 凭什么判断「谁还活着」
 *
 * `cargo build --message-format=json` 会为**构建图里的每一个单元**输出一条
 * `compiler-artifact`（**已经是最新的单元也在内**，带 `"fresh":true`）——
 * 这就是权威答案，不需要猜。
 *
 * 三类东西按这个集合清：
 *
 * | 位置 | 规则 |
 * | --- | --- |
 * | `deps/` | 只留「文件名主干 ∈ 存活集合」的。`libX-hash.rlib` 的伴随文件（`X-hash.d` / `.rmeta` / `.pdb`）靠**去掉 `lib` 前缀**一起认领 —— 少留一个 `.d`，cargo 下次就会重编那个单元 |
 * | `build/` | 只留存活集合里出现过的 `<包>-<hash>/` 目录 |
 * | `incremental/` | 每 crate **保留 K 份**（K = 这个 crate 在存活集合里有几个 hash；最新的 K 份），多出来的删。⚠️ 目录名用的是**另一套 hash**（不是 rlib 那 16 位十六进制，映射不上），所以只能按「数量 + 时间」保 —— 但**它是纯缓存**，多删一份的代价只是那个 crate 下次重编慢一点 |
 * | `.fingerprint/` | **不碰**（只有几十 MB，而删错会让 cargo 白重编） |
 *
 * # 宽限期：`--keep-days`（默认 3 天）
 *
 * 光看「我的构建图里有没有」不够 —— **同一台机器上另一个会话可能用另一套构建参数**
 * （不同 feature / 不同 `-p` 选择），它的产物在我的图里就是「不存活」的，
 * 但它**是在用的**。所以：**最近碰过的一律不动**，只清「既不在我的图里、
 * 又已经凉了很久」的东西。这样每次构建自然清一点，而不会把别人正在用的东西删了
 * （真删错的代价也只是重建，但没必要的重建就是浪费）。
 *
 * # 用法
 *
 * ```bash
 * pnpm clean:win --dry-run     # 只看会删什么（推荐先跑一次）
 * pnpm clean:win               # 自己跑一次 JSON 构建 → 清
 * pnpm clean:wsl --dry-run     # 同样的规则打 WSL 那个 target/
 * ```
 *
 * `pnpm debug:win` 会在构建完自动带上它（复用那次构建的 JSON，不额外编译）——
 * 所以**日常不用手动跑**。
 */

import { spawnSync } from "node:child_process";
import { closeSync, existsSync, openSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { windowsBuildEnv } from "./lib/dav1d-win.mjs";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const WIN_TARGET_DIR = process.env.WIN_TARGET_DIR ?? "C:\\rb-target\\raybend";

/** `C:\rb-target\raybend` → `/mnt/c/rb-target/raybend`；本机路径（含相对路径）原样。 */
function toLocalPath(path) {
  const drive = /^([A-Za-z]):[\\/](.*)$/.exec(path);
  if (drive !== null) return `/mnt/${drive[1].toLowerCase()}/${drive[2].replaceAll("\\", "/")}`;
  return resolve(path);
}

/** 失败即退出（打印补救提示）。 */
function fail(message, remedy) {
  console.error(`\n✗ ${message}`);
  if (remedy) console.error(`  ${remedy}`);
  process.exit(1);
}

/* ── 参数 ─────────────────────────────────────────────────── */

const args = process.argv.slice(2);
const has = (flag) => args.includes(flag);
const valueOf = (flag, fallback) => {
  const at = args.indexOf(flag);
  if (at < 0) return fallback;
  const value = args[at + 1];
  if (value === undefined || value.startsWith("--")) return fallback;
  return value;
};

const DRY_RUN = has("--dry-run");
/** 宽限期（天）：既不在存活集合里、又凉了这么久的才删（理由见文件头）。 */
const KEEP_DAYS = Number(valueOf("--keep-days", "3"));
if (!Number.isFinite(KEEP_DAYS) || KEEP_DAYS < 0) {
  fail(`--keep-days 要给天数（≥0），收到：${valueOf("--keep-days")}`);
}

const TARGET_WIN = valueOf("--target-dir", WIN_TARGET_DIR);
/** 这个 target 在 Windows 侧吗（决定构建那一步走 cmd.exe 还是本地 cargo）。 */
const IS_WINDOWS_TARGET = /^[A-Za-z]:[\\/]/.test(TARGET_WIN);
const TARGET = toLocalPath(TARGET_WIN);
if (TARGET === null) {
  fail(`--target-dir 要给 Windows 路径（C:\\…）或本机绝对路径（/…），收到：${TARGET_WIN}`);
}
if (!existsSync(TARGET)) fail(`target 目录不存在：${TARGET}`);
const DEBUG = join(TARGET, "debug");
const JSON_PATH = valueOf("--json", join(TARGET_WIN, "last-build.json"));
const JSON_LOCAL = toLocalPath(JSON_PATH);


/* ── 1. 拿到构建图（谁还活着） ─────────────────────────────── */

if (!existsSync(JSON_LOCAL)) {
  console.log(`没有现成的单元清单，跑一次 JSON 构建：${JSON_PATH}`);
  const buildArgs = [
    "build",
    "-p",
    "raybend-desktop",
    "-p",
    "raybend",
    "--features",
    "custom-protocol",
    "--message-format=json",
  ];
  let status = 1;
  if (IS_WINDOWS_TARGET) {
    const command = `cargo ${buildArgs.join(" ")} > ${JSON_PATH}`;
    status = spawnSync("cmd.exe", ["/c", `pushd ${windowsRepoPath()} & ${command}`], {
      stdio: ["ignore", "inherit", "inherit"],
      env: windowsBuildEnv({ CARGO_TARGET_DIR: TARGET_WIN }),
    }).status;
  } else {
    const json = openSync(JSON_LOCAL, "w");
    status = spawnSync("cargo", buildArgs, {
      stdio: ["ignore", json, "inherit"],
      env: { ...process.env, CARGO_TARGET_DIR: TARGET_WIN },
    }).status;
    closeSync(json);
  }
  if (status !== 0) fail("JSON 构建失败", "先修构建：pnpm debug:win / cargo check");
}
if (!existsSync(JSON_LOCAL)) fail(`构建没落下单元清单：${JSON_PATH}`);

/** 仓库在 Windows 侧的 UNC 路径（`pushd` 要用）。 */
function windowsRepoPath() {
  const distro = process.env.WSL_DISTRO_NAME;
  if (!distro) fail("读不到 WSL_DISTRO_NAME（要在 WSL 登录 shell 里跑）");
  return `\\\\wsl.localhost\\${distro}\\${ROOT.replace(/^\//, "").replaceAll("/", "\\")}`;
}

const live = collectLive(JSON_LOCAL);
if (live.files.size === 0) fail(`单元清单里一个产物都没有：${JSON_PATH}（构建是不是根本没跑？）`);

/** 读 cargo 的 JSON 流：收集产物路径 + 版本信息。 */
function collectLive(jsonPath) {
  const files = new Set();
  let units = 0;
  let fresh = 0;
  for (const line of readFileLines(jsonPath)) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue; // cargo 偶尔会往 stdout 里混别的东西（例如警告包装），忽略
    }
    if (message.reason !== "compiler-artifact") continue;
    units += 1;
    if (message.fresh === true) fresh += 1;
    for (const file of message.filenames ?? []) {
      const local = toLocalPath(file);
      if (local !== null && local.startsWith(DEBUG)) files.add(local);
    }
  }
  return { files, units, fresh };
}

/* ── 2. 存活集合 → 各类键 ─────────────────────────────────── */

/**
 * 一个产物文件「认领」的所有名字主干（用来认领它的伴随文件）。
 *
 * `libwindows-<hash>.rlib` → 认领 `libwindows-<hash>` 与 `windows-<hash>`
 * （dep-info 是 `windows-<hash>.d`，没有 `lib` 前缀）；反过来
 * `raybend_raw_worker-<hash>.exe` → 认领 `raybend_raw_worker-<hash>` 与 `libraybend_raw_worker-<hash>`。
 *
 * 同时给出 `-` → `_` 的归一形式：cargo 把最终产物的 **`deps/` 副本**改名成下划线形式
 * （JSON 里是 `debug\raybend-desktop.exe`，而磁盘上是 `deps/raybend_desktop.exe` ——
 * 同一份硬链接）。不归一会把它们当成「没人认领的旧产物」误删。
 */
function claimedStems(name) {
  const stems = new Set();
  for (const raw of rawStems(name)) {
    stems.add(raw);
    stems.add(raw.replaceAll("-", "_"));
  }
  return stems;
}

/** 归一前的候选主干（含去掉 / 补上 `lib` 前缀那对）。 */
function rawStems(name) {
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const stems = new Set([name, stem]);
  if (stem.startsWith("lib")) stems.add(stem.slice(3));
  else stems.add(`lib${stem}`);
  return stems;
}

const liveStems = new Set();
const liveBuildDirs = new Set();
/** crate 名 → 存活 hash 集合（`incremental/` 靠它定 K）。 */
const liveHashesPerCrate = new Map();

for (const file of live.files) {
  const name = basename(file);
  for (const stem of claimedStems(name)) liveStems.add(stem);

  const build = /[\\/]build[\\/]([^\\/]+)[\\/]/.exec(file);
  if (build !== null) liveBuildDirs.add(build[1]);

  const unit = /^(?:lib)?([A-Za-z0-9_]+)-([0-9a-f]{16})$/.exec(stemOf(name));
  if (unit !== null) {
    const set = liveHashesPerCrate.get(unit[1]) ?? new Set();
    set.add(unit[2]);
    liveHashesPerCrate.set(unit[1], set);
  }
}

function stemOf(name) {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/* ── 3. 清 ────────────────────────────────────────────────── */

const plan = { files: [], dirs: [], bytes: 0 };
const note = (path, size) => {
  plan.files.push(path);
  plan.bytes += size;
};
const GRACE_MS = KEEP_DAYS * 24 * 60 * 60 * 1000;
const freshEnough = (mtimeMs) => GRACE_MS > 0 && Date.now() - mtimeMs < GRACE_MS;
/** 目录递归体积（报告用；9p 上慢一点，但只在真要删的时候走）。 */
function dirSize(path) {
  let total = 0;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) total += dirSize(child);
    else {
      try {
        total += statSync(child).size;
      } catch {
        // 无所谓的竞态（另一个进程刚动过）：不计入
      }
    }
  }
  return total;
}
const noteDir = (path) => {
  const info = statSync(path);
  plan.dirs.push(path);
  plan.bytes += dirSize(path);
  void info;
};

/* 3a. deps/ —— 主干没人认领、且凉了够久的才删 */
for (const entry of readdirSync(join(DEBUG, "deps"))) {
  const full = join(DEBUG, "deps", entry);
  const info = statSync(full);
  if (!info.isFile()) continue;
  if (liveStems.has(stemOf(entry)) || liveStems.has(stemOf(entry).replaceAll("-", "_"))) continue;
  if (freshEnough(info.mtimeMs)) continue;
  note(full, info.size);
}

/* 3b. build/ —— 目录名不在存活的构建脚本里（且凉了够久）就删 */
for (const entry of readdirSync(join(DEBUG, "build"))) {
  const full = join(DEBUG, "build", entry);
  if (liveBuildDirs.has(entry)) continue;
  if (freshEnough(statSync(full).mtimeMs)) continue;
  noteDir(full);
}

/* 3c. incremental/ —— 每 crate 只留最新的 K 份（K 来自存活集合；至少 1） */
const incrementalRoot = join(DEBUG, "incremental");
const byCrate = new Map();
for (const entry of readdirSync(incrementalRoot)) {
  const parsed = /^(.*)-([0-9a-z]{13})$/.exec(entry);
  const crate = parsed === null ? entry : parsed[1];
  const list = byCrate.get(crate) ?? [];
  list.push({ entry, mtime: statSync(join(incrementalRoot, entry)).mtimeMs });
  byCrate.set(crate, list);
}
for (const [crate, list] of byCrate) {
  const keep = Math.max(1, liveHashesPerCrate.get(crate)?.size ?? 0);
  list.sort((a, b) => b.mtime - a.mtime);
  for (const [index, item] of list.entries()) {
    if (index < keep) continue;
    if (freshEnough(item.mtime)) continue;
    noteDir(join(incrementalRoot, item.entry));
  }
}

/* ── 4. 报告 / 执行 ───────────────────────────────────────── */

const mb = (bytes) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;

if (plan.files.length === 0 && plan.dirs.length === 0) {
  console.log("✓ target 目录里没有失效产物，无需清理");
  process.exit(0);
}

console.log(
  `${DRY_RUN ? "（dry-run）" : ""}将清理：` +
    `deps/ ${plan.files.length} 个文件、build/ + incremental/ ${plan.dirs.length} 个目录，` +
    `合计 ${mb(plan.bytes)}（判定：不在本次构建图里，且已凉 ≥ ${KEEP_DAYS} 天）`,
);
// 最占空间的几项先亮出来 —— 一眼就能看出有没有误删大件（比如 400MB 的 .lib）
const biggest = [...plan.files, ...plan.dirs]
  .map((path) => ({ path, size: statSync(path).isDirectory() ? dirSize(path) : statSync(path).size }))
  .sort((a, b) => b.size - a.size)
  .slice(0, 5);
if (biggest.length > 0) {
  console.log("  最大的几项（确认一下没有还在用的东西）：");
  for (const item of biggest) {
    console.log(`    ${mb(item.size).padStart(10)}  ${item.path.slice(DEBUG.length + 1)}`);
  }
}
if (has("--verbose")) {
  for (const file of plan.files.slice(0, 40)) console.log(`  - ${file.slice(DEBUG.length + 1)}`);
  for (const dir of plan.dirs.slice(0, 40)) console.log(`  - ${dir.slice(DEBUG.length + 1)}/`);
}

if (DRY_RUN) process.exit(0);

for (const file of plan.files) rmSync(file, { force: true });
for (const dir of plan.dirs) rmSync(dir, { force: true, recursive: true });
console.log(`✓ 已清理 ${plan.files.length + plan.dirs.length} 项，回收约 ${mb(plan.bytes)}`);

/* ── 工具 ─────────────────────────────────────────────────── */

function* readFileLines(path) {
  for (const line of readFileSync(path, "utf8").split("\n")) {
    const trimmed = line.trim();
    if (trimmed !== "") yield trimmed;
  }
}
