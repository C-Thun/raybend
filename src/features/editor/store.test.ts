/**
 * 编辑 store 的载荷口径：`interactive`（拖动中）与「换照片必须清掉它」。
 *
 * 为什么值得单测：`interactive` 决定 Rust 侧算哪一档 —— 拖动中只算预览档、
 * 松手才按缩放补全尺寸（人类 2026-09-24）。它一旦**卡在 `true`** 上，
 * 画面会永远偏软，而这条只有真机拖一下才看得出来（`AGENTS.md` §2.10）。
 */
import assert from "node:assert/strict";
import test from "node:test";

import { DEFAULT_EDITOR_PREFS } from "../../lib/editor-prefs.ts";
import { createEditorStore, withTemperatureBaseline } from "./store.ts";

function makeStore(): ReturnType<typeof createEditorStore> {
  return createEditorStore({
    readPrefs: () => ({ ...DEFAULT_EDITOR_PREFS }),
    writePrefs: () => undefined,
  });
}

test("载荷默认不在拖动中；按下 / 松手跟着走", () => {
  const store = makeStore();
  assert.equal(store.developPayload().interactive, false, "默认不在拖");
  store.beginParamDrag();
  assert.equal(store.paramDragging(), true);
  assert.equal(store.developPayload().interactive, true, "按下之后是拖动中");
  store.endParamDrag();
  assert.equal(store.developPayload().interactive, false, "松手之后不再是");
});

test("换照片要把「拖动中」清掉（否则画面永远偏软）", () => {
  const store = makeStore();
  store.beginParamDrag();
  store.loadDevelop({ exposure: 0.5 }, {});
  assert.equal(store.paramDragging(), false, "换照片后不许还挂在拖动中");
  assert.equal(store.developPayload().interactive, false);
});

test("载荷只装与基线不同的项（`interactive` 不掺进 values）", () => {
  const store = makeStore();
  assert.deepEqual(store.developPayload().values, {}, "什么都没动 → 空对象");
  store.setParam("exposure", 0.5);
  const payload = store.developPayload();
  assert.deepEqual(payload.values, { exposure: 0.5 });
  assert.equal("interactive" in payload.values, false, "标志是载荷的字段，不是参数");
});

test("SOOC/RAW 共用调整参数，但镜头配置各保留一份", () => {
  const store = makeStore();
  store.setParam("exposure", 0.5);
  store.setLensProfile("RawMaker|RawLens");
  store.setLensEnabled(true);
  store.setEditBase("sooc");
  assert.equal(store.paramValue("exposure"), 0.5);
  assert.equal(store.lensProfile(), null);
  assert.equal(store.lensEnabled(), null);
  store.setLensProfile("SoocMaker|SoocLens");
  store.setLensEnabled(false);
  store.setEditBase("raw");
  assert.equal(store.lensProfile(), "RawMaker|RawLens");
  assert.equal(store.lensEnabled(), true);
  assert.equal(store.developPayload().values.exposure, 0.5);
  store.setEditBase("sooc");
  assert.equal(store.lensProfile(), "SoocMaker|SoocLens");
  assert.equal(store.lensEnabled(), false);
  assert.equal(store.developDirty(), true, "切换 latest 源需要重新落库");
});

test("读回 latest 只恢复其源侧镜头；换照片清空两侧", () => {
  const store = makeStore();
  store.loadDevelop({ contrast: 20 }, {}, {
    sourceBase: "sooc", lensProfile: "Lens|SOOC", lensEnabled: false,
  });
  assert.equal(store.editBase(), "sooc");
  assert.equal(store.lensProfile(), "Lens|SOOC");
  store.setEditBase("raw");
  assert.equal(store.lensProfile(), null);
  store.loadDevelop({}, {}, {});
  store.setEditBase("sooc");
  assert.equal(store.lensProfile(), null);
  assert.deepEqual(store.developPayload().values, {});
});

test("绝对色温跟随 as-shot 初始化；显式改动保留；重置回拍摄值", () => {
  const store = makeStore();
  store.loadDevelop({}, {});
  assert.equal(store.paramValue("temperature"), 6250);
  store.setAsShotTemperature(4850);
  assert.equal(store.paramValue("temperature"), 4850);
  assert.deepEqual(store.developPayload().values, {});
  store.setParam("temperature", 6250);
  store.setAsShotTemperature(5100);
  assert.equal(store.paramValue("temperature"), 6250);
  store.resetParam("temperature");
  assert.equal(store.paramValue("temperature"), 5100);
  store.setParam("temperature", 7000);
  store.resetParams();
  assert.equal(store.paramValue("temperature"), 5100);
  assert.deepEqual(store.developPayload().values, {});
});

