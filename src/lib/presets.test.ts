/**
 * 编辑预设纯逻辑的测试（`lib/presets.ts`，specs/editor-presets.md §7）。
 *
 * 覆盖：清洗（坏行丢弃 / 非法值）、快照构建（勾选大类写全字段）、
 * 应用计划（整体覆盖 / LUT 丢失忽略 / 恒等曲线回退）、落点规则、重名判定。
 */

import assert from "node:assert/strict";
import { DEFAULT_PRESET_GROUPS, presetColorChoice } from "./presets.ts";
import test from "node:test";

test("色彩预设组默认不选，缺配置或未来处理版本拒绝整份应用",()=> {
  assert.equal(DEFAULT_PRESET_GROUPS.includes("colorManagement" as never),false);
  const valid={version:2,colorManagement:{processVersion:"linear_rec2020_v2",input:{profileId:"b".repeat(64)}}} as const;
  assert.deepEqual(sanitizePresetSnapshot(valid),valid);
  assert.equal(presetColorChoice(valid),"b".repeat(64));
  assert.equal(presetColorChoice({version:1,tone:{exposure:1}}),undefined);
  assert.equal(presetColorChoice({version:2,colorManagement:{processVersion:"linear_rec2020_v2",input:"automatic"}}),null);
  for(const input of [null,{processVersion:"future_v3",input:"automatic"},{processVersion:"linear_rec2020_v2",input:{profileId:"C:/相机.icc"}},{processVersion:"linear_rec2020_v2",input:"automatic",monitor:"x"}]) {
    const original={version:2,tone:{exposure:1},colorManagement:input};
    const saved=sanitizePresetSnapshot(original)!;
    assert.deepEqual(saved.colorManagement,input);
    assert.throws(()=>presetColorChoice(saved),/PRESET_COLOR_UNSUPPORTED/);
  }
});

import {
  DEFAULT_DIRECTORY_ID,
  PRESET_GROUPS,
  buildPresetSnapshot,
  isDirectoryNameTaken,
  isPresetNameTaken,
  newPresetDirectoryId,
  newPresetId,
  planPresetApply,
  prunePresetCollapsed,
  prunePresetSelection,
  resolveCreateDirectory,
  sanitizePresetLibrary,
  sanitizePresetSnapshot,
  snapshotGroups,
  validatePresetName,
  type GroupParams,
  type PresetSnapshot,
} from "./presets.ts";

const GROUP_PARAMS: GroupParams = {
  tone: ["exposure", "contrast"],
  color: ["temperature", "saturation"],
  detail: ["lumaNr", "sharpenAmount"],
  lens: ["distortion", "vignette"],
};

const DEFAULTS: Record<string, number> = {
  exposure: 0, contrast: 0, temperature: 6250, saturation: 0,
  lumaNr: 0, sharpenAmount: 0, distortion: 0, vignette: 50,
};

/* ── 清洗 ──────────────────────────────────────────────── */

test("sanitizePresetSnapshot 拒绝非对象 / 错版本", () => {
  assert.equal(sanitizePresetSnapshot(null), null);
  assert.equal(sanitizePresetSnapshot(42), null);
  assert.equal(sanitizePresetSnapshot("text"), null);
  assert.equal(sanitizePresetSnapshot({ version: 2, tone: {} }), null);
  assert.equal(sanitizePresetSnapshot({ tone: { exposure: 1 } }), null);
});

test("sanitizePresetSnapshot 丢弃非法数值 / 非法曲线点，保留合法部分", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1,
    tone: { exposure: 0.5, contrast: "x", highlights: Number.NaN },
    curve: { rgb: [[0, 0], [1, 1], ["a", "b"]], r: [[0.2]] },
    lut: { id: "lut-1", enabled: "yes" },
  });
  assert.notEqual(snapshot, null);
  assert.deepEqual(snapshot!.tone, { exposure: 0.5 });
  // 曲线：坏点丢弃后 rgb 仍有 2 个合法点；r 只剩 1 个点 → 整通道丢弃
  assert.deepEqual(snapshot!.curve?.rgb, [[0, 0], [1, 1]]);
  assert.equal(snapshot!.curve?.r, undefined);
  // lut.enabled 非 true → false
  assert.deepEqual(snapshot!.lut, { id: "lut-1", enabled: false });
});

test("sanitizePresetSnapshot 裁剪曲线点到 0..1", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1, curve: { g: [[-0.5, 2], [0.5, 0.5]] },
  });
  assert.deepEqual(snapshot!.curve?.g, [[0, 1], [0.5, 0.5]]);
});

