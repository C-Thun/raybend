/** Fixed model build inputs: local resources → verified cache → pinned HTTPS bytes. No model export during builds. */
import { createHash, randomUUID } from "node:crypto";
import { closeSync, copyFileSync, existsSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, rmSync, writeFileSync, writeSync } from "node:fs";
import { join, resolve } from "node:path";
import { inflateRawSync } from "node:zlib";
import { descriptors, verifyAiFile } from "./ai-assets.mjs";
import { acquireReleaseLock } from "./release-mirror.mjs";

function valid(path, file) { try { verifyAiFile(path, file); return true; } catch { return false; } }
function httpsUrl(value) {
  let url;
  try { url = new URL(value); } catch { throw new Error("AI 构建下载地址必须是有效 HTTPS URL"); }
  if (url.protocol !== "https:" || url.username || url.password || url.hash) throw new Error("AI 构建下载地址必须是无内嵌账号/片段的 HTTPS URL");
  return url;
}
function declaration(file) {
  if (!Number.isSafeInteger(file.bytes) || file.bytes < 1 || file.bytes > 256 * 1024 * 1024 || !/^[0-9a-f]{64}$/.test(file.sha256)) throw new Error("AI 下载字节数或摘要无效");
}
async function download(file, url, cache, { fetcher, timeoutMs, log }) {
  declaration(file);
  const target = join(cache, `${file.sha256}.bin`);
  if (valid(target, file)) { log(`✓ AI 构建缓存命中：${file.name}`); return target; }
  url = httpsUrl(url);
  for (let attempt = 1; attempt <= 2; attempt++) {
    const stage = `${target}.stage-${randomUUID()}`;
    let handle, response;
    try {
      const signal = AbortSignal.timeout(timeoutMs);
      let current = url;
      for (let hop = 0; ; hop++) {
        response = await fetcher(current.href, { redirect: "manual", signal, headers: { "accept-encoding": "identity" } });
        if (![301, 302, 303, 307, 308].includes(response.status)) break;
        await response.body?.cancel();
        if (hop >= 5 || !response.headers.get("location")) throw new Error("AI 下载重定向无效或过多");
        current = httpsUrl(new URL(response.headers.get("location"), current).href);
      }
      if (response.status !== 200 || !response.body) throw new Error(`AI 下载 HTTP ${response.status}`);
      const declared = response.headers.get("content-length");
      if (declared !== null && Number(declared) !== file.bytes) throw new Error("AI 下载 Content-Length 与固定字节数不符");
      log(`下载 AI 构建资源：${file.name} · ${Math.ceil(file.bytes / 1048576)} MiB（${attempt}/2）`);
      handle = openSync(stage, "wx", 0o600);
      const hash = createHash("sha256");
      let bytes = 0;
      for await (const chunk of response.body) {
        const buffer = Buffer.from(chunk); bytes += buffer.length;
        if (bytes > file.bytes) throw new Error("AI 下载超过固定字节数");
        for (let offset = 0; offset < buffer.length;) {
          const count = writeSync(handle, buffer, offset, buffer.length - offset);
          if (!count) throw new Error("AI 下载写入被截断");
          offset += count;
        }
        hash.update(buffer);
      }
      if (bytes !== file.bytes || hash.digest("hex") !== file.sha256) throw new Error("AI 下载大小或 SHA-256 不符");
      fsyncSync(handle); closeSync(handle); handle = undefined;
      renameSync(stage, target);
      return target;
    } catch (error) {
      // Do not expose signed URL query parameters or fetch implementation errors.
      if (attempt === 2) throw new Error(`AI 构建资源获取失败：${file.name}；请检查网络/固定地址或提供本地资源`, { cause: error });
      log(`AI 构建资源重试：${file.name}`);
    } finally {
      if (handle !== undefined) closeSync(handle);
      try { await response?.body?.cancel(); } catch { /* an exhausted/locked stream already closed */ }
      rmSync(stage, { force: true });
    }
  }
}

