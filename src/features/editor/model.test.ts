/**
 * 编辑模块的纯逻辑测试：参数表 / 空态判定 / 胶片带适配。
 *
 * 这三块都是「组件里只能靠肉眼验」的规则，抽出来就是为了钉住边界
 * （`AGENTS.md` §2.10：边界与非法值都要覆盖，不只测正常路径）。
 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  CROP_RATIOS,
  PARAMS,
  PARAM_DEFAULTS,
  cropRatioLabel,
  defaultParams,
  formatLimit,
  formatParamValue,
  invertRatio,
  isParamDirty,
  paramsInGroup,
  type ParamSpec,
} from "./params.ts";
import {
  createEditorStrip,
  editorEmptyIcon,
  editorEmptyKind,
  editorEmptyOffersImport,
} from "./source.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import type { MessageKey } from "../../i18n/index.ts";

test("参数表：id 唯一、分组齐全、范围合法", () => {
  const ids = PARAMS.map((p) => p.id);
  assert.equal(new Set(ids).size, ids.length, "id 不能重复（它同时是 IPC 字段名）");
  for (const spec of PARAMS) {
    assert.ok(spec.min < spec.max, `${spec.id}: min 必须小于 max`);
    assert.ok(spec.step > 0, `${spec.id}: 步长必须为正`);
    // 极值文案只在 `limits` 为真时才显示；给了就应该是**字符串**（界面上直接画）
    if (spec.limits) {
      assert.equal(typeof formatLimit(spec, "min"), "string");
      assert.equal(typeof formatLimit(spec, "max"), "string");
    }
  }
  // 四组各有参数（页签不会空着）
  for (const group of ["tone", "color", "detail", "lens"] as const) {
    assert.ok(paramsInGroup(group).length > 0, `${group} 组不能是空的`);
  }
});

test("默认值：双极在 0、单极在左端", () => {
  for (const spec of PARAMS) {
    const value = PARAM_DEFAULTS[spec.id];
    assert.equal(typeof value, "number");
    if (spec.origin === "center") {
      // 双极 ⇒ 把手在正中（**不是**「判区间是否跨 0」：色温全是正数但仍是双极）
      assert.equal(value, (spec.min + spec.max) / 2, `${spec.id} 的默认值应当在正中`);
    } else {
      assert.equal(value, spec.min, `${spec.id} 的单极默认值在左端`);
    }
  }
  assert.deepEqual(defaultParams(), { ...PARAM_DEFAULTS });
  // 每次调用都是**新的**对象（否则「全部重置」之后面板还引用同一份）
  assert.notEqual(defaultParams(), defaultParams());
});

test("值文本：双极带正号、小数位与单位都对", () => {
  const exposure = PARAMS.find((p) => p.id === "exposure") as ParamSpec;
  assert.equal(formatParamValue(exposure, 0.35), "+0.35");
  assert.equal(formatParamValue(exposure, -1.5), "-1.50");
  assert.equal(formatParamValue(exposure, 0), "0.00");

  const temperature = PARAMS.find((p) => p.id === "temperature") as ParamSpec;
  assert.equal(formatParamValue(temperature, 5600), "+5600K");
  assert.equal(formatParamValue(temperature, 6250), "+6250K", "默认值不带正负歧义");

  const noise = PARAMS.find((p) => p.id === "lumaNr") as ParamSpec;
  assert.equal(formatParamValue(noise, 40), "40", "单极参数不带正号");
});

test("极值文案：有自定义用自定义，否则按小数位格式化", () => {
  const temperature = PARAMS.find((p) => p.id === "temperature") as ParamSpec;
  assert.equal(formatLimit(temperature, "min"), "2500");
  assert.equal(formatLimit(temperature, "max"), "10000");
  const contrast = PARAMS.find((p) => p.id === "contrast") as ParamSpec;
  assert.equal(formatLimit(contrast, "min"), "-100");
  assert.equal(formatLimit(contrast, "max"), "100");
});

test("动过没有：与默认值比较（未知 id 一律算没动）", () => {
  assert.equal(isParamDirty("exposure", 0), false);
  assert.equal(isParamDirty("exposure", 0.5), true);
  assert.equal(isParamDirty("不存在", 1), false);
});

test("裁切比例：固定项写自己、语义项走语言包", () => {
  const translate = (key: MessageKey): string => `t:${key}`;
  const free = CROP_RATIOS.find((item) => item.id === "free");
  const threeTwo = CROP_RATIOS.find((item) => item.id === "3:2");
  assert.ok(free !== undefined && threeTwo !== undefined);
  assert.equal(cropRatioLabel(free, translate), "t:editor.crop.free");
  assert.equal(cropRatioLabel(threeTwo, translate), "3:2");
});

test("反转比例：4:3 ⇄ 3:4、16:9 ⇄ 9:16、自由项不动", () => {
  const fourThree = CROP_RATIOS.find((item) => item.id === "4:3");
  const sixteenNine = CROP_RATIOS.find((item) => item.id === "16:9");
  const free = CROP_RATIOS.find((item) => item.id === "free");
  assert.ok(fourThree !== undefined && sixteenNine !== undefined && free !== undefined);
  assert.equal(invertRatio(fourThree).id, "3:4");
  assert.equal(invertRatio(sixteenNine).id, "9:16");
  assert.equal(invertRatio(free).id, "free", "自由没有比例可反转");
});

test("空态优先级：没有库 > 没有目录 > 没有照片 > 没选中", () => {
  const base = {
    loadingRepositories: false,
    repositoryId: "repo",
    scopePath: "photos/a",
    photoCount: 5,
    hasSelection: true,
    loadingPhotos: false,
  };
  assert.equal(editorEmptyKind(base), null, "有照片有选中 = 不出空态");
  assert.equal(editorEmptyKind({ ...base, hasSelection: false }), "no-selection");
  assert.equal(editorEmptyKind({ ...base, photoCount: 0, hasSelection: false }), "no-photos");
  assert.equal(
    editorEmptyKind({ ...base, scopePath: null, photoCount: 0, hasSelection: false }),
    "no-directory",
  );
  assert.equal(
    editorEmptyKind({ ...base, repositoryId: null, scopePath: null, photoCount: 0 }),
    "no-repository",
  );
});

test("读盘途中不报空态（否则会闪一下「还没有库」）", () => {
  const loading = {
    loadingRepositories: true,
    repositoryId: null,
    scopePath: null,
    photoCount: 0,
    hasSelection: false,
    loadingPhotos: false,
  };
  assert.equal(editorEmptyKind(loading), null);
  assert.equal(
    editorEmptyKind({ ...loading, loadingRepositories: false, repositoryId: "r", scopePath: "photos", loadingPhotos: true, photoCount: 0 }),
    null,
    "照片清单还在读时也不报空态",
  );
});

test("空态图标与「去导入」按钮：只有没有库那一态给按钮", () => {
  assert.equal(editorEmptyIcon("no-repository"), "library");
  assert.equal(editorEmptyIcon("no-directory"), "folder");
  assert.equal(editorEmptyIcon("no-photos"), "photo");
  assert.equal(editorEmptyIcon("no-selection"), "select");
  assert.equal(editorEmptyOffersImport("no-repository"), true);
  for (const kind of ["no-directory", "no-photos", "no-selection"] as const) {
    assert.equal(editorEmptyOffersImport(kind), false, `${kind} 去了导入也解决不了`);
  }
  assert.equal(editorEmptyOffersImport(null), false);
});

const PHOTOS: ViewerPhoto[] = [
  { id: "1", path: "/a.jpg", fileName: "a.jpg" },
  { id: "2", path: "/b.jpg", fileName: "b.jpg" },
];

test("胶片带适配：下标跟着锚点走，点一格写回选择", () => {
  let anchor = "2";
  const jumps: number[] = [];
  const strip = createEditorStrip({
    photos: () => PHOTOS,
    anchorId: () => anchor,
    goTo: (index) => jumps.push(index),
  });

  assert.equal(strip.state().index, 1, "锚点在第二张");
  assert.equal(strip.state().active, true);
  assert.equal(strip.current()?.id, "2");

  strip.goTo(0);
  assert.deepEqual(jumps, [0], "点选要交给调用方（写回 browse store 的选择）");

  anchor = "1";
  assert.equal(strip.state().index, 0);
  assert.equal(strip.current()?.id, "1");
});

test("胶片带适配：锚点不在清单里（刚换目录 / 被筛掉）不报错、也不高亮", () => {
  const strip = createEditorStrip({
    photos: () => PHOTOS,
    anchorId: () => "999",
    goTo: () => undefined,
  });
  assert.equal(strip.state().active, false, "不在清单里 = 没有当前照片");
  assert.equal(strip.state().index, 0, "下标给 0（胶片带不会越界）");
  assert.equal(strip.current(), null);

  const empty = createEditorStrip({ photos: () => [], anchorId: () => null, goTo: () => undefined });
  assert.equal(empty.state().active, false);
  assert.equal(empty.current(), null);
});
