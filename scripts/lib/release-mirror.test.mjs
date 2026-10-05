import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { acquireReleaseLock, syncReleaseMirror } from "./release-mirror.mjs";

function fixture() {
  const base = mkdtempSync(join(tmpdir(), "raybend-镜像-")), root = join(base, "源码 空格"), frontendRoot = join(root, "dist/test-build"), destination = join(base, "mirror"), windowsDestination = "D:\\构建目录\\source";
  mkdirSync(frontendRoot, { recursive: true });
  for (const name of ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "package.json", "LICENSE", "THIRD-PARTY-NOTICES.md"]) writeFileSync(join(root, name), name);
  for (const dir of ["crates/core/assets", "src-tauri/icons", "src/api", "public/legal"]) mkdirSync(join(root, dir), { recursive: true });
  writeFileSync(join(root, "src/api/dto-contract.json"), "中文 contract");
  writeFileSync(join(root, "crates/core/assets/照片.webp"), "resource");
  writeFileSync(join(frontendRoot, "index.html"), "frontend");
  writeFileSync(join(frontendRoot, "raybend-build.json"), "{}");
  return { root, frontendRoot, destination, sync: () => syncReleaseMirror({ root, frontendRoot, destination, windowsDestination }), dispose: () => rmSync(base, { recursive: true, force: true }) };
}

test("镜像包含源码、跨层 DTO 和前端；不带依赖/缓存；重复同步保留时间戳", () => {
  const f = fixture();
  try {
    for (const dir of ["node_modules", "target", "src-tauri/target", ".git", "website"]) { mkdirSync(join(f.root, dir), { recursive: true }); writeFileSync(join(f.root, dir, "skip"), "skip"); }
    const first = f.sync(); assert.equal(first.files, first.copied);
    assert.equal(readFileSync(join(f.destination, "src/api/dto-contract.json"), "utf8"), "中文 contract");
    assert.equal(readFileSync(join(f.destination, "crates/core/assets/照片.webp"), "utf8"), "resource");
    assert.equal(readFileSync(join(f.destination, "dist/index.html"), "utf8"), "frontend");
    for (const dir of ["node_modules", "target", "src-tauri/target", ".git", "website", "dist/test-build"]) assert.equal(existsSync(join(f.destination, dir)), false);
    const dest = join(f.destination, "src/api/dto-contract.json");
    utimesSync(dest, 1000, 1000); assert.equal(f.sync().copied, 0); assert.equal(statSync(dest).mtimeMs, 1000000);
    // 同长度、同时间戳仍检测字节变化。
    writeFileSync(join(f.frontendRoot, "index.html"), "FRONTEND");
    assert.equal(f.sync().copied, 1);
    assert.equal(readFileSync(join(f.destination, "dist/index.html"), "utf8"), "FRONTEND");
  } finally { f.dispose(); }
});

test("删除过期文件、大小写改名与目录/文件互换；可选 Cargo 配置不残留", () => {
  const f = fixture();
  try {
    mkdirSync(join(f.root, ".cargo")); writeFileSync(join(f.root, ".cargo/config.toml"), "config");
    writeFileSync(join(f.root, "src/old.rs"), "old"); f.sync();
    rmSync(join(f.root, ".cargo"), { recursive: true }); rmSync(join(f.root, "src/old.rs"));
    rmSync(join(f.root, "src/api"), { recursive: true }); writeFileSync(join(f.root, "src/API"), "file");
    f.sync(); assert.equal(existsSync(join(f.destination, "src/old.rs")), false);
    assert.equal(existsSync(join(f.destination, ".cargo")), false);
    assert.equal(readFileSync(join(f.destination, "src/API"), "utf8"), "file");
    rmSync(join(f.root, "src/API")); mkdirSync(join(f.root, "src/API"));
    f.sync(); assert.ok(statSync(join(f.destination, "src/API")).isDirectory());
  } finally { f.dispose(); }
});