/** Read only named DLL entries from an already SHA-verified small ZIP; never extract arbitrary paths. */
export function runtimeZipFiles(buffer, files) {
  let end = -1;
  for (let offset = buffer.length - 22; offset >= Math.max(0, buffer.length - 65557); offset--) {
    if (buffer.readUInt32LE(offset) === 0x06054b50 && offset + 22 + buffer.readUInt16LE(offset + 20) === buffer.length) { end = offset; break; }
  }
  if (end < 0 || buffer.readUInt16LE(end + 4) !== 0 || buffer.readUInt16LE(end + 6) !== 0) throw new Error("AI CPU ZIP 尾部无效");
  const count = buffer.readUInt16LE(end + 10), size = buffer.readUInt32LE(end + 12), start = buffer.readUInt32LE(end + 16);
  if (count < 1 || count > 4096 || count !== buffer.readUInt16LE(end + 8) || size > 4 * 1024 * 1024 || start + size !== end) throw new Error("AI CPU ZIP 目录无效");
  const wanted = new Map(files.map(file => [`onnxruntime-win-x64-1.28.0/lib/${file.name}`, file])), result = new Map();
  let offset = start;
  for (let entry = 0; entry < count; entry++) {
    if (offset + 46 > end || buffer.readUInt32LE(offset) !== 0x02014b50) throw new Error("AI CPU ZIP 目录被截断");
    const flags = buffer.readUInt16LE(offset + 8), method = buffer.readUInt16LE(offset + 10), compressed = buffer.readUInt32LE(offset + 20), bytes = buffer.readUInt32LE(offset + 24);
    const nameSize = buffer.readUInt16LE(offset + 28), extraSize = buffer.readUInt16LE(offset + 30), commentSize = buffer.readUInt16LE(offset + 32), local = buffer.readUInt32LE(offset + 42);
    const next = offset + 46 + nameSize + extraSize + commentSize;
    if (next > end || buffer.readUInt16LE(offset + 34) !== 0) throw new Error("AI CPU ZIP 条目无效");
    const name = buffer.subarray(offset + 46, offset + 46 + nameSize).toString("utf8"), file = wanted.get(name);
    if (file) {
      declaration(file);
      if (result.has(file.name) || flags & 1 || ![0, 8].includes(method) || bytes !== file.bytes || local + 30 > start || buffer.readUInt32LE(local) !== 0x04034b50) throw new Error("AI CPU ZIP DLL 条目无效");
      const localNameSize = buffer.readUInt16LE(local + 26), localExtraSize = buffer.readUInt16LE(local + 28), body = local + 30 + localNameSize + localExtraSize;
      if (body + compressed > start || buffer.readUInt16LE(local + 6) !== flags || buffer.readUInt16LE(local + 8) !== method || buffer.subarray(local + 30, local + 30 + localNameSize).toString("utf8") !== name) throw new Error("AI CPU ZIP DLL 范围无效");
      const source = buffer.subarray(body, body + compressed);
      const output = method === 0 ? source : inflateRawSync(source, { maxOutputLength: file.bytes });
      if (output.length !== file.bytes || createHash("sha256").update(output).digest("hex") !== file.sha256) throw new Error("AI CPU ZIP DLL 大小或 SHA-256 不符");
      result.set(file.name, output);
    }
    offset = next;
  }
  if (offset !== end || result.size !== files.length) throw new Error("AI CPU ZIP 缺少固定 DLL");
  return result;
}

export async function ensureCpuRuntime({root,env=process.env,runtimeDirectory=env.RAYBEND_AI_RUNTIME_DIR,cacheDirectory=env.RAYBEND_AI_CACHE_DIR,fetcher=globalThis.fetch,timeoutMs=120000,log=console.error}) {
  root=resolve(root);
  if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1||timeoutMs>300000)throw new Error("AI 下载超时设置无效");
  const descriptor=descriptors(root),runtime=descriptor.files.filter(f=>f.kind==='runtime');
  if(runtimeDirectory){for(const f of runtime)verifyAiFile(join(runtimeDirectory,f.name),f);}
  else if(runtime.every(f=>valid(join(f.directory,f.name),f)))return;
  cacheDirectory=resolve(cacheDirectory||join(root,'.cache/ai-build/v1'));mkdirSync(cacheDirectory,{recursive:true});
  const unlock=acquireReleaseLock(join(cacheDirectory,'prepare.lock'),'AI CPU 构建资源');let stage;
  try {
    if(!runtimeDirectory){const archive=descriptor.runtime.archive;declaration(archive);const path=await download({...archive,name:'onnxruntime-win-x64-1.28.0.zip'},archive.url,cacheDirectory,{fetcher,timeoutMs,log});const content=runtimeZipFiles(readFileSync(path),runtime);stage=join(cacheDirectory,`.runtime-${randomUUID()}`);mkdirSync(stage);for(const [name,bytes]of content)writeFileSync(join(stage,name),bytes);runtimeDirectory=stage;}
    for(const f of runtime){const source=join(runtimeDirectory,f.name),target=join(f.directory,f.name);verifyAiFile(source,f);if(source===target||valid(target,f))continue;const temp=`${target}.stage-${randomUUID()}`;try{copyFileSync(source,temp);verifyAiFile(temp,f);renameSync(temp,target);}finally{rmSync(temp,{force:true});}}
  } finally {if(stage)rmSync(stage,{recursive:true,force:true});unlock();}
}
