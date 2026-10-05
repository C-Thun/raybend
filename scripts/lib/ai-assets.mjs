/** Fixed CPU/model build inputs. Generated binaries stay outside Git; no model download URL is invented. */
import { createHash, randomUUID } from "node:crypto";
import { closeSync, copyFileSync, existsSync, fstatSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, renameSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

const digest = bytes => createHash("sha256").update(bytes).digest("hex");
export function verifyAiFile(path, file) {
  declaration(file);
  if (!existsSync(path)) throw new Error(`缺少 AI 资源 ${file.name}；模型请用 pnpm ai:use / ai:export 登记，CPU 运行库请用 pnpm ai:prepare 准备`);
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size !== file.bytes)
    throw new Error(`AI 资源大小或 SHA-256 不符：${file.name}`);
  const handle = openSync(path, "r"), hash = createHash("sha256"), buffer = Buffer.alloc(1024 * 1024);
  try {
    if (!fstatSync(handle).isFile() || fstatSync(handle).size !== file.bytes) throw new Error(`AI 资源大小或 SHA-256 不符：${file.name}`);
    let bytes = 0, count;
    while ((count = readSync(handle, buffer, 0, buffer.length, null)) > 0) {
      bytes += count;
      if (bytes > file.bytes) throw new Error(`AI 资源大小或 SHA-256 不符：${file.name}`);
      hash.update(buffer.subarray(0, count));
    }
    if (bytes !== file.bytes || hash.digest("hex") !== file.sha256) throw new Error(`AI 资源大小或 SHA-256 不符：${file.name}`);
  } finally { closeSync(handle); }
}
function declaration(file) {
  if (!/^[a-zA-Z0-9_.-]+$/.test(file.name) || file.name === "." || file.name === ".." || !Number.isSafeInteger(file.bytes) || file.bytes < 1 || file.bytes > 256 * 1024 * 1024 || !/^[0-9a-f]{64}$/.test(file.sha256))
    throw new Error("AI 资源声明无效");
}
export function descriptors(root) {
  const model = join(root, "crates/raybend/assets/ai/tinyclip-v1");
  const runtime = join(root, "crates/raybend/assets/ai/ort-win-x64");
  for (const path of [join(model,"manifest.json"), join(runtime,"runtime.json")]) {
    if (!existsSync(path)) throw new Error(`缺少 AI 构建元数据：${path}`);
  }
  const manifest = JSON.parse(readFileSync(join(model, "manifest.json"), "utf8"));
  const info = JSON.parse(readFileSync(join(runtime, "runtime.json"), "utf8"));
  if (manifest.format !== 2 || manifest.thresholds_calibrated !== true || manifest.ort_version !== "1.28.0" || info.version !== "1.28.0" || info.platform !== "win-x64") throw new Error("AI 资源版本不符");
  if (manifest.files.map(f => f.name).sort().join(",") !== "LICENSE.txt,classes.json,image_encoder.onnx" || info.files.map(f => f.name).sort().join(",") !== "onnxruntime.dll,onnxruntime_providers_shared.dll") throw new Error("AI 资源文件清单不符");
  const files = [...manifest.files.map(f => ({...f, directory:model, kind:"model"})), ...info.files.map(f => ({...f, directory:runtime, kind:"runtime"}))];
  files.forEach(declaration);
  return { files, manifestSha256:digest(readFileSync(join(model,"manifest.json"))), manifest, runtime:info };
}
export function checkAiAssets(root, resourceDirectory) {
  const descriptor = descriptors(root);
  if (resourceDirectory) {
    const expected = {'ai-model':['manifest.json','image_encoder.onnx','classes.json','LICENSE.txt'], 'ai-runtime':['runtime.json','onnxruntime.dll','onnxruntime_providers_shared.dll','LICENSE.txt','ThirdPartyNotices.txt']};
    for (const [dir,names] of Object.entries(expected)) {
      const path=join(resourceDirectory,dir);if(lstatSync(path).isSymbolicLink()||readdirSync(path).sort().join(',')!==names.sort().join(','))throw new Error('AI 构建资源目录含未知文件或链接');
    }
    const actual = join(resourceDirectory,"ai-model","manifest.json");
    if (digest(readFileSync(actual)) !== descriptor.manifestSha256) throw new Error("Windows AI 模型 manifest 过期");
  }
  for (const file of descriptor.files) verifyAiFile(resourceDirectory ? join(resourceDirectory, file.kind === "model" ? "ai-model" : "ai-runtime",file.name) : join(file.directory,file.name), file);
  for (const name of ["LICENSE.txt","ThirdPartyNotices.txt"]) {
    const source = readFileSync(join(root,"crates/raybend/assets/ai/ort-win-x64",name));
    if (resourceDirectory && !source.equals(readFileSync(join(resourceDirectory,"ai-runtime",name)))) throw new Error(`CPU 运行库许可过期：${name}`);
  }
  return descriptor;
}
/** Validate all incoming bytes before changing any destination; each file is switched atomically. */
export function prepareAiAssets({root, modelDirectory, runtimeDirectory}) {
  root = resolve(root);
  const descriptor = descriptors(root), incoming = [];
  for (const file of descriptor.files) {
    const supplied = file.kind === "model" ? file.name === "image_encoder.onnx" ? modelDirectory : undefined : runtimeDirectory;
    const target = join(file.directory,file.name), source = supplied ? resolve(supplied,file.name) : target;
    verifyAiFile(source,file);
    if (source !== resolve(target)) incoming.push({source,target,file});
  }
  for (const {source,target,file} of incoming) {
    if (existsSync(target)) {
      try {verifyAiFile(target,file);continue;} catch { /* repair a stale/corrupt generated build input */ }
    }
    mkdirSync(file.directory,{recursive:true});
    const stage = `${target}.stage-${randomUUID()}`;
    try {copyFileSync(source,stage);verifyAiFile(stage,file);renameSync(stage,target);} finally {rmSync(stage,{force:true});}
  }
  return checkAiAssets(root);
}
