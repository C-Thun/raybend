/**
 * 编辑 store 的载荷口径：`interactive`（拖动中）与「换照片必须清掉它」。
 *
 * 为什么值得单测：`interactive` 决定 Rust 侧算哪一档 —— 拖动中只算预览档、
 * 松手才按缩放补全尺寸（人类 2026-09-24）。它一旦**卡在 `true`** 上，
 * 画面会永远偏软，而这条只有真机拖一下才看得出来（`AGENTS.md` §2.10）。
 */
import assert from "node:assert/strict";
import test from "node:test";

import { batch, createEffect, createRoot } from "solid-js";

import { DEFAULT_EDITOR_PREFS } from "../../lib/editor-prefs.ts";
import { createEditorStore, withTemperatureBaseline } from "./store.ts";

function makeStore(): ReturnType<typeof createEditorStore> {
  return createEditorStore({
    readPrefs: () => ({ ...DEFAULT_EDITOR_PREFS }),
    writePrefs: () => undefined,
  });
}


/* ── 编辑区（area）层（W3/W4/W5）────────────────────────────── */

test("层：新建追加末尾、默认层位置、载荷与退化形状", () => {
  const store = makeStore();
  assert.deepEqual(store.developPayload().areas, [], "没建层 = 空数组（退化形状）");
  assert.equal(store.developPayload().defaultIndex, 0);

  const first = store.createArea();
  const second = store.createArea();
  assert.deepEqual(
    store.layerRows().map((row) => (row.kind === "default" ? "default" : row.area.id)),
    ["default", first, second],
    "新建层追加到末尾（列表自上而下 = 计算顺序）",
  );
  const payload = store.developPayload();
  assert.equal(payload.areas.length, 2);
  assert.equal(payload.defaultIndex, 0, "默认层初始在第 1 位");

  // 把默认层拖到最后：行序与 defaultIndex 一起变
  store.moveAreaRow(0, 2);
  assert.deepEqual(
    store.layerRows().map((row) => (row.kind === "default" ? "default" : row.area.id)),
    [first, second, "default"],
  );
  assert.equal(store.developPayload().defaultIndex, 2);
});

test("层：参数写进当前层，默认层不被污染", () => {
  const store = makeStore();
  const id = store.createArea();
  store.setParam("exposure", 1.25);
  assert.equal(store.paramValue("exposure"), 1.25);
  assert.deepEqual(store.developPayload().values, {}, "默认层没有被动过");
  assert.equal(store.developPayload().areas[0]?.params.exposure, 1.25);

  // 全局阶段参数（局部色调）不能写进非默认层
  store.setParam("dynamicContrast", 40);
  assert.equal(store.developPayload().areas[0]?.params.dynamicContrast, undefined);

  // 换回默认层：值各自独立
  store.selectArea(null);
  assert.equal(store.paramValue("exposure"), 0);
  store.setParam("exposure", 0.5);
  assert.equal(store.developPayload().values.exposure, 0.5);
  assert.equal(store.developPayload().areas[0]?.params.exposure, 1.25);
  store.selectArea(id);
  assert.equal(store.paramValue("exposure"), 1.25);
});

test("层：删除当前层选下一层；默认层删不掉；上限 32", () => {
  const store = makeStore();
  const a = store.createArea();
  const b = store.createArea();
  assert.equal(store.activeAreaId(), b);
  store.removeArea(b);
  assert.equal(store.activeAreaId(), a, "删当前层后选下一层（没有下一层就上一层）");
  store.removeArea(a);
  assert.equal(store.activeAreaId(), null, "只剩默认层时回到默认层");

  for (let index = 0; index < 32; index += 1) assert.ok(store.createArea() > 0);
  assert.equal(store.areasFull(), true);
  assert.equal(store.createArea(), 0, "满 32 层不再新建");
  assert.equal(store.developPayload().areas.length, 32);
});

