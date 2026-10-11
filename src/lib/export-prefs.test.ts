import assert from "node:assert/strict";
import test from "node:test";
import {
  createExportPreferences,
  DEFAULT_EXPORT_DISPLAY,
  EXPORT_PREFS_KEY,
  readExportDisplay,
} from "./export-prefs.ts";
test("invalid primitive, damage, missing and future values use defaults", () => {
  for (const raw of [null, "bad", "null", "0", '"text"', "[]"])
    assert.deepEqual(readExportDisplay(raw), DEFAULT_EXPORT_DISPLAY);
  assert.deepEqual(
    readExportDisplay('{"scope":"future","info":"future"}'),
    DEFAULT_EXPORT_DISPLAY,
  );
});
test("independent continuous zoom values and ratio clamped", () => {
  const p = readExportDisplay(
    '{"topStep":16.5,"queueStep":-1,"ratio":99,"grouped":true,"scope":"edited"}',
  );
  // 上部档位是 192–320 的 10 档（2026-10-11 下限 240 → 192）
  assert.equal(p.topStep, 9);
  assert.equal(p.queueStep, 0);
  assert.equal(p.ratio, 0.8);
  assert.equal(p.scope, "edited");
  assert.equal(readExportDisplay('{"topStep":2.25}').topStep, 2.25);
});
test("preferences delayed commit and whitelist persistence never includes queue", () => {
  const writes: string[] = [];
  const prefs = createExportPreferences({
    getItem: () => null,
    setItem: (key, value) => {
      assert.equal(key, EXPORT_PREFS_KEY);
      writes.push(value);
    },
  });
  prefs.update({ topStep: 3.5 }, false);
  assert.equal(writes.length, 0);
  prefs.commit();
  assert.equal(JSON.parse(writes[0]!).topStep, 3.5);
  prefs.update({ queueStep: 2 });
  assert.equal(JSON.parse(writes[1]!).queueStep, 2);
  assert.equal(prefs.value().topStep, 3.5);
  assert.doesNotMatch(writes.join(""), /enabled|queues/);
});
test("disabled storage does not break interaction", () => {
  const p = createExportPreferences({
    getItem: () => {
      throw Error("denied");
    },
    setItem: () => {
      throw Error("denied");
    },
  });
  p.update({ scope: "issues" });
  assert.equal(p.value().scope, "issues");
});

test("v1/v2 display migration preserves physical size and raises upper minimum", () => {
 for(const version of [1,2]) for(const index of [0,8,12,16]) {
  const saved=new Map([[`raybend.export-display.v${version}`, JSON.stringify({topStep:index,queueStep:2,scope:version===1?"sooc":"issues"})]]);
  const prefs=createExportPreferences({getItem:key=>saved.get(key)??null,setItem:(key,value)=>{saved.set(key,value);}});
  // v1/v2 存的是**全局 17 档**的下标：先按尺寸换算，再落到上部 10 档里
  assert.equal(prefs.value().topStep,[0,1,5,9][[0,8,12,16].indexOf(index)]);
  assert.equal(prefs.value().queueStep,2);
  assert.equal(prefs.value().scope,version===1?"all":"issues");
  assert(saved.has(EXPORT_PREFS_KEY));
 }
});

test("v3 display migration keeps the physical size when the upper minimum drops to 192", () => {
 // v3 存的是**旧上部边界（240–320）**下的下标：按尺寸回填，240 仍落在 240（新下标 4）
 for (const [index, expected] of [[0, 4], [2, 6], [5, 9]] as const) {
  const saved = new Map([[`raybend.export-display.v3`, JSON.stringify({ topStep: index, queueStep: 3, scope: "issues" })]]);
  const prefs = createExportPreferences({ getItem: key => saved.get(key) ?? null, setItem: (key, value) => { saved.set(key, value); } });
  assert.equal(prefs.value().topStep, expected);
  assert.equal(prefs.value().queueStep, 3);
  assert.equal(prefs.value().scope, "issues");
  assert(saved.has(EXPORT_PREFS_KEY));
 }
});