test("sanitizePresetSnapshot 规范化 detail / lens 的附加字段", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1,
    detail: { lumaNr: 10, nrMethod: "高" },
    lens: { distortion: -5, profile: "", enabled: 1 },
  });
  assert.equal(snapshot!.detail?.nrMethod, null);
  assert.equal(snapshot!.lens?.profile, null);
  assert.equal(snapshot!.lens?.enabled, null);
  assert.equal(snapshot!.detail?.lumaNr, 10);
});

test("sanitizePresetLibrary 丢坏行且不抛错", () => {
  const library = sanitizePresetLibrary({
    directories: [
      { id: "default", name: "Default", sortOrder: 0, createdAt: 1 },
      { id: "bad id", name: "x", sortOrder: 0, createdAt: 1 },
      { id: "d2", name: "", sortOrder: 1, createdAt: 2 },
      null,
    ],
    presets: [
      { id: "p1", directoryId: "default", name: "好行", createdAt: 1, updatedAt: 1,
        payload: { version: 1, tone: { exposure: 1 } } },
      // 坏 payload → 丢
      { id: "p2", directoryId: "default", name: "坏载荷", createdAt: 2, updatedAt: 2, payload: "not json" },
      // 目录不存在 → 丢
      { id: "p3", directoryId: "nope", name: "孤儿", createdAt: 3, updatedAt: 3,
        payload: { version: 1, tone: {} } },
      "garbage",
    ],
  });
  assert.equal(library.directories.length, 1);
  assert.equal(library.directories[0].id, "default");
  assert.equal(library.presets.length, 1);
  assert.equal(library.presets[0].id, "p1");
  // 整体是非对象也不抛
  assert.deepEqual(sanitizePresetLibrary("x"), { directories: [], presets: [] });
});

test("sanitizePresetLibrary 名称按字符计数（80 个 emoji 是合法的 80 字符）", () => {
  const emoji80 = "🌄".repeat(80);
  const library = sanitizePresetLibrary({
    directories: [{ id: "default", name: "Default", sortOrder: 0, createdAt: 1 }],
    presets: [
      { id: "p1", directoryId: "default", name: emoji80, createdAt: 1, updatedAt: 1, payload: { version: 1, tone: {} } },
      { id: "p2", directoryId: "default", name: `${emoji80}x`, createdAt: 2, updatedAt: 2, payload: { version: 1, tone: {} } },
      { id: "p3", directoryId: "default", name: "带\u0007控制符", createdAt: 3, updatedAt: 3, payload: { version: 1, tone: {} } },
    ],
  });
  assert.deepEqual(library.presets.map((preset) => preset.id), ["p1"]);
  assert.equal(library.presets[0].name.length, 160, "80 个 emoji 在 UTF-16 里是 160 个单元");
});

test("validatePresetName 与 Rust 同口径：字符数 / 控制字符 / 空白", () => {
  assert.equal(validatePresetName("", "preset"), "empty");
  assert.equal(validatePresetName("   ", "directory"), "empty");
  assert.equal(validatePresetName("名".repeat(80), "preset"), null);
  assert.equal(validatePresetName("名".repeat(81), "preset"), "tooLong");
  assert.equal(validatePresetName("名".repeat(40), "directory"), null);
  assert.equal(validatePresetName("名".repeat(41), "directory"), "tooLong");
  assert.equal(validatePresetName("🌄".repeat(80), "preset"), null, "emoji 按字符算，不是 UTF-16 单元");
  assert.equal(validatePresetName("a\u0000b", "preset"), "control");
  assert.equal(validatePresetName("a\u007fb", "preset"), "control");
  assert.equal(validatePresetName(" 柔和 ", "preset"), null);
});

test("prunePresetSelection 收敛失效选中（目录与预设）", () => {
  const directories = new Set(["default", "d1"]);
  const presets = new Set(["p1"]);
  assert.deepEqual(prunePresetSelection({ kind: "presets", ids: ["p1", "p2"] }, directories, presets), { kind: "presets", ids: ["p1"] });
  assert.equal(prunePresetSelection({ kind: "presets", ids: ["p2"] }, directories, presets), null);
  assert.deepEqual(prunePresetSelection({ kind: "directory", id: "d1" }, directories, presets), { kind: "directory", id: "d1" });
  assert.equal(prunePresetSelection({ kind: "directory", id: "gone" }, directories, presets), null);
  assert.equal(prunePresetSelection(null, directories, presets), null);
});