test("武装不建层：点「新建控制点」到落点之前，层列表里一行都不该多", () => {
  const store = makeStore();
  assert.equal(store.pointArmed(), false);
  assert.equal(store.developPayload().areas.length, 0);

  store.armPointArea();
  assert.equal(store.pointArmed(), true, "按钮进入武装态");
  assert.equal(store.developPayload().areas.length, 0, "武装态**没有层**（2026-11-01 修）");

  // 再点一次 = 取消（按钮是开关）
  store.armPointArea();
  assert.equal(store.pointArmed(), false);
  assert.equal(store.developPayload().areas.length, 0);

  // 落点这一下才建层
  store.armPointArea();
  const id = store.placePointArea({ x: 0.4, y: 0.6, seed: [0.3, 0.1, -0.1] });
  assert.ok(id > 0);
  assert.equal(store.pointArmed(), false, "落点后自动解除武装");
  assert.equal(store.developPayload().areas.length, 1);
  assert.equal(store.activeArea()?.id, id, "新层成为当前层");
});

test("控制点的颜色槽：取已用最少、并列取最小（与 Rust 同一条规则）", () => {
  const store = makeStore();
  const colors: number[] = [];
  for (let index = 0; index < 9; index += 1) {
    store.armPointArea();
    store.placePointArea({ x: 0.5, y: 0.5, seed: [0.5, 0, 0] });
    colors.push(store.activeArea()?.color ?? -1);
  }
  assert.deepEqual(colors, [0, 1, 2, 3, 4, 5, 6, 7, 0], "八色輪完回到下标最小的空闲槽");
});

test("控制点：落点写采样位与种子；容差是 100..1000 的数值域", () => {
  const store = makeStore();
  store.armPointArea();
  const id = store.placePointArea({ x: 0.25, y: 0.75, seed: [0.4, -0.05, 0.02] });
  const area = store.activeArea();
  assert.ok(area !== null && area.type === "point");
  assert.equal(area.x, 0.25);
  assert.equal(area.y, 0.75);
  assert.deepEqual(area.seed, [0.4, -0.05, 0.02]);
  assert.equal(area.tolerance, 300, "初值 300（不是最小容差）");
  assert.equal(area.seedVersion, 1);

  // 拖 handle 改容差：只动 tolerance，种子与采样位不动，并夹在 100..1000
  store.setAreaPoint(id, { tolerance: 640 });
  const after = store.activeArea();
  assert.ok(after !== null && after.type === "point");
  assert.equal(after.tolerance, 640);
  const point = (): Extract<import("../../api/editor.ts").Area, { type: "point" }> => {
    const current = store.activeArea();
    assert.ok(current !== null && current.type === "point");
    return current;
  };
  store.setAreaPoint(id, { tolerance: 5 });
  assert.equal(point().tolerance, 100, "低于下限夹到 100");
  store.setAreaPoint(id, { tolerance: 4000 });
  assert.equal(point().tolerance, 1000, "高于上限夹到 1000");
  assert.deepEqual(point().seed, [0.4, -0.05, 0.02]);
  store.setAreaPoint(id, { tolerance: Number.NaN });
  assert.equal(point().tolerance, 300, "NaN 不能进入载荷 / 落库校验");
});

test("SOOC：不能武装/落点；已有控制点只在载荷里旁路，数据不改", () => {
  const store = makeStore();
  const id = store.placePointArea({ x: 0.3, y: 0.4, seed: [0.4, 0.1, -0.1] });
  assert.ok(id > 0);
  store.setEditBase("sooc");

  store.armPointArea();
  assert.equal(store.pointArmed(), false, "命令面板也不能绕过 SOOC 禁用");
  assert.equal(store.placePointArea({ x: 0.5, y: 0.5, seed: [0.5, 0, 0] }), 0);

  const payload = store.developPayload();
  assert.equal(payload.areas[0]?.enabled, false, "SOOC 载荷中控制点不计算");
  const layer = store.layerRows().find((row) => row.kind === "area");
  assert.ok(layer !== undefined && layer.kind === "area");
  assert.equal(layer.area.enabled, true, "层自身状态不被 SOOC 改写，切回 RAW 可恢复");
});

