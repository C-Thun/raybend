/**
 * EXIF 格式化与分组的单元测试。
 *
 * 这一层的特点是「输入来自相机文件，什么都可能有」：缺字段、0、NaN、
 * 快门是 1/3 秒这种奇葩值、机型字符串里带前后空格。所以覆盖重点放在**退化输入**上：
 * 界面上宁可少一项，也绝不能出现 `ƒ/undefined` 或 `1/NaN s`。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  formatAperture,
  formatDimensions,
  formatFocalLength,
  formatIso,
  formatMegapixels,
  formatShutter,
  formatText,
  MAX_SHUTTER_DENOMINATOR,
} from "./exif-format.ts";
import { EXIF_PART_SEPARATOR, groupExif, hasExif } from "./grouping.ts";

/* ─── 格式化 ───────────────────────────────────────────── */

test("焦距：整数不带小数，非整数保留一位", () => {
  assert.equal(formatFocalLength(12), "12mm");
  assert.equal(formatFocalLength(10.5), "10.5mm");
  assert.equal(formatFocalLength(24.04), "24mm");
});

test("光圈：用 ƒ（U+0192）而不是普通 f，且去掉多余的 0", () => {
  assert.equal(formatAperture(2.8), "ƒ/2.8");
  assert.equal(formatAperture(4), "ƒ/4");
  assert.equal(formatAperture(1.4), "ƒ/1.4");
});

test("快门：短于 1 秒用 1/N 写法，长于等于 1 秒用 Ns", () => {
  assert.equal(formatShutter(0.008), "1/125s");
  assert.equal(formatShutter(1 / 60), "1/60s");
  assert.equal(formatShutter(0.333), "1/3s", "1/3 秒这种非整值要能收住");
  assert.equal(formatShutter(2), "2s");
  assert.equal(formatShutter(1.5), "1.5s");
});

test("快门：极端值不产出 Infinity / NaN 写法", () => {
  assert.equal(formatShutter(0), undefined);
  assert.equal(formatShutter(-1), undefined);
  assert.equal(formatShutter(Number.NaN), undefined);
  assert.equal(
    formatShutter(1e-9),
    undefined,
    "元数据里的垃圾值不该变成 1/1000000000s",
  );
  assert.equal(
    formatShutter(1 / (MAX_SHUTTER_DENOMINATOR + 1)),
    undefined,
    "超过快门分母上限一律当作没有值",
  );
});

test("快门：真实上限之内的快門都能显示（不要误杀高速快门）", () => {
  assert.equal(formatShutter(1 / 8000), "1/8000s");
  assert.equal(
    formatShutter(1 / MAX_SHUTTER_DENOMINATOR),
    `1/${MAX_SHUTTER_DENOMINATOR}s`,
  );
});

test("感光度：加 ISO 前缀并取整", () => {
  assert.equal(formatIso(200), "ISO 200");
  assert.equal(formatIso(100.4), "ISO 100");
});

test("尺寸与像素数：乘号两侧留空格；像素数保留一位小数", () => {
  assert.equal(formatDimensions(5184, 3888), "5184 × 3888");
  assert.equal(formatMegapixels(5184, 3888), "20.2 MP");
  assert.equal(formatMegapixels(6000, 4000), "24 MP", "整数不该显示成 24.0 MP");
});

test("非法值一律当「没有值」：NaN / 0 / 负数 / 缺参", () => {
  for (const bad of [Number.NaN, 0, -1]) {
    assert.equal(formatFocalLength(bad), undefined);
    assert.equal(formatAperture(bad), undefined);
    assert.equal(formatIso(bad), undefined);
  }
  assert.equal(
    formatDimensions(1920, undefined),
    undefined,
    "只有一边不算尺寸",
  );
  assert.equal(formatMegapixels(undefined, 1080), undefined);
  assert.equal(formatDimensions(Number.POSITIVE_INFINITY, 100), undefined);
});

test("文本字段：去空白，空串当作没有值", () => {
  assert.equal(formatText("  NIKON Z 7II  "), "NIKON Z 7II");
  assert.equal(formatText(""), undefined);
  assert.equal(formatText("   "), undefined);
  assert.equal(formatText(undefined), undefined);
});

/* ─── 分组 ─────────────────────────────────────────────── */

