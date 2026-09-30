import assert from "node:assert/strict";
import { test } from "node:test";
import { parsePhotoDrag } from "./organization-drag.ts";

test("drag payload keeps repository identity and rejects malformed photos", () => {
  assert.deepEqual(parsePhotoDrag(JSON.stringify([
    { repositoryId: "甲", assetId: 1 }, { repositoryId: "乙", assetId: 1 },
  ])), [{ repositoryId: "甲", assetId: 1 }, { repositoryId: "乙", assetId: 1 }]);
  for (const bad of ["", "{}", '[{"repositoryId":"A","assetId":0}]',
    '[{"repositoryId":"A","assetId":1.5}]', '[{"repositoryId":"","assetId":1}]']) {
    assert.deepEqual(parsePhotoDrag(bad), []);
  }
});
