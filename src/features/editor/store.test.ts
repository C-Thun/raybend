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

test("基础曲线只进 RAW 载荷，切到 SOOC 禁用，换照片清空", () => {
  const store = makeStore();
  store.setBaseCurve("7", [[0, 0], [0.5, 0.6], [1, 1]]);
  assert.equal(store.baseCurveProfile(), "7");
  assert.deepEqual(store.developPayload().baseCurvePoints, [[0, 0], [0.5, 0.6], [1, 1]]);
  store.setEditBase("sooc");
  assert.equal(store.developPayload().baseCurvePoints, null);
  store.setEditBase("raw");
  assert.equal(store.baseCurveProfile(), "7");
  store.loadDevelop({}, {}, { baseCurveProfile: "none", baseCurvePoints: null });
  assert.equal(store.baseCurveProfile(), "none");
  assert.equal(store.developPayload().baseCurvePoints, null);
  store.loadDevelop({}, {}, {});
  assert.equal(store.baseCurveProfile(), null);
});

test("定稿读取保留 LUT 开关的 null/false 区别与拍摄色温", () => {
  const store = makeStore();
  store.loadDevelop({}, {}, { lutId: "film", lutEnabled: null, asShotK: 5400 });
  assert.equal(store.lutEnabled(), false);
  assert.equal(store.lutEnabledSetting(), null);
  assert.equal(store.asShotTemperature(), 5400);
  assert.deepEqual(store.developPayload().values, {});
  store.applyDevelop({}, {}, { lutId: "film", lutEnabled: false, asShotK: 5600 });
  assert.equal(store.lutEnabledSetting(), false);
  assert.equal(store.asShotTemperature(), 5600);
  store.setLutEnabled(true);
  assert.equal(store.lutEnabledSetting(), true);
  store.resetParams();
  assert.equal(store.lutEnabledSetting(), null);
});

test("切换定稿与撤销重读完整 profile：编辑源、曲线、基础曲线、LUT、镜头和裁切同步", () => {
  const store = makeStore();
  const before = { sourceBase: "raw" as const, asShotK: 5100,
    baseCurveProfile: "7", baseCurvePoints: [[0, 0], [0.5, 0.6], [1, 1]] as [number, number][],
    lutId: "film", lutEnabled: true, lensProfile: "Maker|Lens", lensEnabled: true,
    nrMethod: "high" as const,
    geometry: { rotation: 2, crop: { x: 0.1, y: 0.1, width: 0.8, height: 0.8 } },
  };
  const curve = { rgb: [[0, 0], [0.5, 0.55], [1, 1]] as [number, number][] };
  store.loadDevelop({ exposure: 0.4, temperature: 6000 }, curve, before);
  store.applyDevelop({ contrast: 25 }, {}, {
    sourceBase: "sooc", asShotK: 4800, lutId: "warm", lutEnabled: false,
    lensProfile: "Other|Lens", lensEnabled: false, geometry: null,
  });
  assert.equal(store.editBase(), "sooc");
  assert.equal(store.lutEnabledSetting(), false);
  assert.equal(store.developDirty(), true, "切稿也要写入 latest 并进入撤销历史");
  store.loadDevelop({ exposure: 0.4, temperature: 6000 }, curve, before);
  assert.equal(store.editBase(), "raw");
  assert.equal(store.asShotTemperature(), 5100);
  assert.deepEqual(store.developPayload().values, { exposure: 0.4, temperature: 6000 });
  assert.deepEqual(store.developPayload().curves, curve);
  assert.equal(store.baseCurveProfile(), "7");
  assert.deepEqual(store.baseCurvePoints(), before.baseCurvePoints);
  assert.equal(store.lutId(), "film");
  assert.equal(store.lutEnabledSetting(), true);
  assert.equal(store.lensProfile(), "Maker|Lens");
  assert.equal(store.lensEnabled(), true);
  assert.equal(store.nrMethod(), "high");
  assert.deepEqual(store.geometry(), before.geometry);
});

const automatic = () => ({ values: { exposure: 0.5, contrast: 12, saturation: 8, lumaNr: 9 },
  lensProfile: "Maker|AutoLens", lensEnabled: true, nrMethod: "high" as const });
const basePoints: [number,number][] = [[0,0],[0.5,0.65],[1,1]];

test("两层重置：空照片不可用，返回默认值后也不可用", () => {
  const store = makeStore();
  store.setAsShotTemperature(4800);
  assert.equal(store.resetStage(), "none");
  store.setParam("exposure", 1);
  assert.equal(store.resetStage(), "edits");
  store.setParam("exposure", 0);
  assert.equal(store.resetStage(), "none", "按实际值判断，不按 dirty 标志判断");
  store.setNrMethod("fast");
  store.setLensEnabled(true);
  assert.equal(store.resetStage(), "none", "默认档与默认启用开关没有额外调整");
});

