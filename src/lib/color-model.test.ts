import assert from "node:assert/strict";
import test from "node:test";
import { validOutputColor, outputColorChoice, outputColorFromChoice, readColorDefaults } from "./color-model.ts";
import { readPresets, serializePresets, presetErrors, type ExportPreset } from "./export-model.ts";

test("色彩输出身份只接受标准空间或便携内容指纹", () => {
  for (const kind of ["srgb", "display_p3", "adobe_rgb"]) assert.ok(validOutputColor({ kind }));
  assert.ok(validOutputColor({ kind: "custom_rgb_icc", profile_id: "a".repeat(64) }));
  for (const bad of [null, [], "srgb", {}, { kind: "bad" }, { kind: "srgb", monitor: "x" },
    { kind: "custom_rgb_icc", profile_id: "A".repeat(64) }, { kind: "custom_rgb_icc", profile_id: "C:/摄影棚.icc" }]) {
    assert.equal(validOutputColor(bad), false);
  }
});

test("导出预设 v4 保存目标 ICC，旧预设缺字段保持 sRGB，AVIF 广色域明确拒绝", () => {
  const preset: ExportPreset = { id: "p", name: "摄影棚", format: "png", quality: 90,
    maxEdge: 0, sizeMode: "original", percent: 100, directory: "C:/输出", template: ":FILENAME", existingFile: "append",
    outputColor: { kind: "custom_rgb_icc", profile_id: "b".repeat(64) } };
  const encoded = serializePresets([preset]);
  assert.equal(JSON.parse(encoded).version, 4);
  assert.deepEqual(readPresets(encoded), [preset]);
  const { outputColor: _, ...legacy } = preset;
  assert.deepEqual(readPresets(JSON.stringify({ version: 3, presets: [legacy] })), [legacy]);
  assert.ok(presetErrors({ ...preset, format: "avif" }).outputColor);
  assert.deepEqual(presetErrors({ ...preset, format: "avif", outputColor: { kind: "srgb" } }), {});
});

test("默认规则和控件往返不接收显示器路径或静默损坏配置", () => {
  for (const value of [{kind:"srgb"},{kind:"display_p3"},{kind:"adobe_rgb"},{kind:"custom_rgb_icc",profile_id:"b".repeat(64)}] as const) {
    assert.deepEqual(outputColorFromChoice(outputColorChoice(value)),value);
  }
  assert.equal(outputColorChoice(undefined),"srgb");
  for (const invalid of ["", "AdobeRGB", "A".repeat(64), "C:/显示器.icc"]) assert.equal(outputColorFromChoice(invalid),null);
  const defaults={untagged_input:{kind:"require_assignment"},output:{kind:"display_p3"}};
  assert.deepEqual(readColorDefaults(JSON.stringify(defaults)),defaults);
  for (const invalid of [null,"", "null", "[]", JSON.stringify({...defaults,output:{kind:"bad"}}),
    JSON.stringify({...defaults,untagged_input:{kind:"srgb",monitor:"x"}}),JSON.stringify({...defaults,monitor:"local"})]) assert.equal(readColorDefaults(invalid),null);
});