test("相同拍摄色温保留参数对象身份，真实变化只更新色温", () => {
  const initial = { exposure: 0.5, temperature: 4850 };
  assert.strictEqual(withTemperatureBaseline(initial, 4850), initial);
  const changed = withTemperatureBaseline(initial, 5100);
  assert.notStrictEqual(changed, initial);
  assert.deepEqual(changed, { exposure: 0.5, temperature: 5100 });
  assert.deepEqual(initial, { exposure: 0.5, temperature: 4850 });
});

test("单根拉杆复原使用对应基准：静态、镜头相对量、旋转", () => {
  const store = makeStore();
  store.setParam("exposure", 0.75);
  store.setParam("distortion", -22);
  store.setParam("vignette", 14);
  store.setParam("chromatic", 9);
  store.setAngle(35);
  for (const id of ["exposure", "distortion", "vignette", "chromatic"]) {
    store.resetParam(id);
    assert.equal(store.paramValue(id), 0, `${id} 回到零点`);
  }
  store.resetAngle();
  assert.equal(store.angle(), 0);
  assert.deepEqual(store.developPayload().values, {});
});

test("新镜头通道与暗角范围沿用保存、重置和各自基准", () => {
  const store = makeStore();
  assert.equal(store.paramValue("vignetteRange"), 50);
  store.setParam("chromaticBlue", -23);
  store.setParam("vignetteRange", 80);
  assert.equal(store.developPayload().values["chromaticBlue"], -23);
  assert.equal(store.developPayload().values["vignetteRange"], 80);
  store.resetParam("vignetteRange");
  assert.equal(store.paramValue("vignetteRange"), 50);
  assert.equal(store.developPayload().values["vignetteRange"], undefined);
  store.resetParam("chromaticBlue");
  assert.deepEqual(store.developPayload().values, {});
});


test("宽高输入立即进入自定义，重新选预设后再次输入仍切回自定义", () => {
  const store = makeStore();
  store.setCropRatioId("3:2");
  store.setCropSize(5, 4);
  assert.equal(store.cropRatioId(), "custom");
  assert.equal(store.cropRatio().ratio, 1.25);
  store.flipCropRatio();
  assert.equal(store.cropRatio().ratio, 0.8);
  store.unlinkCropRatio();
  assert.equal(store.cropRatio().ratio, null);
  store.setCropSize(6000, 4000);
  assert.equal(store.cropRatioId(), "custom");
  assert.equal(store.cropRatio().ratio, 1.5);
});

test("自定义比例拒绝零、负数、非有限值与超限比例，保留最近有效草稿", () => {
  const store = makeStore();
  store.setCropRatioId("free");
  for (const [width, height] of [[0,2],[-1,2],[NaN,2],[Infinity,2],[1,0],[1,Infinity],[101,1],[1,101]]) {
    store.setCropSize(width!, height!);
    assert.equal(store.cropRatioId(), "custom");
    assert.equal(store.cropRatio().ratio, 1.5);
  }
  for (const [w,h] of [[1,100],[100,1],[0.5,0.25]]) {
    store.setCropSize(w!,h!);
    assert.equal(store.cropRatio().ratio, w! / h!);
  }
});


test("确认裁切的比例随照片恢复，取消草稿后重进回到确认值", () => {
  const store = makeStore();
  const saved = {
    rotation: 0,
    crop: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 },
    cropRatio: { id: "custom", width: 5, height: 4 },
  };
  store.loadDevelop({}, {}, { geometry: saved });
  assert.equal(store.cropRatioId(), "custom");
  assert.equal(store.cropWidth(), 5);
  assert.equal(store.cropHeight(), 4);
  store.toggleTool("crop");
  store.setCropSize(7, 5);
  store.closeTool();
  store.toggleTool("crop");
  assert.equal(store.cropRatioId(), "custom");
  assert.equal(store.cropWidth(), 5);
  assert.equal(store.cropHeight(), 4);
  store.closeTool();
  store.loadDevelop({}, {}, {});
  assert.equal(store.cropRatioId(), "free", "另一张照片不能继承前一张的比例");
  store.loadDevelop({}, {}, { geometry: saved });
  assert.equal(store.cropRatioId(), "custom", "重读编辑栈能恢复比例");
  store.setGeometry({ ...saved, cropRatio: { id: "4:3", width: 8, height: 5 } });
  assert.equal(store.cropRatioId(), "4:3", "确认结果立即成为下次进入的比例");
  assert.equal(store.cropWidth(), 8);
  assert.equal(store.cropHeight(), 5);
  store.loadDevelop({}, {}, { geometry: { rotation: 0, crop: saved.crop } });
  assert.equal(store.cropRatioId(), "free", "旧记录没有面板配置时保持已存裁切框，使用自由模式");
  assert.deepEqual(store.geometry()?.crop, saved.crop);
});
