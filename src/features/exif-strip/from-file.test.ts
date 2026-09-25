/**
 * `FileExif` → `ExifData` 的映射测试。
 *
 * 这里的错会表现为「EXIF 区永远是空的」或者「格式显示成 RW2 而不是 RAW」——
 * 都是不容易一眼看出的问题，所以按字段钉一遍。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { FileExif } from "../../api/types.ts";
import { formatLabel, toExifData } from "./from-file.ts";

function file(overrides: Partial<FileExif> = {}): FileExif {
  return {
    tags: [],
    cameraMake: null,
    cameraModel: null,
    lens: null,
    software: null,
    gpsLat: null,
    gpsLon: null,
    datetimeRaw: null,
    focalMm: null,
    fNumber: null,
    exposureMs: null,
    exposureBiasEv: null,
    iso: null,
    width: null,
    height: null,
    orientation: null,
    takenAtMs: null,
    takenAtSource: null,
    takenAtOffsetMin: null,
    ext: null,
    kind: "image",
    ...overrides,
  };
}

test("品牌与机型分别保留，由分组层合成 EasyCopy", () => {
  assert.equal(
    toExifData(file({ cameraMake: "Panasonic", cameraModel: "DC-G9" })).camera,
    "DC-G9",
  );
  assert.equal(toExifData(file({ cameraMake: "Panasonic" })).cameraMake, "Panasonic");
  assert.equal(toExifData(file({ cameraMake: "Panasonic" })).camera, undefined);
  assert.equal(toExifData(file()).camera, undefined, "都没有就是 undefined");
});

test("快门时间：毫秒 → 秒（界面按秒排版）", () => {
  assert.equal(toExifData(file({ exposureMs: 8 })).exposureSeconds, 0.008);
  assert.equal(toExifData(file({ exposureMs: 1000 })).exposureSeconds, 1);
  assert.equal(toExifData(file()).exposureSeconds, undefined);
});

test("镜头 / 焦距 / 光圈 / 感光度 / 尺寸：直接透传，空值变 undefined", () => {
  const full = toExifData(
    file({
      lens: "LEICA DG 12-60mm F2.8-4.0",
      focalMm: 12,
      fNumber: 2.8,
      iso: 200,
      width: 5184,
      height: 3888,
    }),
  );
  assert.deepEqual(full, {
    cameraMake: undefined,
    camera: undefined,
    lens: "LEICA DG 12-60mm F2.8-4.0",
    focalLengthMm: 12,
    fNumber: 2.8,
    exposureSeconds: undefined,
    iso: 200,
    widthPx: 5184,
    heightPx: 3888,
    format: undefined,
  });
  assert.equal(toExifData(file()).lens, undefined);
});

test("格式：RAW 一律显示 RAW；jpg/jpeg → JPEG；其它大写", () => {
  assert.equal(formatLabel(file({ kind: "raw", ext: "rw2" })), "RAW");
  assert.equal(formatLabel(file({ kind: "raw", ext: "orf" })), "RAW");
  assert.equal(formatLabel(file({ ext: "jpg" })), "JPEG");
  assert.equal(formatLabel(file({ ext: "JPG" })), "JPEG");
  assert.equal(formatLabel(file({ ext: "jpeg" })), "JPEG");
  assert.equal(formatLabel(file({ ext: "png" })), "PNG");
  assert.equal(formatLabel(file({ ext: "tif" })), "TIFF");
  assert.equal(formatLabel(file({ ext: "heic" })), "HEIF");
  assert.equal(formatLabel(file({ ext: "webp" })), "WEBP");
  assert.equal(formatLabel(file()), undefined);
  assert.equal(
    formatLabel(file({ ext: "   " })),
    undefined,
    "只有空白的扩展名不算",
  );
});

test("kind 是 raw 但扩展名奇怪：仍然算 RAW", () => {
  assert.equal(formatLabel(file({ kind: "raw", ext: null })), "RAW");
});