test("载入层时 params / curves 缺字段也不能炸（Rust 空层就长这样）", () => {
  /*
   * 回归（2026-11-01 真机）：`Area` 的 `params` / `curves` 曾经是
   * `skip_serializing_if = is_empty` —— 空层从 Rust 回来时这两个字段**整个不存在**，
   * `loadDevelop` 里 `Object.entries(area.curves)` 抛
   * `TypeError: Cannot convert undefined or null to object`，整次载入作废。
   * 现象：建过 area、退出去再进来只剩默认一层，右栏底下挂一条红色 TypeError。
   */
  const store = makeStore();
  store.loadDevelop({}, {}, {
    areas: [
      {
        id: 7,
        type: "point",
        name: "天空",
        enabled: true,
        opacity: 1,
        color: 2,
        x: 0.25,
        y: 0.4,
        seed: [0.4, 0.5, 0.5],
        tolerance: 0.15,
        // 故意不给 params / curves：这正是 Rust 空层序列化出来的形状
      } as unknown as import("../../api/editor.ts").Area,
    ],
    defaultIndex: 0,
  });
  const rows = store.layerRows();
  assert.equal(rows.length, 2, "层必须真的读回来了（不是只剩默认层）");
  assert.ok(store.activeArea() === null, "换照片 / 载入后当前层是默认层");
  store.selectArea(7);
  assert.equal(store.developPayload().areas.length, 1);
  const loaded = store.developPayload().areas[0];
  assert.ok(loaded !== undefined && loaded.type === "point");
  assert.ok(Math.abs(loaded.tolerance - 235) < 1e-6,
    "旧 0.15 容差要折算成 100 + 0.15×900 = 235（定稿 / 侧车不再经过 DB load）");
  // 缺字段的层照样能改参数（写进去时按空对象起步）
  store.setParam("exposure", 0.5);
  assert.equal(store.developPayload().areas[0]?.params.exposure, 0.5);
});

test("预设只认默认层；套预设不新建 / 不删除层（W5）", () => {
  const store = makeStore();
  store.loadDevelop({ exposure: 0.5 }, {});
  const id = store.createArea();
  store.setParam("exposure", -1);
  const snapshot = store.presetSnapshot(["tone"]);
  assert.equal(snapshot.tone?.exposure, 0.5, "取的是默认层（不是当前层）");

  const before = store.developPayload().areas.length;
  store.applyPresetSnapshot({ version: 3, tone: { exposure: 0.2 } });
  assert.equal(store.developPayload().areas.length, before, "套预设不动层");
  assert.equal(store.activeAreaId(), id, "当前层也不变");
  assert.equal(store.developPayload().values.exposure, 0.2, "写进默认层");
});

test("自动调整只写默认层（非默认层在编时也一样，W5）", () => {
  const store = makeStore();
  const id = store.createArea();
  store.applyAutoAdjust({
    values: { exposure: 0.3 },
    lensProfile: null,
    lensEnabled: null,
    nrMethod: null,
  });
  assert.equal(store.activeAreaId(), id);
  assert.equal(
    store.developPayload().areas[0]?.params.exposure,
    undefined,
    "自动结果不能写进附加层",
  );
  store.selectArea(null);
  assert.equal(store.paramValue("exposure"), 0.3);
});