test("prunePresetCollapsed 丢掉已删目录的展开记录", () => {
  assert.deepEqual(
    prunePresetCollapsed({ d1: true, gone: true, default: false }, new Set(["default", "d1"])),
    { d1: true, default: false },
  );
});

/* ── 快照构建 ──────────────────────────────────────────── */

test("buildPresetSnapshot 勾选的大类写全字段（含默认值项）", () => {
  const snapshot = buildPresetSnapshot(
    ["tone", "lut"],
    {
      values: { exposure: 0.5, contrast: 0, temperature: 5400, saturation: 10 },
      curves: { rgb: [[0, 0], [1, 1]] },
      nrMethod: "high",
      lensProfile: null,
      lensEnabled: null,
      lutId: "lut-1",
      lutEnabled: true,
    },
    GROUP_PARAMS,
  );
  // tone 全字段（contrast 没动也写）
  assert.deepEqual(snapshot.tone, { exposure: 0.5, contrast: 0 });
  // 未勾选的大类不出现
  assert.equal(snapshot.color, undefined);
  assert.equal(snapshot.detail, undefined);
  assert.equal(snapshot.lens, undefined);
  assert.equal(snapshot.curve, undefined);
  assert.deepEqual(snapshot.lut, { id: "lut-1", enabled: true });
  // 缺失的参数值按 0 兜底
  const partial = buildPresetSnapshot(["tone"],
    { values: {}, curves: {}, nrMethod: null, lensProfile: null, lensEnabled: null, lutId: null, lutEnabled: false },
    GROUP_PARAMS);
  assert.deepEqual(partial.tone, { exposure: 0, contrast: 0 });
});

test("buildPresetSnapshot detail/lens 携带附加字段", () => {
  const snapshot = buildPresetSnapshot(
    ["detail", "lens"],
    {
      values: { lumaNr: 12, sharpenAmount: 3, distortion: -4, vignette: 60 },
      curves: {}, nrMethod: "high", lensProfile: "maker|model", lensEnabled: false,
      lutId: null, lutEnabled: false,
    },
    GROUP_PARAMS,
  );
  assert.equal(snapshot.detail?.nrMethod, "high");
  assert.equal(snapshot.detail?.lumaNr, 12);
  assert.equal(snapshot.lens?.profile, "maker|model");
  assert.equal(snapshot.lens?.enabled, false);
  assert.equal(snapshot.lens?.distortion, -4);
});

test("snapshotGroups 返回实际保存的大类（顺序即界面顺序）", () => {
  const snapshot: PresetSnapshot = { version: 1, lut: { id: null, enabled: false }, tone: {} };
  assert.deepEqual(snapshotGroups(snapshot), ["tone", "lut"]);
  assert.deepEqual(snapshotGroups({ version: 1 }), []);
});

/* ── 应用计划 ──────────────────────────────────────────── */

test("planPresetApply 按保存的大类整体覆盖（缺项回退默认值）", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1,
    tone: { exposure: 0.5 }, // contrast 缺 → 回退默认 0
    detail: { lumaNr: 10, nrMethod: "high" },
  })!;
  const plan = planPresetApply(snapshot, GROUP_PARAMS, DEFAULTS, () => false);
  assert.deepEqual(plan.values, { exposure: 0.5, contrast: 0, lumaNr: 10, sharpenAmount: 0 });
  assert.equal(plan.hasDetail, true);
  assert.equal(plan.nrMethod, "high");
  assert.equal(plan.hasLens, false);
  assert.equal(plan.lut, null);
  assert.deepEqual(plan.appliedGroups, ["tone", "detail"]);
});

test("planPresetApply 曲线大类写满四通道（缺通道 = 恒等）", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1, curve: { r: [[0, 0.1], [1, 0.9]] },
  })!;
  const plan = planPresetApply(snapshot, GROUP_PARAMS, DEFAULTS, () => false);
  assert.deepEqual(plan.curves.r, [[0, 0.1], [1, 0.9]]);
  assert.deepEqual(plan.curves.rgb, [[0, 0], [1, 1]]);
  assert.deepEqual(plan.curves.g, [[0, 0], [1, 1]]);
  assert.deepEqual(plan.curves.b, [[0, 0], [1, 1]]);
});

