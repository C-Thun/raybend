/**
 * 从**文件头**判断图片 MIME（只给 `Blob` 的 `type` 用；不解码、不碰像素）。
 *
 * # 为什么需要它
 *
 * `thumb_get` / `view_image` 走 Tauri 的**原始字节**通道（`ipc::Response`）——
 * 响应里没有 Content-Type，而 `URL.createObjectURL(new Blob([...], { type }))`
 * 必须给一个 type。以前两处（`thumb-queue.ts` / `viewer/store.ts`）都写死
 * `image/jpeg`，但那是个**错声明**：
 *
 * * 渲染管线现在吐的是 **AVIF**（`thumbnail/render.rs::encode`，人类 2026-09-24 定的缓存格式）；
 * * `view_image` 的 `original` 用途给的是**原文件本身**（JPEG / PNG / TIFF / HEIC…）。
 *
 * 写死任何一个都会在另一条路上撒谎。字节自己是知道格式的 —— 只看头几个字节，
 * 认不出退回 `application/octet-stream`：浏览器对 `<img>` 仍会按内容嗅探，
 * 但至少我们没有替它乱声明。
 *
 * 这是本仓**唯一**一份字节 → MIME 的判断；两个调用点都从这里走
 * （`AGENTS.md` §2.12：同一个能力只允许有一套实现）。
 */

/** 是不是以给定字节序列开头（越界一律 `false`） */
function startsWith(bytes: Uint8Array, pattern: readonly number[]): boolean {
  if (bytes.length < pattern.length) return false;
  for (let index = 0; index < pattern.length; index += 1) {
    if (bytes[index] !== pattern[index]) return false;
  }
  return true;
}

/** `offset` 处是不是给定 ASCII 串（越界一律 `false`） */
function asciiAt(bytes: Uint8Array, offset: number, text: string): boolean {
  if (bytes.length < offset + text.length) return false;
  for (let index = 0; index < text.length; index += 1) {
    if (bytes[offset + index] !== text.charCodeAt(index)) return false;
  }
  return true;
}

/** `offset` 起的 4 个 ASCII 字符（`asciiAt` 已经保证长度时用） */
function asciiBrand(bytes: Uint8Array, offset: number): string {
  return String.fromCharCode(
    bytes[offset] ?? 0,
    bytes[offset + 1] ?? 0,
    bytes[offset + 2] ?? 0,
    bytes[offset + 3] ?? 0,
  );
}

/** ISO-BMFF 的 `ftyp` brand → MIME（认不出返回 `null`）。 */
function isoBrandMime(brand: string): string | null {
  if (brand === "avif" || brand === "avis") return "image/avif";
  if (
    brand === "heic" ||
    brand === "heix" ||
    brand === "heim" ||
    brand === "heis" ||
    brand === "hevc" ||
    brand === "hevx" ||
    brand === "mif1" ||
    brand === "msf1"
  ) {
    return "image/heic";
  }
  return null;
}

/**
 * 按容器魔数判 MIME；认不出（含空输入、截断）返回 `application/octet-stream`，**不抛错**。
 */
export function imageMimeOfBytes(bytes: Uint8Array): string {
  // JPEG：SOI + 第一个 marker（FF D8 FF）
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return "image/jpeg";
  // PNG：8 字节签名
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  // GIF：GIF87a / GIF89a
  if (asciiAt(bytes, 0, "GIF87a") || asciiAt(bytes, 0, "GIF89a")) return "image/gif";
  // WebP：RIFF 容器 + 偏移 8 处的 WEBP
  if (asciiAt(bytes, 0, "RIFF") && asciiAt(bytes, 8, "WEBP")) return "image/webp";
  // TIFF：小端 / 大端两种 magic
  if (startsWith(bytes, [0x49, 0x49, 0x2a, 0x00]) || startsWith(bytes, [0x4d, 0x4d, 0x00, 0x2a])) {
    return "image/tiff";
  }
  // ISO-BMFF（AVIF / HEIC…）：偏移 4 是 "ftyp"，紧跟着 4 字节 brand
  if (asciiAt(bytes, 4, "ftyp")) {
    const mime = isoBrandMime(asciiBrand(bytes, 8));
    if (mime !== null) return mime;
  }
  return "application/octet-stream";
}
