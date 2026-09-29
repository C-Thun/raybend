/** Windows 本地构建输入镜像；Node 内置能力，无 rsync / Windows Node 依赖。 */
import { copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { dirname, join, resolve, sep, win32 } from "node:path";

const MARKER = ".raybend-release-mirror";
const OWNER = "raybend Windows release input mirror v1\n";
const INPUTS = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "package.json", "LICENSE", "THIRD-PARTY-NOTICES.md", "crates", "src-tauri", "src", "public"];
const EXCLUDED = new Set(["target", "node_modules", ".git", ".release", "release-out"]);

/** 锁覆盖整个发行事务，共享 Windows 本地 mirror / target 的各工作区不能并发构建。 */
export function acquireReleaseLock(path) {
  mkdirSync(dirname(path), { recursive: true });
  try {
    writeFileSync(path, JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() }) + "\n", { flag: "wx" });
  } catch (error) {
    if (error.code === "EEXIST") throw new Error(`Windows 发行任务锁已存在：${path}；确认没有打包进程后再手动移除残留锁`);
    throw error;
  }
  return () => rmSync(path);
}

function validatePath(path, windowsDestination) {
  for (const part of path.split("/")) {
    if (!part || part === "." || part === ".." || /[\\<>:"|?*\u0000-\u001f]/.test(part) || /[. ]$/.test(part) ||
        /^(con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)/i.test(part)) {
      throw new Error(`构建输入含 Windows 不支持的路径：${path}`);
    }
  }
  if (win32.join(windowsDestination, path).length >= 260) throw new Error(`构建输入超过 WiX 路径长度限制：${path}；请用 --win-dir 选择更短的构建目录`);
}

function ordinary(path) {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile())) throw new Error(`构建镜像不允许符号链接或特殊文件：${path}`);
  return stat;
}

/** 先校验全部输入，再同步；固定目录允许 Cargo 复用缓存。仅删除带归属标记的镜像内容。 */
export function syncReleaseMirror({ root, frontendRoot, destination, windowsDestination = "" }) {
  root = resolve(root); frontendRoot = resolve(frontendRoot); destination = resolve(destination);
  for (const source of [root, frontendRoot]) {
    if (destination === source || destination.startsWith(source + sep) || source.startsWith(destination + sep)) {
      throw new Error("构建镜像与源码/前端目录不能重叠");
    }
  }
  const files = new Map(), directories = new Set(), names = new Map();
  const collect = (source, path, frontend = false) => {
    validatePath(path, windowsDestination);
    const folded = path.normalize("NFC").toLowerCase();
    if (names.has(folded)) throw new Error(`构建输入在 Windows 下重名：${names.get(folded)} / ${path}`);
    names.set(folded, path);
    const stat = ordinary(source);
    if (stat.isDirectory()) {
      directories.add(path);
      for (const name of readdirSync(source).sort()) {
        if (!frontend && (EXCLUDED.has(name) || name.endsWith(":Zone.Identifier"))) continue;
        collect(join(source, name), `${path}/${name}`, frontend);
      }
    } else files.set(path, { source, stat });
  };
  for (const input of INPUTS) collect(join(root, input), input);
  if (existsSync(join(root, ".cargo"))) collect(join(root, ".cargo"), ".cargo");
  collect(frontendRoot, "dist", true);
  if (!files.has("dist/index.html") || !files.has("dist/raybend-build.json")) throw new Error("构建镜像缺本轮前端 index.html / raybend-build.json");

  const marker = join(destination, MARKER);
  if (existsSync(destination)) {
    if (!ordinary(destination).isDirectory() || !existsSync(marker) || !ordinary(marker).isFile() || readFileSync(marker, "utf8") !== OWNER) {
      throw new Error(`拒绝覆盖未归属的构建镜像目录：${destination}`);
    }
  } else {
    mkdirSync(dirname(destination), { recursive: true });
    mkdirSync(destination);
    writeFileSync(marker, OWNER, { flag: "wx" });
  }

  const prune = (dir, prefix = "") => {
    for (const name of readdirSync(dir)) {
      if (!prefix && name === MARKER) continue;
      const path = prefix ? `${prefix}/${name}` : name, full = join(dir, name), stat = ordinary(full);
      if (stat.isDirectory() && directories.has(path)) prune(full, path);
      else if (!(stat.isFile() && files.has(path))) rmSync(full, { recursive: stat.isDirectory() });
    }
  };
  prune(destination);
  for (const dir of directories) mkdirSync(join(destination, dir), { recursive: true });
  let copied = 0;
  for (const [path, { source, stat }] of files) {
    const dest = join(destination, path);
    if (existsSync(dest) && readFileSync(source).equals(readFileSync(dest))) continue;
    copyFileSync(source, dest);
    utimesSync(dest, stat.atime, stat.mtime);
    copied++;
  }
  return { files: files.size, copied };
}