test("屏幕打样不污染照片编辑版本或便携载荷",()=> {
  const store=makeStore();
  const before=store.developPayload(); const revision=store.developRev();
  store.setProofEnabled(true); store.setProofWarning(true); store.setProofTarget({kind:"display_p3"});
  store.setAdvancedTab("color");
  assert.equal(store.developRev(),revision);
  assert.deepEqual(store.developPayload(),before);
  store.loadDevelop({}, {}, {color:{process_version:"linear_rec2020_v2",source:{kind:"assumed_srgb"}}});
  const snapshot=store.presetSnapshot(["colorManagement"]);
  assert.deepEqual(snapshot,{version:3,colorManagement:{processVersion:"linear_rec2020_v2",input:"automatic"}});
  store.setColorState({process_version:"linear_rec2020_v2",source:{kind:"assigned_rgb_icc",profile_id:"a".repeat(64)}});
  assert.deepEqual(store.presetSnapshot(["colorManagement"]).colorManagement?.input,{profileId:"a".repeat(64)});
});

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
  store.loadDevelop({}, {}, { lutChain: [{ id: "film", enabled: true, strength: 1 }], lutEnabled: null, asShotK: 5400 });
  assert.equal(store.lutEnabled(), false);
  assert.equal(store.lutEnabledSetting(), null);
  assert.equal(store.asShotTemperature(), 5400);
  assert.deepEqual(store.developPayload().values, {});
  store.applyDevelop({}, {}, { lutChain: [{ id: "film", enabled: true, strength: 1 }], lutEnabled: false, asShotK: 5600 });
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
    lutChain: [{ id: "film", enabled: true, strength: 1 }], lutEnabled: true, lensProfile: "Maker|Lens", lensEnabled: true,
    nrMethod: "high" as const,
    geometry: { rotation: 2, crop: { x: 0.1, y: 0.1, width: 0.8, height: 0.8 } },
  };
  const curve = { rgb: [[0, 0], [0.5, 0.55], [1, 1]] as [number, number][] };
  store.loadDevelop({ exposure: 0.4, temperature: 6000 }, curve, before);
  store.applyDevelop({ contrast: 25 }, {}, {
    sourceBase: "sooc", asShotK: 4800, lutChain: [{ id: "warm", enabled: true, strength: 1 }], lutEnabled: false,
    lensProfile: "Other|Lens", lensEnabled: false, geometry: null,
  });
  assert.equal(store.editBase(), "sooc");
  assert.equal(store.lutEnabledSetting(), false);
  assert.equal(store.developDirty(), true, "applyDevelop 只载入并抬 rev，dirty 由工作区收尾");
  /*
   * 2026-10-27 基准：查看目标（SOOC / RAW / 定稿）不写 latest。
   * 工作区载入后立刻 `markCommitted` —— 少了这一步，下一次任何 confirm 都会把
   * 「看」的状态写进 `develop_stacks`，正是用户报的那个丢失。
   */
  store.markCommitted(store.developRev());
  assert.equal(store.developDirty(), false, "查看目标载入后标记已落库，不写 latest");
  store.loadDevelop({ exposure: 0.4, temperature: 6000 }, curve, before);
  assert.equal(store.editBase(), "raw");
  assert.equal(store.asShotTemperature(), 5100);
  assert.deepEqual(store.developPayload().values, { exposure: 0.4, temperature: 6000 });
  assert.deepEqual(store.developPayload().curves, curve);
  assert.equal(store.baseCurveProfile(), "7");
  assert.deepEqual(store.baseCurvePoints(), before.baseCurvePoints);
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["film"]);
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
  assert.equal(store.lutChain().length, 0);
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

/* ── 预设（审计 2026-10-04 回归）────────────────────────── */

const presetLibrary = (): Parameters<ReturnType<typeof createEditorStore>["setPresetLibrary"]>[0] => ({
  directories: [
    { id: "default", name: "Default", sortOrder: 0, createdAt: 1 },
    { id: "d1", name: "人像", sortOrder: 1, createdAt: 2 },
  ],
  presets: [
    { id: "p1", directoryId: "default", name: "柔和", payload: { version: 1, tone: { exposure: 0.5 } }, createdAt: 1, updatedAt: 1 },
    { id: "p2", directoryId: "d1", name: "夜景", payload: { version: 1, tone: { contrast: 10 } }, createdAt: 2, updatedAt: 2 },
  ],
});

const click = (shiftKey: boolean): MouseEvent => ({ shiftKey } as unknown as MouseEvent);

