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
export function versionEdits(root, target, read = path => readFileSync(path, "utf8")) {
  parseVersion(target);
  const packagePath = join(root, "package.json"), cargoPath = join(root, "Cargo.toml"), lockPath = join(root, "Cargo.lock");
  const packageText = read(packagePath), cargoText = read(cargoPath), lockText = read(lockPath);
  const pkg = JSON.parse(packageText), current = pkg.version;
  if (workspaceVersion(cargoText) !== current) throw new Error("package.json 与 Cargo workspace 版本不一致，先修复再升版");
  const edits = [{path:packagePath,before:packageText,after:JSON.stringify({...pkg,version:target},null,2)+"\n"}];
  const cargoAfter = one(cargoText, /^(\[workspace\.package\]\s*\n[\s\S]*?^version\s*=\s*")[^"]+("[^\n]*$)/gm, `$1${target}$2`, "Cargo workspace");
  edits.push({path:cargoPath,before:cargoText,after:cargoAfter});
  let lockAfter = lockText;
  for (const name of ["raybend","raybend-desktop"]) {
    const re = new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")[^"]+("\\n)`,"g");
    const match = [...lockAfter.matchAll(re)];
    if (match.length !== 1 || !match[0][0].includes(`version = "${current}"`)) throw new Error(`Cargo.lock ${name} 与产品版本不一致`);
    lockAfter = one(lockAfter,re,`$1${target}$2`,name);
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
