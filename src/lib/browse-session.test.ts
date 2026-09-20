import assert from "node:assert/strict";
import test from "node:test";

import {
  BROWSE_SESSION_STORAGE_KEY,
  DEFAULT_BROWSE_SESSION,
  readBrowseSession,
  sanitizeBrowseSession,
  writeBrowseSession,
  type BrowseSessionStorage,
} from "./browse-session.ts";

function memory(seed: string | null = null): BrowseSessionStorage & { value: string | null } {
  return {
    value: seed,
    getItem(key) {
      return key === BROWSE_SESSION_STORAGE_KEY ? this.value : null;
    },
    setItem(key, value) {
      if (key === BROWSE_SESSION_STORAGE_KEY) this.value = value;
    },
  };
}

test("browse session：库与中文深层目录可往返", () => {
  const store = memory();
  const session = { repositoryId: "repo-1", scopePath: "photos/日本/京都" };
  writeBrowseSession(session, store);
  assert.deepEqual(readBrowseSession(store), session);
});

test("browse session：坏 JSON、空值与非对象退回默认", () => {
  assert.deepEqual(readBrowseSession(memory("{oops")), DEFAULT_BROWSE_SESSION);
  assert.deepEqual(readBrowseSession(memory("")), DEFAULT_BROWSE_SESSION);
  assert.deepEqual(sanitizeBrowseSession(null), DEFAULT_BROWSE_SESSION);
  assert.deepEqual(sanitizeBrowseSession([]), DEFAULT_BROWSE_SESSION);
});

test("browse session：无库时不保留孤立目录；绝对路径与越界路径拒绝", () => {
  assert.deepEqual(
    sanitizeBrowseSession({ repositoryId: null, scopePath: "photos/a" }),
    DEFAULT_BROWSE_SESSION,
  );
  for (const scopePath of [
    "/photos/a",
    "photos/../outside",
    "photos/./outside",
    "photos//outside",
    "other/a",
    "photos\\a",
  ]) {
    assert.deepEqual(
      sanitizeBrowseSession({ repositoryId: "repo", scopePath }),
      { repositoryId: "repo", scopePath: null },
    );
  }
});

test("browse session：目录名中的连续点不是越界段", () => {
  assert.deepEqual(
    sanitizeBrowseSession({ repositoryId: "repo", scopePath: "photos/a..b/成片" }),
    { repositoryId: "repo", scopePath: "photos/a..b/成片" },
  );
});

test("browse session：超长 id/路径仍按原值保存，不截断 Unicode", () => {
  const repositoryId = `库-${"x".repeat(1024)}`;
  const scopePath = `photos/${"相片".repeat(1024)}`;
  assert.deepEqual(
    sanitizeBrowseSession({ repositoryId, scopePath }),
    { repositoryId, scopePath },
  );
});
