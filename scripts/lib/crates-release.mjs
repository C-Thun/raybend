/**
 * crates.io 同步发布：把核心库 `crates/raybend` 发到 crates.io 的那一步。
 *
 * 设计约束（见 `specs/m5-crates-publish.md`）：
 * 1. **只发稳态版**：预发布（beta / test）不发；`--no-crates` 显式跳过。
 * 2. **幂等**：版本已存在时，本地重新 `cargo package` 并与索引里的 sha256 比对——
 *    一致视为「已发布」跳过，不一致硬报错（版本号在 crates.io 不可覆盖，只能升版）。
 * 3. **不做任何 git 写入、不改版本号**；只读索引、打包、上传。
 * 4. 发布形态由崔总决定（AGENTS.md §2.1）：本模块只在人类显式 `--execute` 的路径里被调用。
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const CRATE_NAME = "raybend";
export const CRATE_PAGE = `https://crates.io/crates/${CRATE_NAME}`;
export const INDEX_BASE = "https://index.crates.io";
const USER_AGENT = "raybend-release (+https://github.com/C-Thun/raybend)";

/** crates.io sparse index 路径规则：1 位 → `1/{n}`，2 位 → `2/{n}`，3 位 → `3/{首字}/{n}`，其余 `{前2}/{次2}/{n}`。 */
export function indexPath(name) {
  const n = String(name).toLowerCase();
  if (n.length === 1) return `1/${n}`;
  if (n.length === 2) return `2/${n}`;
  if (n.length === 3) return `3/${n[0]}/${n}`;
  return `${n.slice(0, 2)}/${n.slice(2, 4)}/${n}`;
}
export function indexUrl(name) { return `${INDEX_BASE}/${indexPath(name)}`; }

/** sparse index 是 JSON-lines；只留判占用与核对需要的字段。 */
export function parseIndex(text) {
  const lines = String(text).split("\n").filter(line => line.trim());
  return lines.map((line, index) => {
    let entry;
    try { entry = JSON.parse(line); }
    catch { throw new Error(`crates.io 索引第 ${index + 1} 行不是 JSON（${line.trim().slice(0, 60)}）：可能是 CDN 错误页`); }
    return { version: entry.vers, checksum: entry.cksum, yanked: entry.yanked === true };
  });
}

/** 这一步该不该发：唯一判据，便于单测（预发布 / --no-crates）。 */
export function crateEligibility({ version, prerelease = false, noCrates = false }) {
  if (prerelease) return { publish: false, reason: `预发布版本不发 crates.io（${CRATE_NAME} ${version}）` };
  if (noCrates) return { publish: false, reason: `--no-crates：跳过 crates.io（${CRATE_NAME} ${version}）` };
  return { publish: true, reason: `发布 ${CRATE_NAME} ${version} 到 crates.io（不可撤回）` };
}

export function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
export function localCratePath(root, version) { return join(root, "target/package", `${CRATE_NAME}-${version}.crate`); }
/** 只用于日志：完整 sha256 太长。 */
const short = value => String(value).slice(0, 12);

/**
 * 用 curl 同步读 sparse index（本脚本全流程是同步的，避免把 async 传染给发布链）。
 * 404 = 该 crate 名还没被占（返回空版本表）；其它状态与传输失败一律抛错，由调用方降级。
 */
export function curlIndexText({ run = execFileSync, ua = USER_AGENT } = {}) {
  return url => {
    let out;
    try {
      out = String(run("curl", ["-sS", "-A", ua, "-w", "\n%{http_code}", url], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 }));
    } catch (error) { throw new Error(`读取 ${url} 失败：${error.message}`); }
    const at = out.lastIndexOf("\n");
    const status = Number(out.slice(at + 1).trim());
    if (status === 404) { const error = new Error(`索引里没有 ${url}（404）`); error.status = 404; throw error; }
    if (status !== 200) { const error = new Error(`索引返回 HTTP ${status}`); error.status = status; throw error; }
    return out.slice(0, at);
  };
}

function readIndex({ readIndexText }) {
  try { return { ok: true, versions: parseIndex(readIndexText(indexUrl(CRATE_NAME))) }; }
  catch (error) {
    // 404 不是错误：说明这个 crate 名还没被占（索引里没有该文件）。
    if (error?.status === 404) return { ok: true, versions: [] };
    return { ok: false, reason: error.message };
  }
}

/**
 * 发布核心库。调用方保证：工作树干净、版本号已由发布流程处理、人类已 `--execute`。
 * @returns {{action:'already-published'|'published', checksum?:string, verified?:boolean, warning?:string}}
 */