test("第一层恢复整套自动结果，保留当前基础曲线；第二层清空并回未选择", () => {
  const store = makeStore();
  store.setAsShotTemperature(4800);
  store.setBaseCurve("5", basePoints);
  store.applyAutoAdjust(automatic());
  assert.equal(store.resetStage(), "automatic");
  store.setParam("exposure", 1);
  store.setParam("saturation", -20);
  store.setParam("lumaNr", 30);
  store.setParam("temperature", 6500);
  store.setLensProfile("Maker|ManualLens");
  store.setLensEnabled(false);
  store.setNrMethod("fast");
  store.setLut("lut", false);
  store.setCurvePoints("rgb", [[0,0],[0.5,0.4],[1,1]]);
  store.setGeometry({ rotation: 10, crop: null, cropRatio: null });
  assert.equal(store.resetStage(), "edits");
  const revision = store.developRev();
  store.resetDevelop("edits");
  assert.equal(store.developRev(), revision + 1);
  assert.equal(store.resetStage(), "automatic");
  assert.deepEqual(store.developPayload().values, automatic().values);
  assert.equal(store.paramValue("temperature"), 4800);
  assert.equal(store.lensProfile(), "Maker|AutoLens");
  assert.equal(store.lensEnabled(), true);
  assert.equal(store.nrMethod(), "high");
  assert.deepEqual(store.developPayload().curves, {});
  assert.equal(store.lutId(), null);
  assert.equal(store.geometry(), null);
  assert.equal(store.baseCurveProfile(), "5");
  assert.deepEqual(store.baseCurvePoints(), basePoints);
  store.resetDevelop("automatic");
  assert.equal(store.resetStage(), "none");
  assert.equal(store.baseCurveProfile(), null);
  assert.equal(store.baseCurvePoints(), null);
  assert.equal(store.autoAdjustBaseline(), null);
  assert.equal(store.lensProfile(), null);
  assert.equal(store.nrMethod(), null);
  assert.deepEqual(store.developPayload().values, {});
});

test("重新读照片或撤销：自动结果恢复，元数据不被手动改写", () => {
  const store = makeStore();
  const baseline = automatic();
  store.loadDevelop({ exposure: 1 }, {}, { autoAdjust: baseline, baseCurveProfile: "5", baseCurvePoints: basePoints });
  baseline.values.exposure = -1;
  assert.equal(store.autoAdjustBaseline()?.values.exposure, 0.5, "加载复制元数据");
  assert.equal(store.resetStage(), "edits");
  store.resetDevelop("edits");
  assert.equal(store.paramValue("exposure"), 0.5);
  store.loadDevelop({}, {}, { baseCurveProfile: "none" });
  assert.equal(store.autoAdjustBaseline(), null, "新照片不串入自动基线");
  assert.equal(store.resetStage(), "automatic");
  store.resetDevelop("automatic");
  assert.equal(store.baseCurveProfile(), null);
});

test("自动调整不收编已有手动值与镜头，第一层只保留实际生成项", () => {
  const store = makeStore();
  store.setParam("lumaNr", 25);
  store.setLensProfile("Maker|ManualLens");
  store.applyAutoAdjust({ values: { exposure: 0.5, bogus: 42, contrast: NaN }, lensProfile: null, lensEnabled: null, nrMethod: null });
  assert.deepEqual(store.autoAdjustBaseline()?.values, { exposure: 0.5 });
  assert.equal(store.paramValue("lumaNr"), 25);
  assert.equal(store.lensProfile(), "Maker|ManualLens");
  assert.equal(store.resetStage(), "edits");
  store.resetDevelop("edits");
  assert.equal(store.paramValue("lumaNr"), 0);
  assert.equal(store.lensProfile(), null);
  assert.equal(store.paramValue("exposure"), 0.5);
});

test("两层重置不切换编辑来源，SOOC 镜头不继承 RAW 自动镜头", () => {
  const store = makeStore();
  store.applyAutoAdjust(automatic());
  store.setEditBase("sooc");
  store.setLensProfile("Maker|SOOC");
  store.resetDevelop("edits");
  assert.equal(store.editBase(), "sooc");
  assert.equal(store.lensProfile(), null);
  assert.equal(store.paramValue("exposure"), 0.5);
  store.resetDevelop("automatic");
  assert.equal(store.editBase(), "sooc");
  assert.equal(store.resetStage(), "none");
});

test("旧档案来源未知：第一层清手动值但保留基础曲线", () => {
  const store = makeStore();
  store.loadDevelop({ exposure: 0.8 }, {}, { baseCurveProfile: "5", baseCurvePoints: basePoints });
  assert.equal(store.resetStage(), "edits");
  store.resetDevelop("edits");
  assert.deepEqual(store.developPayload().values, {});
  assert.equal(store.baseCurveProfile(), "5");
  assert.equal(store.resetStage(), "automatic");
});
