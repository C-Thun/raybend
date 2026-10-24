import assert from "node:assert/strict";
import test from "node:test";
import { emptyRuleDraft, rulesFromBrowseFilter } from "./organization-rules.ts";

const tags = [{ id: 7, name: "飞鸟", useCount: 3 }];

test("AND remains one group with the current library scope", () => {
  assert.deepEqual(rulesFromBrowseFilter({
    minRating: 4, colors: ["red", "yellow"], tags: [7], combinator: "and",
  }, tags, ["A", "A", "B"]), { ok: true, rules: { groups: [{
    repositoryIds: ["A", "B"], minRating: 4, colors: ["red", "yellow"], tagKeys: ["飞鸟"],
  }] } });
});

test("OR becomes separate groups without expanding values", () => {
  assert.deepEqual(rulesFromBrowseFilter({
    minRating: 4, colors: ["red", "yellow"], combinator: "or",
  }, tags, ["A"]), { ok: true, rules: { groups: [
    { repositoryIds: ["A"], minRating: 4 },
    { repositoryIds: ["A"], colors: ["red", "yellow"] },
  ] } });
});

test("transient or unknown conditions fail rather than disappearing", () => {
  assert.deepEqual(rulesFromBrowseFilter({ flag: { mode: "pick" }, tags: [999] }, tags),
    { ok: false, unsupported: ["flag", "unknownTags"] });
  assert.deepEqual(rulesFromBrowseFilter({ takenFrom: 1, text: "鸟" }, tags),
    { ok: false, unsupported: ["takenFrom", "text"] });
});

test("empty filter starts one editable group in the current scope", () => {
  assert.deepEqual(rulesFromBrowseFilter({}, tags, ["A"]), { ok: true, rules: emptyRuleDraft(["A"]) });
  assert.deepEqual(emptyRuleDraft(), { groups: [{ repositoryIds: [] }] });
});
