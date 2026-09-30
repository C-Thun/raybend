import assert from "node:assert/strict";
import test from "node:test";
import { emptyRuleDraft, rulesFromBrowseFilter } from "./organization-rules.ts";

const tags = [{ id: 7, name: "飞鸟", useCount: 3 }];

test("AND remains one group with explicit repository selection pending", () => {
  assert.deepEqual(rulesFromBrowseFilter({
    minRating: 4, colors: ["red", "yellow"], tags: [7], combinator: "and",
  }, tags), { ok: true, rules: { groups: [{
    repositoryIds: [], minRating: 4, colors: ["red", "yellow"], tagKeys: ["飞鸟"],
  }] } });
});

test("OR becomes separate groups without expanding values", () => {
  assert.deepEqual(rulesFromBrowseFilter({
    minRating: 4, colors: ["red", "yellow"], combinator: "or",
  }, tags), { ok: true, rules: { groups: [
    { repositoryIds: [], minRating: 4 },
    { repositoryIds: [], colors: ["red", "yellow"] },
  ] } });
});

test("transient or unknown conditions fail rather than disappearing", () => {
  assert.deepEqual(rulesFromBrowseFilter({ flag: { mode: "pick" }, tags: [999] }, tags),
    { ok: false, unsupported: ["flag", "unknownTags"] });
  assert.deepEqual(rulesFromBrowseFilter({ takenFrom: 1, text: "鸟" }, tags),
    { ok: false, unsupported: ["takenFrom", "text"] });
});

test("empty filter starts one editable group", () => {
  assert.deepEqual(rulesFromBrowseFilter({}, tags), { ok: true, rules: emptyRuleDraft() });
});
