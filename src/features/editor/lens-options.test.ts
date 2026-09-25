import assert from "node:assert/strict";
import test from "node:test";
import type { LensMatch, LensProfile } from "../../api/types.ts";
import { appliedLensProfile, chosenLensProfile, searchLensProfiles, suggestedLensProfiles } from "./lens-options.ts";

const lens = (key: string, min: number, max: number): LensProfile => ({
  key, maker: "松下", model: key, focalMin: min, focalMax: max, rectilinear: true,
});
const profiles = [lens("12-60mm", 12, 60), lens("12-35mm", 12, 35), lens("20mm", 20, 20), lens("100-400mm", 100, 400)];
const match: LensMatch = {
  warnings: [], ready: true, lensName: "12-60mm", focalMm: 24, detected: profiles[0]!, candidates: profiles,
};

test("镜头推荐只列相近焦段且变焦类型相同的配置", () => {
  assert.deepEqual(suggestedLensProfiles(match).map((profile) => profile.key), ["12-60mm", "12-35mm"]);
  assert.deepEqual(suggestedLensProfiles({ ...match, focalMm: null }).map((profile) => profile.key), ["12-60mm"]);
});

test("镜头搜索覆盖厂商中文、型号和空查询", () => {
  assert.equal(searchLensProfiles(match, "松下").length, 4);
  assert.deepEqual(searchLensProfiles(match, "100-400").map((profile) => profile.key), ["100-400mm"]);
  assert.equal(searchLensProfiles(match, "不存在").length, 0);
  assert.equal(searchLensProfiles(match, "  ").length, 4);
});

const catalog: LensMatch = { ...match, candidates: [
  { ...lens("canon", 24, 70), maker: "Canon", model: "EF 24-70mm f/2.8L II USM" },
  { ...lens("leica", 12, 60), maker: "Panasonic", model: "LEICA DG 12-60/F2.8-4.0" },
  { ...lens("lumix", 12, 60), maker: "Panasonic", model: "Lumix G Vario 12-60mm f/3.5-5.6" },
  { ...lens("olympus", 12, 45), maker: "Olympus", model: "OLYMPUS OM 12-45mm F4.0" },
  { ...lens("nikon", 24, 70), maker: "Nikon", model: "Nikkor Z 24-70mm f/2.8 S" },
] };
test("品牌加焦段支持中文、别名、单位、全角和不同横线", () => {
  for (const query of ["松下 12-60", "LUMIX 12mm–60mm", "Ｐａｎａｓｏｎｉｃ １２—６０", "12 60 松下", "松下 12.0-60.00mm"]) {
    assert.deepEqual(searchLensProfiles(catalog, query).map((p) => p.key), ["leica", "lumix"], query);
  }
  for (const brand of ["奥巴", "奥林巴斯", "OM System", "Olympus"]) {
    assert.deepEqual(searchLensProfiles(catalog, `${brand} 12-45`).map((p) => p.key), ["olympus"]);
  }
  assert.deepEqual(searchLensProfiles(catalog, "佳能 24-70").map((p) => p.key), ["canon"]);
  assert.deepEqual(searchLensProfiles(catalog, "尼康 24-70 2.8").map((p) => p.key), ["nikon"]);
  assert.equal(searchLensProfiles(catalog, "松下 2-6").length, 0, "数字不得匹配焦段子串");
});
test("匹配结果只是推荐，只有具体选择才应用；空状态不是加载中", () => {
  assert.equal(appliedLensProfile(match, null, true).profile, null);
  assert.equal(appliedLensProfile(match, "20mm", true).mode, "manual");
  assert.equal(appliedLensProfile(match, null, false).mode, "disabled");
  assert.equal(appliedLensProfile(null, null, true).mode, "none");
  assert.equal(appliedLensProfile(match, "gone", true).mode, "none");
  assert.equal(appliedLensProfile(match, "none", true).profile, null);
  assert.equal(chosenLensProfile(null, "Panasonic|LEICA DG 12-60/F2.8-4.0")?.maker, "Panasonic");
  assert.equal(chosenLensProfile(null, "none"), null);
  assert.equal(chosenLensProfile(null, "broken"), null);
  assert.equal(chosenLensProfile(null, "品牌|中文镜头")?.model, "中文镜头");
});