test("完整数据：品牌机型 / 镜头 / 曝光 / 文件四组顺序固定", () => {
  const groups = groupExif({
    cameraMake: "Panasonic",
    camera: "DC-G9",
    lens: "LEICA DG 12-60mm F2.8-4.0",
    focalLengthMm: 12,
    fNumber: 2.8,
    exposureSeconds: 0.008,
    iso: 200,
    widthPx: 5184,
    heightPx: 3888,
    format: "RAW",
  });

  assert.deepEqual(
    groups.map((group) => group.id),
    ["camera", "lens", "exposure", "file"],
  );
  assert.deepEqual(
    groups[2].parts.map((part) => part.text),
    ["12mm", "ƒ/2.8", "1/125s", "ISO 200"],
  );
  assert.deepEqual(
    groups[3].parts.map((part) => part.text),
    ["5184 × 3888", "20.2 MP", "RAW"],
  );
});

test("只有机型时，强调项是机型", () => {
  const groups = groupExif({ camera: "X-T5" });
  assert.equal(groups.length, 1);
  assert.deepEqual(groups[0].parts, [{ text: "X-T5", emphasis: true }]);
});

test("没有机型但有镜头：镜头提为强调项（否则整组都是灰的）", () => {
  const groups = groupExif({ lens: "NIKKOR Z 24-70mm" });
  assert.equal(groups[0].id, "lens");
  assert.deepEqual(groups[0].parts, [
    { text: "NIKKOR Z 24-70mm", labelKey: "exif.lens", emphasis: true },
  ]);
});

test("空组被丢掉：只有曝光数据时只返回一组", () => {
  const groups = groupExif({ iso: 400 });
  assert.deepEqual(
    groups.map((group) => group.id),
    ["exposure"],
  );
  assert.deepEqual(groups[0].parts, [
    { text: "ISO 400", labelKey: "exif.iso" },
  ]);
});

test("没有数据 / 全是空值 → 空数组（视图走空态）", () => {
  assert.deepEqual(groupExif(null), []);
  assert.deepEqual(groupExif(undefined), []);
  assert.deepEqual(groupExif({}), []);
  assert.deepEqual(groupExif({ camera: "  ", iso: 0, widthPx: 100 }), []);
  assert.equal(hasExif({}), false);
  assert.equal(hasExif({ camera: "X-T5" }), true);
});

test("复制文本带字段名，用间隔点连接（粘出去能看懂）", () => {
  const labels = {
    "exif.focal": "焦距",
    "exif.aperture": "光圈",
    "exif.iso": "感光度",
    "exif.lens": "镜头",
  } as Record<string, string>;
  const translate = (key: string) => labels[key] ?? key;

  const groups = groupExif(
    { camera: "DC-G9", lens: "LEICA", focalLengthMm: 12, fNumber: 2.8 },
    translate as never,
  );

  assert.equal(
    groups[2].copyText,
    ["焦距 12mm", "光圈 ƒ/2.8"].join(EXIF_PART_SEPARATOR),
  );
  assert.equal(groups[0].copyText, "DC-G9");
  assert.equal(groups[1].copyText, "镜头 LEICA");
});

test("未注入翻译函数时，复制文本里是 key 名（开发期能一眼看出漏传）", () => {
  const [group] = groupExif({ iso: 200 });
  assert.equal(group.copyText, "exif.iso ISO 200");
});

test("品牌与机型成一组，镜头独立且缺任一字段仍可显示", () => {
  const groups = groupExif({ cameraMake: " Panasonic ", camera: " DC-G9 ", lens: " 12-60 " });
  assert.deepEqual(groups.map((group) => group.id), ["camera", "lens"]);
  assert.deepEqual(groups[0].parts.map((part) => part.text), ["Panasonic", "DC-G9"]);
  assert.equal(groups[0].copyText, "Panasonic · DC-G9");
  assert.equal(groups[1].copyText, "exif.lens 12-60");
  assert.deepEqual(groupExif({ cameraMake: "OLYMPUS" }).map((group) => group.id), ["camera"]);
});

test("相机型号自带厂商前缀时不重复展示品牌", () => {
  const canon = groupExif({ cameraMake: "Canon", camera: "Canon EOS R5" });
  assert.deepEqual(canon[0].parts.map((part) => part.text), ["Canon", "EOS R5"]);
  const nikon = groupExif({ cameraMake: "NIKON CORPORATION", camera: "NIKON Z 7II" });
  assert.deepEqual(nikon[0].parts.map((part) => part.text), ["NIKON CORPORATION", "Z 7II"]);
  assert.deepEqual(groupExif({ cameraMake: "Sony", camera: "Sony" })[0].parts.map((part) => part.text), ["Sony"]);
});