test("setPresetLibrary 收敛失效选中与折叠记录（删掉的条目不能留在选中态）", () => {
  const store = makeStore();
  store.setPresetLibrary(presetLibrary());
  store.selectPreset("p1", click(false));
  store.selectPreset("p2", click(true));
  store.togglePresetDirectory("d1");
  assert.deepEqual(store.presetSelection(), { kind: "presets", ids: ["p1", "p2"] });

  // 删除 p2 后的整库回写：选中收敛到只剩 p1
  store.setPresetLibrary({
    directories: presetLibrary().directories,
    presets: presetLibrary().presets.filter((preset) => preset.id !== "p2"),
  });
  assert.deepEqual(store.presetSelection(), { kind: "presets", ids: ["p1"] });

  // 选中目录被删 → 回 null；该目录的折叠记录一并清掉（缺省展开）
  store.selectPresetDirectory("d1");
  store.setPresetLibrary({ directories: presetLibrary().directories.filter((d) => d.id !== "d1"), presets: [] });
  assert.equal(store.presetSelection(), null);
  assert.equal(store.presetExpanded("d1"), true);
});

test("applyPresetSnapshot 只抬一次 rev（含镜头组）", () => {
  const store = makeStore();
  const before = store.developRev();
  store.applyPresetSnapshot({
    version: 1,
    lens: { distortion: -12, vignette: 20, profile: "m|l", enabled: true },
  });
  assert.equal(store.developRev(), before + 1, "镜头与数值项写入只抬一次 rev");
  assert.equal(store.paramValue("distortion"), -12);
  assert.equal(store.paramValue("vignette"), 20);
  assert.equal(store.lensProfile(), "m|l");
  assert.equal(store.lensEnabled(), true);
});

test("applyPresetSnapshot：LUT 不在当前库里 → 不动当前 LUT，也不抬 rev", () => {
  const store = makeStore();
  store.setLut("old", true);
  store.setLutCategories([
    { id: "c", name: "分类", entries: [{ id: "old", name: "old.cube", available: true }] },
  ]);
  const rev = store.developRev();
  store.applyPresetSnapshot({
    version: 1,
    lut: { id: "lost", enabled: true, chain: [{ id: "lost", enabled: true, strength: 1 }] },
  });
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["old"]);
  assert.equal(store.developRev(), rev, "整份快照只有丢失的 LUT → 没有任何写入");
});

/*
 * 回归（崔总 2026-10-10 报告“新建预设里切 Lens chip，当前照片像被关了镜头校正”）：
 * 新建预设弹窗里的 chip 只是 `presetSnapshot(groups)` 的**参数** —— 它只读当前状态，
 * 不写回照片。这条测试把“快照是纯读”钉死：任何一组（含 lens）构建快照后，
 * rev / 载荷 / 镜头侧都不许变；同时快照里确实**记录**了当前镜头设置。
 */
test("presetSnapshot 是纯读：勾不勾 Lens 都不动当前照片状态", () => {
  const store = makeStore();
  store.setLensProfile("Maker|12mm");
  store.setLensEnabled(true);
  const before = {
    rev: store.developRev(),
    payload: store.developPayload(),
    profile: store.lensProfile(),
    enabled: store.lensEnabled(),
  };

  const withLens = store.presetSnapshot(["lens"]);
  store.presetSnapshot(["tone", "color", "detail", "lens", "curve", "lut", "colorManagement"]);
  store.presetSnapshot(["tone"]);
  store.presetSnapshot([]);

  assert.equal(store.developRev(), before.rev, "构建快照不抬 rev");
  assert.deepEqual(store.developPayload(), before.payload, "载荷逐字节不变");
  assert.equal(store.lensProfile(), before.profile, "镜头 profile 不变");
  assert.equal(store.lensEnabled(), before.enabled, "镜头开关不变");
  // 创建预设只是把当时的 profile / 开关**记进预设**（apply 才会写回照片）；
  // 镜头组按契约写全字段，所以除两个附加字段外还有数值参数
  assert.equal(withLens.lens?.profile, "Maker|12mm");
  assert.equal(withLens.lens?.enabled, true);
  assert.equal(typeof withLens.lens?.distortion, "number");
  assert.equal(store.presetSnapshot(["tone"]).lens, undefined, "没勾 lens 就不记录该组");
});

