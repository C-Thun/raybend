import assert from "node:assert/strict";
import { test } from "node:test";
import { stageTagEdit, tagDraftRows, tagKey } from "./tag-draft.ts";
import type { PhotoTagState } from "../../api/organization.ts";
const state: PhotoTagState = { version: 1, manual: ["海"], ai: ["海", "人物"], masks: ["人物"], result: { sourceKey: "x", modelSha256: "a", pipelineSha256: "b", generatedAt: 1, valid: true, origin: "local", evidence: [] } };
test("removing manual preserves AI and retaining a masked AI tag clears mask", () => {
  let edits = stageTagEdit([], { name: "海", manual: false, masked: null });
  assert.deepEqual(tagDraftRows(state, edits).find((row) => row.name === "海"), { name: "海", manual: false, ai: true, masked: false });
  edits = stageTagEdit(edits, { name: "人物", manual: true, masked: null });
  assert.deepEqual(tagDraftRows(state, edits).find((row) => row.name === "人物"), { name: "人物", manual: true, ai: true, masked: false });
  assert.deepEqual(state.masks, ["人物"]);
});
test("text folding, same-term staged edits, invalid AI and empty state", () => {
  assert.equal(tagKey(" E\u0301TÉ "), tagKey("été"));
  const edits = stageTagEdit(stageTagEdit([], { name: "CAFÉ", manual: true, masked: null }), { name: "Cafe\u0301", manual: null, masked: true });
  assert.equal(edits.length, 1);
  assert.equal(tagDraftRows({ ...state, result: null }, []).some((row) => row.name === "人物" && row.ai), false);
  assert.deepEqual(tagDraftRows({ version: 1, manual: [], ai: [], masks: [], result: null }, []), []);
});
