import assert from "node:assert/strict";
import test from "node:test";

import { membersAllOffline, mountedMemberCount } from "./bucket-counts.ts";

const bucket = {
  memberCount: 7,
  memberCountsByRepository: { a: 3, b: 4 },
};

test("current mounted count only sums online repositories", () => {
  assert.equal(mountedMemberCount(bucket, new Set(["a"])), 3);
  assert.equal(mountedMemberCount(bucket, new Set(["a", "b"])), 7);
  assert.equal(mountedMemberCount(bucket, new Set()), 0);
});

test("all offline is distinct from an empty bucket", () => {
  assert.equal(membersAllOffline(bucket, new Set(["a"])), false);
  assert.equal(membersAllOffline(bucket, new Set(["c"])), true);
  assert.equal(membersAllOffline({ memberCount: 0, memberCountsByRepository: {} }, new Set()), false);
});
