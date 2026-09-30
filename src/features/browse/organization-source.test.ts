import assert from "node:assert/strict";
import { test } from "node:test";
import { createRoot } from "solid-js";
import type { AssetItem, RepositoryView } from "../../api/types.ts";
import { createOrganizationSource, organizationPhotoId } from "./organization-source.ts";

const repo = (id: string): RepositoryView => ({
  id, name: id, online: true, root: `/photos/${id}`, connection: { generation: 1 } as unknown as RepositoryView["connection"],
} as RepositoryView);
const asset = (id: number, name: string): AssetItem => ({
  id, relPath: `day/${name}.jpg`, fileName: `${name}.jpg`, ext: "jpg", width: 100, height: 80,
  rating: 0, colorLabel: null, likeState: null, lockLevel: 0, isRaw: false, hasRaw: false,
  issueCount: 0, edited: false, missing: false,
} as AssetItem);

test("cross-library IDs hydrate independently and selection survives a same-scope refresh", async () => {
  await createRoot(async (dispose) => {
    const source = createOrganizationSource({
      repositories: () => [repo("A"), repo("B")],
      thumbs: { get: () => ({ status: "idle" }) as ReturnType<Parameters<typeof createOrganizationSource>[0]["thumbs"]["get"]>, request: () => {} },
      tileStep: () => 1, setTileStep: () => {}, commitTileStep: () => {}, infoMode: () => "off", filter: () => ({}),
      api: {
        tagTimeline: async (repositoryId) => ({ total: 1, entries: [{ id: 7, takenAt: repositoryId === "A" ? 2 : 1, relPath: "day" }] as never }),
        bucketTimeline: async () => ({ total: 0, entries: [] }),
        assets: async (repositoryId, ids) => ids.map((id) => asset(id, repositoryId)),
        flags: async () => ({ picks: [], rejects: [], total: 0 }),
      },
    });
    const target = { kind: "tag" as const, key: "旅行" };
    await source.load(target);
    assert.equal(source.source.count(), 2);
    const a = organizationPhotoId("A", 7), b = organizationPhotoId("B", 7);
    assert.notEqual(a, b);
    source.source.ensureRange?.(0, 2);
    await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(source.itemById(a)?.fileName, "A.jpg");
    assert.equal(source.itemById(b)?.fileName, "B.jpg");
    source.source.select(b, "replace");
    await source.load(target);
    assert.deepEqual([...source.selection().ids], [b]);
    assert.equal(source.selection().anchor, b);
    dispose();
  });
});

test("older timeline response cannot replace a newer tag selection", async () => {
  await createRoot(async (dispose) => {
    let releaseOld: (value: { total: number; entries: never[] }) => void = () => {};
    const old = new Promise<{ total: number; entries: never[] }>((resolve) => { releaseOld = resolve; });
    const source = createOrganizationSource({
      repositories: () => [repo("A")],
      thumbs: { get: () => ({ status: "idle" }) as ReturnType<Parameters<typeof createOrganizationSource>[0]["thumbs"]["get"]>, request: () => {} },
      tileStep: () => 1, setTileStep: () => {}, commitTileStep: () => {}, infoMode: () => "off", filter: () => ({}),
      api: {
        tagTimeline: async (_repo, key) => key === "old" ? old :
          { total: 1, entries: [{ id: 9, takenAt: 1, relPath: "day" }] as never },
        bucketTimeline: async () => ({ total: 0, entries: [] }),
        assets: async () => [], flags: async () => ({ picks: [], rejects: [], total: 0 }),
      },
    });
    const pending = source.load({ kind: "tag", key: "old" });
    await source.load({ kind: "tag", key: "new" });
    releaseOld({ total: 1, entries: [{ id: 7, takenAt: 2, relPath: "day" }] as never });
    await pending;
    assert.equal(source.source.idAt?.(0), organizationPhotoId("A", 9));
    dispose();
  });
});

test("one unavailable library does not hide the other library's tag photos", async () => {
  await createRoot(async (dispose) => {
    const failed: string[][] = [];
    const source = createOrganizationSource({
      repositories: () => [repo("A"), repo("B")],
      thumbs: { get: () => ({ status: "idle" }) as ReturnType<Parameters<typeof createOrganizationSource>[0]["thumbs"]["get"]>, request: () => {} },
      tileStep: () => 1, setTileStep: () => {}, commitTileStep: () => {}, infoMode: () => "off", filter: () => ({}),
      onPartialFailure: (ids) => failed.push(ids),
      api: {
        tagTimeline: async (repositoryId) => {
          if (repositoryId === "A") throw new Error("temporarily offline");
          return { total: 1, entries: [{ id: 7, takenAt: 1, relPath: "day" }] as never };
        },
        bucketTimeline: async () => ({ total: 0, entries: [] }),
        assets: async () => [], flags: async () => ({ picks: [], rejects: [], total: 0 }),
      },
    });
    await source.load({ kind: "tag", key: "旅行" });
    assert.equal(source.source.count(), 1);
    assert.equal(source.source.idAt?.(0), organizationPhotoId("B", 7));
    assert.equal(source.error(), null);
    assert.deepEqual(failed, [["A"]]);
    dispose();
  });
});