test("planPresetApply LUT 存在才生效，丢失静默忽略", () => {
  const exists = sanitizePresetSnapshot({ version: 1, lut: { id: "lut-1", enabled: true } })!;
  const applied = planPresetApply(exists, GROUP_PARAMS, DEFAULTS, (id) => id === "lut-1");
  assert.deepEqual(applied.lut, { id: "lut-1", enabled: true });
  assert.deepEqual(applied.appliedGroups, ["lut"]);

  const missing = planPresetApply(exists, GROUP_PARAMS, DEFAULTS, () => false);
  assert.equal(missing.lut, "missing");
  assert.deepEqual(missing.appliedGroups, []);
});

test("planPresetApply LUT id 为 null 表示清除当前 LUT", () => {
  const snapshot = sanitizePresetSnapshot({ version: 1, lut: { id: null, enabled: false } })!;
  const plan = planPresetApply(snapshot, GROUP_PARAMS, DEFAULTS, () => true);
  assert.deepEqual(plan.lut, { id: null, enabled: false });
  assert.deepEqual(plan.appliedGroups, ["lut"]);
});

test("planPresetApply lens 大类带配置与开关", () => {
  const snapshot = sanitizePresetSnapshot({
    version: 1, lens: { distortion: 3, profile: "m|l", enabled: false },
  })!;
  const plan = planPresetApply(snapshot, GROUP_PARAMS, DEFAULTS, () => false);
  assert.equal(plan.hasLens, true);
  assert.deepEqual(plan.lens, { profile: "m|l", enabled: false });
});

/* ── 目录规则 ──────────────────────────────────────────── */

test("resolveCreateDirectory 落点：目录 → 预设所在目录 → default", () => {
  const directories = [{ id: "default" }, { id: "d1" }, { id: "d2" }];
  const presets = [
    { id: "p1", directoryId: "d1" },
    { id: "p2", directoryId: "d2" },
  ];
  // 选中目录
  assert.equal(resolveCreateDirectory({ kind: "directory", id: "d2" }, directories, presets), "d2");
  // 选中预设 → 其所在目录
  assert.equal(resolveCreateDirectory({ kind: "presets", ids: ["p2"] }, directories, presets), "d2");
  assert.equal(resolveCreateDirectory({ kind: "presets", ids: ["p1", "p2"] }, directories, presets), "d1");
  // 无选中 / 目录已不存在 / 预设已不存在 → default
  assert.equal(resolveCreateDirectory(null, directories, presets), DEFAULT_DIRECTORY_ID);
  assert.equal(resolveCreateDirectory({ kind: "directory", id: "gone" }, directories, presets), DEFAULT_DIRECTORY_ID);
  assert.equal(resolveCreateDirectory({ kind: "presets", ids: ["gone"] }, directories, presets), DEFAULT_DIRECTORY_ID);
});

test("isPresetNameTaken 大小写与首尾空格都算同一个", () => {
  const presets = [
    { name: "柔和胶片", directoryId: "d1" },
    { name: " Night ", directoryId: "d1" },
  ];
  assert.ok(isPresetNameTaken("柔和胶片", "d1", presets));
  assert.ok(isPresetNameTaken("柔和胶片 ", "d1", presets));
  assert.ok(!isPresetNameTaken("柔和胶片", "d2", presets)); // 不同目录不算
  assert.ok(isPresetNameTaken("night", "d1", presets));
  assert.ok(!isPresetNameTaken("", "d1", presets));
  assert.ok(!isPresetNameTaken("  ", "d1", presets));
});

test("isDirectoryNameTaken 同口径", () => {
  const directories = [{ name: "人像" }, { name: " Portrait " }];
  assert.ok(isDirectoryNameTaken("人像", directories));
  assert.ok(isDirectoryNameTaken("PORTRAIT", directories));
  assert.ok(!isDirectoryNameTaken("风景", directories));
});

test("id 生成器本地唯一", () => {
  const existing = [{ id: "presetdir-a" }, { id: "preset-b" }];
  const dirId = newPresetDirectoryId(existing, () => 0.5);
  assert.ok(!existing.some((entry) => entry.id === dirId));
  assert.ok(dirId.startsWith("presetdir-"));
  const presetId = newPresetId(existing, () => 0.5);
  assert.ok(!existing.some((entry) => entry.id === presetId));
  assert.ok(presetId.startsWith("preset-"));
});

test("PRESET_GROUPS 顺序即界面顺序", () => {
  assert.deepEqual([...PRESET_GROUPS], ["tone", "color", "detail", "lens", "curve", "lut", "colorManagement"]);
});