/*
 * 2026-10-31 真机缺陷的回归：**在 effect 里调 store 的面板开关**
 * （`left-panel.tsx` 的 `issueFocusTick` → `setLeftPanel("issues")`）。
 *
 * 旧实现 `updateChrome` 先 `produce(chrome())` 读信号、再 `setChrome` 写同一个信号，
 * 于是「读 chrome 的 effect」把自己订阅上了 chrome：写完立刻失效 → 再跑 → 再写……
 * 生产构建没有循环保护，同步递归到爆栈（`RangeError: Maximum call stack size exceeded`）。
 * 爆栈发生在 `finalize()` 的 `setIssueFocusTick()` 那一行，**后面的 createIssue 一行没跑**
 * —— 现象是「点定稿确认什么都不生成，但按钮永久变灰（busy 没复位），连 LUT 开关也失灵」。
 *
 * 判据：effect 只该因为**自己的依赖**变化而重跑，不该被自己写下去的 signal 反激。
 */
test("在 effect 里切左列面板不会自激（定稿确认后爆栈的回归）", () => {
  const store = makeStore();
  let runs = 0;
  const dispose = createRoot((stop) => {
    createEffect(() => {
      runs += 1;
      store.setLeftPanel("issues");
    });
    return stop;
  });
  batch(() => {}); // 触发一次更新刷新，让排队的 effect 真正执行
  dispose();
  assert.equal(runs, 1, `effect 应只执行 1 次，实际 ${runs} 次（写了自己读的信号 = 无限循环）`);
  assert.equal(store.leftPanel(), "issues");
});

/** 同一个坑的另一种入口：`toggleLeft`（toolsbar 左段的两个开关）也不能自激。 */
test("在 effect 里 toggleLeft 不会自激", () => {
  const store = makeStore();
  let runs = 0;
  const dispose = createRoot((stop) => {
    createEffect(() => {
      runs += 1;
      store.toggleLeft("lut");
    });
    return stop;
  });
  batch(() => {}); // 触发一次更新刷新，让排队的 effect 真正执行
  dispose();
  assert.equal(runs, 1, `effect 应只执行 1 次，实际 ${runs} 次`);
});

test("应用层链：点选换底层、圈选加减层、拖动排序、强度与上限", () => {
  const store = makeStore();
  // 空链 → 点一支 = 底层，总开关自动开
  store.selectLutAsBase("a");
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["a"]);
  assert.equal(store.lutEnabled(), true);
  // 点另一支 = 换底层（不新增）
  store.selectLutAsBase("b");
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["b"]);
  // 圈选 = 追加到链尾
  store.toggleLutInChain("c");
  store.toggleLutInChain("d");
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["b", "c", "d"]);
  // 点链里已有的那支 = 移到第 1 层，不重复
  store.selectLutAsBase("d");
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["d", "b", "c"]);
  // 再点圆圈 = 移出
  store.toggleLutInChain("b");
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["d", "c"]);
  // 排序
  store.moveLutLayer(0, 1);
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["c", "d"]);
  // 强度会被夹到 0..1，单层开关与删除
  store.setLutLayerStrength(0, 1.5);
  assert.equal(store.lutChain()[0].strength, 1);
  store.setLutLayerStrength(0, 0.35);
  assert.equal(store.lutChain()[0].strength, 0.35);
  store.setLutLayerEnabled(1, false);
  assert.equal(store.lutChain()[1].enabled, false);
  store.removeLutLayer(0);
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["d"]);
  // 清空 = 总开关自动关
  store.removeLutLayer(0);
  assert.equal(store.lutChain().length, 0);
  assert.equal(store.lutEnabled(), false);
});

