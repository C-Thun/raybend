import { test } from "node:test";
import assert from "node:assert/strict";

import type { AssetItem } from "../../api/types.ts";
import { assetItemExif } from "./from-asset.ts";

/** 造一个字段齐全的列表项；只覆盖本测试关心的字段，其余给保守值 */
function item(overrides: Partial<AssetItem> = {}): AssetItem {
  return {
    id: 1,
    relPath: "photos/2026-08-15/MYP0001.RW2",
    fileName: "MYP0001.RW2",
    ext: "RW2",
    isRaw: true,
    hasRaw: true,
    author: null,
    description: null,
    gpsLat: null,
    gpsLon: null,
    country: null,
    provinceState: null,
    city: null,
    sublocation: null,
    createdMs: null,
    takenAt: 1_700_000_000_000,
    takenAtOffsetMin: 480,
    rating: 0,
    colorLabel: null,
    likeState: null,
    lockLevel: 0,
    cameraMake: "Panasonic",
    cameraModel: "DC-G9",
    lens: "LEICA DG 12-60mm F2.8-4.0",
    focalMm: 12,
    fNumber: 2.8,
    exposureMs: 8,
    iso: 200,
    width: 6000,
    height: 4000,
    orientation: 1,
    sizeBytes: 20_000_000,
    missing: false,
    ...overrides,
  };
}

test("assetItemExif：机型优先用型号，毫秒换成秒，RAW 显示 RAW", () => {
  const exif = assetItemExif(item());
  assert.equal(exif.cameraMake, "Panasonic");
  assert.equal(exif.camera, "DC-G9");
  assert.equal(exif.fNumber, 2.8);
  assert.equal(exif.exposureSeconds, 0.008);
  assert.equal(exif.focalLengthMm, 12);
  assert.equal(exif.iso, 200);
  assert.equal(exif.widthPx, 6000);
  assert.equal(exif.heightPx, 4000);
  assert.equal(exif.format, "RAW");
});

test("assetItemExif：没有型号时保留独立的厂商品牌", () => {
  const exif = assetItemExif(item({ cameraModel: null }));
  assert.equal(exif.cameraMake, "Panasonic");
  assert.equal(exif.camera, undefined);
});

test("assetItemExif：全是 null 的项不产生空组（缺的字段一律 undefined）", () => {
  const exif = assetItemExif(
    item({
      cameraMake: null,
      cameraModel: null,
      lens: null,
      focalMm: null,
      fNumber: null,
      exposureMs: null,
      iso: null,
      width: null,
      height: null,
      ext: "",
      isRaw: false,
    hasRaw: false,
    }),
  );
  assert.equal(exif.camera, undefined);
  assert.equal(exif.lens, undefined);
  assert.equal(exif.focalLengthMm, undefined);
  assert.equal(exif.fNumber, undefined);
  assert.equal(exif.exposureSeconds, undefined);
  assert.equal(exif.iso, undefined);
  assert.equal(exif.widthPx, undefined);
  assert.equal(exif.heightPx, undefined);
  assert.equal(exif.format, undefined, "没有扩展名时不给格式名");
});

test("assetItemExif：位图按扩展名给通用显示名（jpg → JPEG）", () => {
  const exif = assetItemExif(item({ isRaw: false, ext: "jpg" }));
  assert.equal(exif.format, "JPEG");
});

test("assetItemExif：像素尺寸原样搬运，不做方向交换（EXIF 方向已在上游应用）", () => {
  // 竖拍：宽 < 高 —— 转换器不许自作聪明地换过来
  const exif = assetItemExif(item({ width: 4000, height: 6000 }));
  assert.equal(exif.widthPx, 4000);
  assert.equal(exif.heightPx, 6000);
});
