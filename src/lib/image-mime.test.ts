/**
 * 字节 → MIME 的判断（`image-mime.ts`）。
 *
 * 钉两件事：**认得出真格式**（每种容器的魔数各一条）与**认不出不抛错**
 * （空 / 截断 / 垃圾 —— 界面上最多是声明保守，不能把渲染搞崩）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { imageMimeOfBytes } from "./image-mime.ts";

function bytes(...values: number[]): Uint8Array {
  return Uint8Array.from(values);
}

/** ISO-BMFF：4 字节长度 + "ftyp" + brand + 补到 16 字节 */
function isoBmff(brand: string): Uint8Array {
  const out = new Uint8Array(16);
  out.set([0x00, 0x00, 0x00, 0x20], 0);
  out.set([0x66, 0x74, 0x79, 0x70], 4); // "ftyp"
  out.set([...brand].map((char) => char.charCodeAt(0)), 8);
  return out;
}

test("按容器魔数认出每种格式", () => {
  assert.equal(imageMimeOfBytes(bytes(0xff, 0xd8, 0xff, 0xe0)), "image/jpeg");
  assert.equal(
    imageMimeOfBytes(bytes(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00)),
    "image/png",
  );
  assert.equal(imageMimeOfBytes(bytes(0x47, 0x49, 0x46, 0x38, 0x39, 0x61)), "image/gif");
  assert.equal(imageMimeOfBytes(bytes(0x47, 0x49, 0x46, 0x38, 0x37, 0x61)), "image/gif");
  // RIFF....WEBP
  const webp = bytes(0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50);
  assert.equal(imageMimeOfBytes(webp), "image/webp");
  // TIFF 两种字节序
  assert.equal(imageMimeOfBytes(bytes(0x49, 0x49, 0x2a, 0x00)), "image/tiff");
  assert.equal(imageMimeOfBytes(bytes(0x4d, 0x4d, 0x00, 0x2a)), "image/tiff");
  // ISO-BMFF：AVIF 与 HEIC
  assert.equal(imageMimeOfBytes(isoBmff("avif")), "image/avif");
  assert.equal(imageMimeOfBytes(isoBmff("avis")), "image/avif");
  assert.equal(imageMimeOfBytes(isoBmff("heic")), "image/heic");
  assert.equal(imageMimeOfBytes(isoBmff("mif1")), "image/heic");
});

test("认不出 / 空 / 截断：一律 octet-stream，不抛错", () => {
  assert.equal(imageMimeOfBytes(new Uint8Array(0)), "application/octet-stream");
  assert.equal(imageMimeOfBytes(bytes(0xff)), "application/octet-stream", "截断的 JPEG 头");
  assert.equal(imageMimeOfBytes(bytes(1, 2, 3, 4, 5, 6, 7, 8)), "application/octet-stream");
  // RIFF 但不是 WEBP（例如 WAV）
  const wav = bytes(0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45);
  assert.equal(imageMimeOfBytes(wav), "application/octet-stream");
  // ftyp 但 brand 不认识（例如 mp4）
  assert.equal(imageMimeOfBytes(isoBmff("isom")), "application/octet-stream");
});