test("缺输入或前端清单、大小写冲突、保留名与过长路径在写镜像前拒绝", () => {
  for (const mutation of [
    f => rmSync(join(f.root, "LICENSE")),
    f => rmSync(join(f.frontendRoot, "raybend-build.json")),
    f => { writeFileSync(join(f.root, "src/A.rs"), "a"); writeFileSync(join(f.root, "src/a.rs"), "b"); },
    f => writeFileSync(join(f.root, "src/CON.txt"), "bad"),
    f => writeFileSync(join(f.root, "src/name."), "bad"),
    f => writeFileSync(join(f.root, "src/a:b"), "bad"),
    f => writeFileSync(join(f.root, "src", "a".repeat(250)), "long"),
    f => symlinkSync(join(f.root, "LICENSE"), join(f.root, "src/link")),
  ]) {
    const f = fixture(); try { mutation(f); assert.throws(f.sync); assert.equal(existsSync(f.destination), false); } finally { f.dispose(); }
  }
});

test("拒绝未归属目录、目标符号链接与源目标重叠；不动外部文件", () => {
  const f = fixture();
  try {
    mkdirSync(f.destination); writeFileSync(join(f.destination, "keep"), "keep");
    assert.throws(f.sync, /未归属/); assert.equal(readFileSync(join(f.destination, "keep"), "utf8"), "keep");
    rmSync(f.destination, { recursive: true }); f.sync();
    symlinkSync(f.root, join(f.destination, "link")); assert.throws(f.sync, /符号链接/);
    assert.equal(readFileSync(join(f.root, "LICENSE"), "utf8"), "LICENSE");
    assert.throws(() => syncReleaseMirror({ ...f, destination: f.root }), /重叠/);
    assert.throws(() => syncReleaseMirror({ ...f, destination: join(f.root, "mirror") }), /重叠/);
  } finally { f.dispose(); }
});

test("独占锁阻止并发，释放后可重入；不抢残留锁", () => {
  const f = fixture();
  try {
    const path = `${f.destination}.lock`, unlock = acquireReleaseLock(path);
    assert.throws(() => acquireReleaseLock(path), /锁已存在/);
    assert.equal(JSON.parse(readFileSync(path, "utf8")).pid, process.pid);
    unlock(); acquireReleaseLock(path)(); assert.equal(existsSync(path), false);
    writeFileSync(path, "interrupted"); assert.throws(() => acquireReleaseLock(path), /锁已存在/);
    assert.equal(readFileSync(path, "utf8"), "interrupted");
  } finally { f.dispose(); }
});

test("AI 镜像只消费本次冻结输入，关闭时清除旧包；软件侧残留大文件不入镜像", () => {
 const f=fixture();try{
  const legacy=join(f.root,'crates/raybend/assets/ai/old');mkdirSync(legacy,{recursive:true});writeFileSync(join(legacy,'encoder.onnx'),'old model');writeFileSync(join(legacy,'runtime.dll'),'old dll');
  const snapshot=join(f.root,'.release/ai-input');mkdirSync(join(snapshot,'ai-model'),{recursive:true});mkdirSync(join(snapshot,'ai-runtime'));writeFileSync(join(snapshot,'ai-model/encoder.onnx'),'frozen model');writeFileSync(join(snapshot,'ai-runtime/runtime.dll'),'frozen dll');
  syncReleaseMirror({...f,aiSnapshot:snapshot});assert.equal(readFileSync(join(f.destination,'.ai-bundle/ai-model/encoder.onnx'),'utf8'),'frozen model');assert.equal(existsSync(join(f.destination,'crates/raybend/assets/ai/old/encoder.onnx')),false);
  f.sync();assert.equal(existsSync(join(f.destination,'.ai-bundle')),false);
  syncReleaseMirror({...f,aiSnapshot:snapshot});assert.equal(readFileSync(join(f.destination,'.ai-bundle/ai-runtime/runtime.dll'),'utf8'),'frozen dll');
 }finally{f.dispose();}
});
