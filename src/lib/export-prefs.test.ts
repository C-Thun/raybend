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
  assert.equal(p.topStep, 5);
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
  assert.equal(prefs.value().topStep,index<11?0:index-11);
  assert.equal(prefs.value().queueStep,2);
  assert.equal(prefs.value().scope,version===1?"all":"issues");
  assert(saved.has(EXPORT_PREFS_KEY));
 }
});