test("改一层强度不换其它层的对象身份（否则列表 DOM 会整片重建、拉杆拖动被打断）", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  store.toggleLutInChain("b");
  store.toggleLutInChain("c");
  const before = store.lutChain();
  store.setLutLayerStrength(1, 0.4);
  const after = store.lutChain();
  assert.notEqual(after, before, "链本身是新数组（响应式要靠这个）");
  assert.deepEqual(after.map((layer) => layer.id), ["a", "b", "c"]);
  assert.equal(after[1].strength, 0.4);
  assert.equal(after[0], before[0], "第 1 层没动，必须是同一个对象");
  assert.equal(after[2], before[2], "第 3 层没动，必须是同一个对象");
  assert.notEqual(after[1], before[1], "改动的这一层换新对象");
  // 单层开关同理
  const beforeToggle = store.lutChain();
  store.setLutLayerEnabled(0, false);
  assert.equal(store.lutChain()[2], beforeToggle[2]);
  assert.equal(store.lutChain()[1], beforeToggle[1]);
});

test("应用层链最多 8 层", () => {
  const store = makeStore();
  for (let i = 0; i < 12; i += 1) store.toggleLutInChain(`l${i}`);
  assert.equal(store.lutChain().length, 8);
  assert.deepEqual(store.lutChain().map((layer) => layer.id), ["l0", "l1", "l2", "l3", "l4", "l5", "l6", "l7"]);
});

test("链编辑不会重新打开已关闭的总开关", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  store.setLutEnabled(false);
  assert.equal(store.lutEnabled(), false);
  store.setLutLayerStrength(0, 0.5);
  assert.equal(store.lutEnabled(), false, "调强度不能擅自打开总开关");
  store.moveLutLayer(0, 0);
  assert.equal(store.lutEnabled(), false, "排序不能擅自打开总开关");
  store.toggleLutInChain("b");
  assert.equal(store.lutEnabled(), false, "加层也不能擅自打开总开关");
  store.removeLutLayer(0);
  store.removeLutLayer(0);
  assert.equal(store.lutChain().length, 0);
  store.selectLutAsBase("a");
  assert.equal(store.lutEnabled(), true, "空链新建第一层时才自动开启");
  store.setLutEnabled(false);
  store.selectLutAsBase("b");
  assert.equal(store.lutEnabled(), true, "点新 LUT 换底层仍然是立即应用");
});

test("预设保留多层链与总开关关闭状态", () => {
  const store = makeStore();
  store.setLutCategories([
    { id: "c", name: "分类", entries: [
      { id: "a", name: "a.cube", available: true },
      { id: "b", name: "b.cube", available: true },
    ] },
  ]);
  store.applyPresetSnapshot({
    version: 1,
    lut: {
      id: "a",
      enabled: false,
      chain: [
        { id: "a", enabled: true, strength: 0.65 },
        { id: "b", enabled: false, strength: 0.4 },
      ],
    },
  });
  assert.deepEqual(store.lutChain(), [
    { id: "a", enabled: true, strength: 0.65 },
    { id: "b", enabled: false, strength: 0.4 },
  ]);
  assert.equal(store.lutEnabled(), false, "预设的总开关不能被链自动打开");
});

test("强度与开关联动：拉到 0% 立刻关层，从关闭状态拉回 >0% 立刻开层", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  assert.equal(store.lutChain()[0].enabled, true);

  // 拖到 0% → 该层在**写入的同一步**就关掉（不等松手，强度保留 0 便于再拉回来）
  store.setLutLayerStrength(0, 0);
  assert.equal(store.lutChain()[0].enabled, false, "0% 不允许保持开启");
  assert.equal(store.lutChain()[0].strength, 0);

  // 从关闭状态拉到 40% → 同一步就开启
  store.setLutLayerStrength(0, 0.4);
  assert.equal(store.lutChain()[0].enabled, true);
  assert.equal(store.lutChain()[0].strength, 0.4);

  // 松手（commit）只是把这次手势记成一个撤销节点，状态不变
  store.commitLutLayerStrength(0, 0.4);
  assert.equal(store.lutChain()[0].enabled, true);
  assert.equal(store.lutChain()[0].strength, 0.4);

  // 双击还原 = commit(1)，关着的层也会被开启
  store.setLutLayerEnabled(0, false);
  store.commitLutLayerStrength(0, 1);
  assert.equal(store.lutChain()[0].enabled, true);
});

