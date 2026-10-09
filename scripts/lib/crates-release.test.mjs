import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { crateEligibility, indexUrl, indexPath, localCratePath, parseIndex, publishCrate, sha256 } from "./crates-release.mjs";

const PACKED = Buffer.from("synthetic crate bytes, not a real tarball");
const line = (version, checksum, yanked = false) => JSON.stringify({ name: "raybend", vers: version, cksum: checksum, deps: [], yanked });

/** 离线夹具：run 只认 cargo package / publish；索引由 before/after 两段文本模拟（含 404 与传输失败）。 */
function fixture({ before = "missing", after = "missing", packageBytes = PACKED, publishError, credentials = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "raybend-crates-"));
  mkdirSync(join(root, "target/package"), { recursive: true });
  mkdirSync(join(root, "home/.cargo"), { recursive: true });
  if (credentials) writeFileSync(join(root, "home/.cargo/credentials.toml"), '[registry]\ntoken = "synthetic"\n');
  const state = { calls: [], logs: [], reads: 0, before, after };
  const run = (cmd, args) => {
    state.calls.push([cmd, ...args]);
    const write = () => writeFileSync(localCratePath(root, "1.2.3"), packageBytes);
    if (cmd === "cargo" && args[0] === "package") { write(); return ""; }
    if (cmd === "cargo" && args[0] === "publish") { if (publishError) throw publishError; write(); return ""; }
    throw new Error(`unexpected synthetic command: ${cmd} ${args.join(" ")}`);
  };
  const readIndexText = () => {
    const source = state.reads++ === 0 ? state.before : state.after;
    if (source === "unreadable") throw new Error("curl 传输失败");
    if (source === "missing") { const error = new Error("索引里没有 raybend（404）"); error.status = 404; throw error; }
    return source;
  };
  return {
    root, state, readIndexText, run,
    call: (extra = {}) => publishCrate({ root, version: "1.2.3", run, readIndexText, wait: () => {}, home: join(root, "home"), env: {}, log: message => state.logs.push(String(message)), ...extra }),
    cargo: () => state.calls.filter(c => c[0] === "cargo").map(c => c.slice(0, 2).join(" ")),
    dispose: () => rmSync(root, { recursive: true, force: true }),
  };
}

test("sparse index 路径规则与解析", () => {
  assert.equal(indexPath("a"), "1/a");
  assert.equal(indexPath("ab"), "2/ab");
  assert.equal(indexPath("abc"), "3/a/abc");
  assert.equal(indexPath("raybend"), "ra/yb/raybend");
  assert.equal(indexPath("RaYbEnD"), "ra/yb/raybend");
  assert.equal(indexUrl("raybend"), "https://index.crates.io/ra/yb/raybend");
  assert.deepEqual(parseIndex(`${line("0.1.0", "a".repeat(64))}\n\n${line("0.1.1", "b".repeat(64), true)}\n`), [
    { version: "0.1.0", checksum: "a".repeat(64), yanked: false },
    { version: "0.1.1", checksum: "b".repeat(64), yanked: true },
  ]);
  assert.deepEqual(parseIndex(""), []);
  assert.throws(() => parseIndex("<html>CDN 错误页</html>"), /第 1 行不是 JSON/);
  assert.equal(parseIndex(`${line("0.1.0", "a".repeat(64))}\n`)[0].checksum.length, 64);
});

test("该不该发：稳态发，预发布与 --no-crates 不发", () => {
  assert.equal(crateEligibility({ version: "1.2.3" }).publish, true);
  assert.match(crateEligibility({ version: "1.2.3" }).reason, /不可撤回/);
  assert.equal(crateEligibility({ version: "1.2.3-beta.1", prerelease: true }).publish, false);
  assert.match(crateEligibility({ version: "1.2.3-beta.1", prerelease: true }).reason, /预发布版本不发/);
  assert.equal(crateEligibility({ version: "1.2.3", noCrates: true }).publish, false);
  assert.match(crateEligibility({ version: "1.2.3", noCrates: true }).reason, /--no-crates/);
  assert.equal(crateEligibility({ version: "1.2.3-beta.1", prerelease: true, noCrates: true }).publish, false);
});

test("未发布：只调 cargo publish，并用索引 checksum 核对本地包", () => {
  const f = fixture({ before: "missing", after: line("1.2.3", sha256(PACKED)) });
  try {
    const result = f.call();
    assert.deepEqual(f.cargo(), ["cargo publish"]);
    assert.deepEqual(result, { action: "published", checksum: sha256(PACKED), verified: true });
    assert.ok(!f.state.calls.some(c => c[1] === "package"));
  } finally { f.dispose(); }
});

test("已发布且内容一致：重打包比对后跳过，不重复上传", () => {
  const index = line("1.2.3", sha256(PACKED));
  const f = fixture({ before: index, after: index });
  try {
    const result = f.call();
    assert.deepEqual(f.cargo(), ["cargo package"]);
    assert.equal(result.action, "already-published");
    assert.equal(result.verified, true);
    assert.match(f.state.logs.join("\n"), /已发布且与本地内容一致/);
  } finally { f.dispose(); }
});

test("已发布但内容不同：硬报错且不上传（版本号不可覆盖）", () => {
  const f = fixture({ before: line("1.2.3", "f".repeat(64)), after: line("1.2.3", "f".repeat(64)) });
  try {
    assert.throws(() => f.call(), /内容与本地重打包不一致/);
    assert.deepEqual(f.cargo(), ["cargo package"]);
  } finally { f.dispose(); }
});

test("已 yank 的版本：直接报错，不做任何 cargo 动作", () => {
  const f = fixture({ before: line("1.2.3", sha256(PACKED), true) });
  try {
    assert.throws(() => f.call(), /已被 yank/);
    assert.deepEqual(f.cargo(), []);
  } finally { f.dispose(); }
});

test("索引不可读时降级：仍然发布，只警告", () => {
  const f = fixture({ before: "unreadable", after: line("1.2.3", sha256(PACKED)) });
  try {
    const result = f.call();
    assert.deepEqual(f.cargo(), ["cargo publish"]);
    assert.equal(result.verified, true);
    assert.match(f.state.logs.join("\n"), /⚠ crates.io 索引暂不可读/);
  } finally { f.dispose(); }
});

test("未登录 crates.io：明确报错，且不调用 cargo", () => {
  const f = fixture({ credentials: false });
  try {
    assert.throws(() => f.call(), /未登录 crates.io：先\*\*不带参数\*\*执行 cargo login/);
    assert.deepEqual(f.cargo(), []);
  } finally { f.dispose(); }
});

test("索引延迟：上传成功但尚未进索引只警告，不当作失败", () => {
  const f = fixture({ before: "missing", after: "missing" });
  try {
    const result = f.call({ attempts: 3 });
    assert.deepEqual(f.cargo(), ["cargo publish"]);
    assert.equal(result.action, "published");
    assert.equal(result.verified, false);
    assert.equal(result.warning, "index-lag");
    assert.match(f.state.logs.join("\n"), /索引 3 次查询内尚未出现/);
  } finally { f.dispose(); }
});

test("cargo 报「已上传」且索引内容一致：按已发布处理，不把重跑当失败", () => {
  const publishError = Object.assign(new Error("api errors: crate version `1.2.3` is already uploaded"), { stderr: "" });
  const f = fixture({ before: "unreadable", after: line("1.2.3", sha256(PACKED)), publishError });
  try {
    const result = f.call();
    assert.equal(result.action, "already-published");
    assert.match(f.state.logs.join("\n"), /此前已发布/);
  } finally { f.dispose(); }
});
