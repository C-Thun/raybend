import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { parseVersion } from "../../src/lib/release-plan.ts";
export const RELEASE_SOURCE_FILES = Object.freeze(['package.json', 'Cargo.toml', 'Cargo.lock', 'public/legal/third-party.json']);
export const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
function one(source, regex, replacement, label) {
  const matches = [...source.matchAll(regex)];
  if (matches.length !== 1) throw new Error(`${label} 应恰有一处版本，实际 ${matches.length}`);
  return source.replace(regex, replacement);
}
export function workspaceVersion(text) {
  const block = /^\[workspace\.package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(text)?.[1];
  const matches = [...(block ?? "").matchAll(/^version\s*=\s*"([^"]+)"\s*$/gm)];
  if (matches.length !== 1) throw new Error("找不到唯一 workspace.package.version");
  return matches[0][1];
}
/**
 * 列出 workspace 成员及其包名 / 版本写法（从各成员自己的 `Cargo.toml` 读）。
 *
 * ❗ 同步版本**不能写死包名**：2026-10-12 新增 `raybend-crashpad` 后漏改它的 lock 版本，
 * 2026-10-14 的正式发版被 `cargo metadata --locked` 的
 * 「cannot update the lock file」当场拦下。以后新增成员一次，这里自动跟上。
 */
export function workspaceMembers(cargoText, read, root) {
  const block = /^\[workspace\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(cargoText)?.[1] ?? "";
  const dirs = [...(/^members\s*=\s*\[([\s\S]*?)\]/m.exec(block)?.[1] ?? "").matchAll(/"([^"]+)"/g)].map(m => m[1]);
  if (!dirs.length) throw new Error("Cargo.toml 的 [workspace] members 为空或无法解析");
  return dirs.map(dir => {
    const text = read(join(root, dir, "Cargo.toml"));
    const pkg = /^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(text)?.[1] ?? "";
    const name = /^name\s*=\s*"([^"]+)"\s*$/m.exec(pkg)?.[1];
    if (!name) throw new Error(`workspace 成员 ${dir} 找不到 [package] name`);
    const follows = /^version\.workspace\s*=\s*true\s*$/m.test(pkg) || /^version\s*=\s*\{\s*workspace\s*=\s*true\s*\}$/m.test(pkg);
    return { dir, name, follows };
  });
}
export function versionEdits(root, target, read = path => readFileSync(path, "utf8")) {
  parseVersion(target);
  const packagePath = join(root, "package.json"), cargoPath = join(root, "Cargo.toml"), lockPath = join(root, "Cargo.lock");
  const packageText = read(packagePath), cargoText = read(cargoPath), lockText = read(lockPath);
  let pkg; try { pkg = JSON.parse(packageText); } catch (error) { throw new Error("package.json 不是有效 JSON", { cause: error }); }
  if (typeof pkg?.version !== "string") throw new Error("package.json 缺少字符串 version");
  const current = pkg.version;
  if (workspaceVersion(cargoText) !== current) throw new Error("package.json 与 Cargo workspace 版本不一致，先修复再升版");
  const edits = [{path:packagePath,before:packageText,after:JSON.stringify({...pkg,version:target},null,2)+"\n"}];
  const cargoAfter = one(cargoText, /^(\[workspace\.package\]\s*\n[\s\S]*?^version\s*=\s*")[^"]+("[^\n]*$)/gm, `$1${target}$2`, "Cargo workspace");
  edits.push({path:cargoPath,before:cargoText,after:cargoAfter});
  let lockAfter = lockText;
  for (const {name, follows} of workspaceMembers(cargoText, read, root)) {
    if (!follows) continue; // 自带独立版本号的成员不跟随 workspace 版本。
    const re = new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")[^"]+("\\n)`, "g");
    const match = [...lockAfter.matchAll(re)];
    if (match.length !== 1 || !match[0][0].includes(`version = "${current}"`)) throw new Error(`Cargo.lock 里 ${name} 与产品版本不一致（workspace 成员漏同步？）`);
    lockAfter = one(lockAfter, re, `$1${target}$2`, name);
  }
  edits.push({path:lockPath,before:lockText,after:lockAfter});
  return edits.filter(e=>e.before!==e.after);
}
/** 不覆盖同时发生的编辑；部分写入失败也只恢复已经写入的文件。 */
export function applyVersionEdits(edits, io = {read: p=>readFileSync(p,"utf8"),write:(p,v)=>writeFileSync(p,v)}) {
  for (const e of edits) if (io.read(e.path)!==e.before) throw new Error(`版本文件已被修改：${e.path}`);
  const applied=[];
  const rollback=()=>{
    const conflicts=[];
    for (const e of [...applied].reverse()) {
      if(io.read(e.path)===e.after) io.write(e.path,e.before);
      else conflicts.push(e.path);
    }
    if(conflicts.length) throw new Error(`保留了外部修改，需手工核对版本：${conflicts.join(", ")}`);
  };
  try { for(const e of edits) {if(io.read(e.path)!==e.before)throw new Error(`版本文件已被修改：${e.path}`);io.write(e.path,e.after);applied.push(e);} }
  catch(error) {rollback();throw error;}
  return rollback;
}