test("不变式：任何操作都不会留下「开着但强度 0%」的层", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  store.toggleLutInChain("b");
  const broken = (): number[] => store.lutChain()
    .map((layer, index) => (layer.enabled && layer.strength <= 0 ? index : -1))
    .filter((index) => index >= 0);

  // 各种入口轮番来一轮
  store.setLutLayerStrength(0, 0);
  store.commitLutLayerStrength(1, 0);
  store.setLutLayerEnabled(0, true);
  store.setLutLayerEnabled(1, true);
  store.setLutLayerStrength(1, 0.02);
  store.setLutLayerEnabled(1, false);
  store.commitLutLayerStrength(1, 0);
  store.setLutLayerEnabled(0, false);
  assert.deepEqual(broken(), [], "低于 1% 也算 0%：不允许开着却没效果");
});

test("0% 关掉之后点开关：强度直接给回 100%（不存在「开着但 0%」的层）", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  // 拖到 0% 松手 → 该层关闭
  store.commitLutLayerStrength(0, 0);
  assert.equal(store.lutChain()[0].enabled, false);
  assert.equal(store.lutChain()[0].strength, 0);

  // 反向：把 0% 的那层点开 → 开着 + 100%（否则是「开着却什么都没发生」）
  store.setLutLayerEnabled(0, true);
  assert.equal(store.lutChain()[0].enabled, true);
  assert.equal(store.lutChain()[0].strength, 1, "开关打开时 0% 要给回 100%");

  // 非 0% 的层开关照旧：只改开/关，强度一点不动
  store.setLutLayerStrength(0, 0.35);
  store.setLutLayerEnabled(0, false);
  store.setLutLayerEnabled(0, true);
  assert.equal(store.lutChain()[0].strength, 0.35, "非 0% 的层开关不改强度");
  assert.equal(store.lutChain()[0].enabled, true);
});

test("非有限的强度一律忽略，不会被猜成 100%", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  store.setLutLayerStrength(0, 0.4);
  const before = store.lutChain();
  store.setLutLayerStrength(0, Number.NaN);
  store.commitLutLayerStrength(0, Number.NaN);
  assert.equal(store.lutChain()[0].strength, 0.4, "NaN 不改强度");
  assert.equal(store.lutChain()[0].enabled, true, "NaN 不改开关");
  assert.equal(store.lutChain()[0], before[0], "连对象都不换（不留无谓的 rev 变化）");
});

test("强度松手与顶部总开关互不影响：总开关关着时该层照常开关，但不参与渲染", () => {
  const store = makeStore();
  store.selectLutAsBase("a");
  store.setLutEnabled(false);
  assert.equal(store.lutEnabled(), false);

  // 总开关关着：0% 松手照样关该层
  store.commitLutLayerStrength(0, 0);
  assert.equal(store.lutChain()[0].enabled, false);
  assert.equal(store.lutEnabled(), false, "不能因此打开总开关");

  // 总开关关着：>0% 松手照样开该层
  store.commitLutLayerStrength(0, 0.5);
  assert.equal(store.lutChain()[0].enabled, true);
  assert.equal(store.lutEnabled(), false, "也不能因此打开总开关");

  // 总开关开着时同理，且互不牵连
  store.setLutEnabled(true);
  store.commitLutLayerStrength(0, 0);
  assert.equal(store.lutChain()[0].enabled, false);
  assert.equal(store.lutEnabled(), true, "关该层不等于关总开关");
});
