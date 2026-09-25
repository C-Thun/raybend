import assert from "node:assert/strict";
import { test } from "node:test";
import type { FileExif } from "../../api/types.ts";
import { createSelectedFileMetadata } from "./selected.ts";

const file = (lens: string): FileExif => ({ lens, cameraMake: "Panasonic", cameraModel: "DC-G9" }) as FileExif;
const tick = async () => { await Promise.resolve(); await Promise.resolve(); };

test("current selection uses catalog values immediately, then one file read updates every consumer", async () => {
  let resolve!: (value: FileExif) => void;
  const selected = createSelectedFileMetadata(() => new Promise<FileExif>((done) => { resolve = done; }));
  selected.select("/photos/a.RW2", { cameraMake: "Panasonic", camera: "DC-G9", lens: "catalog lens" });
  assert.equal(selected.path(), "/photos/a.RW2");
  assert.equal(selected.data()?.lens, "catalog lens");
  assert.equal(selected.file(), null);
  resolve(file("LEICA DG 12-60/F2.8-4.0"));
  await tick();
  assert.equal(selected.data()?.lens, "LEICA DG 12-60/F2.8-4.0");
  assert.equal(selected.file()?.lens, "LEICA DG 12-60/F2.8-4.0");

});

test("late file response cannot replace a newer selection or an empty state", async () => {
  const pending = new Map<string, (value: FileExif) => void>();
  const selected = createSelectedFileMetadata((path) => new Promise<FileExif>((done) => pending.set(path, done)));
  selected.select("a", { lens: "a" });
  selected.select("b", { lens: "b" });
  pending.get("a")!(file("old"));
  await tick();
  assert.equal(selected.data()?.lens, "b");
  pending.get("b")!(file("new"));
  await tick();
  assert.equal(selected.data()?.lens, "new");
  selected.select(null);
  assert.equal(selected.data(), null);
  assert.equal(selected.file(), null);
});

test("failed file read keeps the memory fallback", async () => {
  const selected = createSelectedFileMetadata(async () => { throw new Error("offline"); });
  selected.select("missing", { cameraMake: "OLYMPUS" });
  await tick();
  assert.equal(selected.data()?.cameraMake, "OLYMPUS");
  assert.equal(selected.file(), null);
});

test("missing fields in a disk response retain the current catalog values", async () => {
  const selected = createSelectedFileMetadata(async () => ({ lens: null, cameraMake: "Panasonic" }) as FileExif);
  selected.select("paired.jpg", { lens: "LEICA DG 12-60", camera: "DC-G9" });
  await tick();
  assert.equal(selected.data()?.lens, "LEICA DG 12-60");
  assert.equal(selected.data()?.camera, "DC-G9");
});

test("paired RAW or matcher can enrich the same selection without another read", () => {
  let reads = 0;
  const selected = createSelectedFileMetadata(async () => { reads++; return file("lens from disk"); });
  selected.select("paired.jpg", { camera: "DC-G9" });
  selected.enrich({ lens: "lens from RAW", cameraMake: "Panasonic" });
  assert.equal(selected.data()?.lens, "lens from RAW");
  assert.equal(selected.data()?.cameraMake, "Panasonic");
  assert.equal(reads, 1);
});