export function publishCrate({
  root,
  version,
  run = execFileSync,
  log = console.log,
  readIndexText,
  wait = () => {},
  home = homedir(),
  env = process.env,
  attempts = 10,
} = {}) {
  if (!version) throw new Error("缺少要发布的版本号");
  const read = readIndexText ?? curlIndexText({ run });
  const credentialFile = join(home, ".cargo", "credentials.toml");
  let credentials;
  if (env.CARGO_REGISTRY_TOKEN) credentials = "环境变量 CARGO_REGISTRY_TOKEN";
  else if (existsSync(credentialFile)) credentials = "~/.cargo/credentials.toml";
  if (!credentials) throw new Error(`本机未登录 crates.io：先**不带参数**执行 cargo login（token 从 stdin 读）或设置 CARGO_REGISTRY_TOKEN；本轮未调用 cargo publish（${CRATE_PAGE}）`);

  const before = readIndex({ readIndexText: read });
  const existing = before.ok ? before.versions.find(entry => entry.version === version) : undefined;
  if (existing) {
    if (existing.yanked) throw new Error(`crates.io 上 ${CRATE_NAME} ${version} 已被 yank：版本号不可复用，请升版后再发（${CRATE_PAGE}）`);
    run("cargo", ["package", "-p", CRATE_NAME, "--no-verify"], { cwd: root, encoding: "utf8" });
    const path = localCratePath(root, version);
    if (!existsSync(path)) throw new Error(`本地重打包未产出 ${path}，无法核对已发布内容`);
    const local = sha256(readFileSync(path));
    if (local !== existing.checksum) {
      throw new Error(`crates.io 上已有 ${CRATE_NAME} ${version}，且内容与本地重打包不一致（远端 ${short(existing.checksum)}… ≠ 本地 ${short(local)}…）：版本号不可覆盖，请升版后再发`);
    }
    log(`crates.io：${CRATE_NAME} ${version} 已发布且与本地内容一致（sha256 ${short(local)}…），跳过`);
    return { action: "already-published", checksum: local, verified: true };
  }
  if (!before.ok) log(`⚠ crates.io 索引暂不可读（${before.reason}）；按未发布继续，重复版本会由 cargo 拒绝`);

  log(`crates.io：发布 ${CRATE_NAME} ${version}（${CRATE_PAGE}/${version}，不可撤回；登录凭据：${credentials}）`);
  try {
    run("cargo", ["publish", "-p", CRATE_NAME], { cwd: root, stdio: "inherit", encoding: "utf8" });
  } catch (error) {
    // 索引降级时可能撞上「已发布」：此时回到幂等判据，别把重复发布当成新失败。
    if (!/already (been )?(uploaded|published)|already exists|crate version .* is already/i.test(String(error.stderr ?? "") + error.message)) throw error;
    const now = readIndex({ readIndexText: read });
    const entry = now.ok ? now.versions.find(item => item.version === version) : undefined;
    if (!entry) throw error;
    const path = localCratePath(root, version);
    const local = existsSync(path) ? sha256(readFileSync(path)) : undefined;
    if (local && local !== entry.checksum) throw new Error(`crates.io 上 ${CRATE_NAME} ${version} 已存在但内容不同（远端 ${short(entry.checksum)}… ≠ 本地 ${short(local)}…）：请升版`);
    log(`crates.io：${CRATE_NAME} ${version} 此前已发布（cargo 报重复），内容一致，按已发布处理`);
    return { action: "already-published", checksum: entry.checksum, verified: Boolean(local) };
  }

  const path = localCratePath(root, version);
  const local = existsSync(path) ? sha256(readFileSync(path)) : undefined;
  for (let attempt = 0; attempt < attempts; attempt++) {
    const after = readIndex({ readIndexText: read });
    const entry = after.ok ? after.versions.find(item => item.version === version) : undefined;
    if (entry) {
      if (local && entry.checksum !== local) {
        log(`⚠ crates.io 已收录 ${CRATE_NAME} ${version}，但 checksum 与本地包不一致（远端 ${short(entry.checksum)}… ≠ 本地 ${short(local)}…）：上报成功，请人工核对`);
        return { action: "published", checksum: entry.checksum, verified: false, warning: "checksum-mismatch" };
      }
      log(`crates.io：${CRATE_NAME} ${version} 已进索引，checksum 核对通过（sha256 ${short(entry.checksum)}…）`);
      return { action: "published", checksum: entry.checksum, verified: true };
    }
    if (attempt < attempts - 1) wait();
  }
  log(`⚠ crates.io 已接受上传，但索引 ${attempts} 次查询内尚未出现 ${CRATE_NAME} ${version}（索引有延迟）；稍后自查 ${CRATE_PAGE}/${version}`);
  return { action: "published", checksum: local, verified: false, warning: "index-lag" };
}
