import assert from "node:assert/strict";
import { test } from "node:test";
import type { AssetItem, FileExif } from "../../api/types.ts";
import type { IssueLibrary } from "../../api/issues.ts";
import { retainBrowseInfo, type BrowseInfoSnapshot } from "./info-snapshot.ts";

const item = (id: number) => ({ id, fileName: `照片${id}.JPG` }) as AssetItem;
const file = { lens: "镜头" } as FileExif;
const library = { issues: [] } as unknown as IssueLibrary;
const old: BrowseInfoSnapshot = { context: "库/目录", item: item(1), file, library, error: null };
const ready = {
  context: old.context, item: item(2), path: "照片2.JPG",
  metadata: { path: "照片2.JPG", file }, issuesReady: true, library, error: null,
};

test("right panel retains the whole previous photo until EXIF and issues both settle", () => {
  assert.equal(retainBrowseInfo(old, { ...ready, metadata: null }), old);
  assert.equal(retainBrowseInfo(old, { ...ready, issuesReady: false }), old);
  assert.equal(retainBrowseInfo(old, { ...ready, metadata: { path: "照片1.JPG", file } }), old);
  const next = retainBrowseInfo(old, ready)!;
  assert.equal(next.item.id, 2);
  assert.equal(next.file, file);
  assert.equal(next.library, library);
});

test("empty selection and repository/directory changes never retain another scope", () => {
  assert.equal(retainBrowseInfo(old, { ...ready, item: null }), null);
  assert.equal(retainBrowseInfo(old, { ...ready, path: null }), null);
  assert.equal(retainBrowseInfo(old, { ...ready, context: "其它库", metadata: null }), null);
  assert.equal(retainBrowseInfo(null, { ...ready, metadata: null }), null);
});

test("failed reads settle to the new photo instead of leaving stale editable metadata", () => {
  const next = retainBrowseInfo(old, {
    ...ready, metadata: { path: ready.path, file: null }, library: null, error: "offline",
  })!;
  assert.equal(next.item.id, 2);
  assert.equal(next.file, null);
  assert.equal(next.library, null);
  assert.equal(next.error, "offline");
});
