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
  assert.equal(selected.settled(), null);
  resolve(file("LEICA DG 12-60/F2.8-4.0"));
  await tick();
  assert.equal(selected.data()?.lens, "LEICA DG 12-60/F2.8-4.0");
  assert.equal(selected.file()?.lens, "LEICA DG 12-60/F2.8-4.0");
  assert.equal(selected.settled()?.path, "/photos/a.RW2");

});

test("late file response cannot replace a newer selection or an empty state", async () => {
  const pending = new Map<string, (value: FileExif) => void>();
  const selected = createSelectedFileMetadata((path) => new Promise<FileExif>((done) => pending.set(path, done)));
  selected.select("a", { lens: "a" });
  selected.select("b", { lens: "b" });
  pending.get("a")!(file("old"));
  await tick();
  assert.equal(selected.data()?.lens, "b");
  assert.equal(selected.settled(), null);
  pending.get("b")!(file("new"));
  await tick();
  assert.equal(selected.data()?.lens, "new");
  selected.select(null);
  assert.equal(selected.data(), null);
  assert.equal(selected.file(), null);
  assert.equal(selected.settled(), null);
});

test("failed file read keeps the memory fallback", async () => {
  const selected = createSelectedFileMetadata(async () => { throw new Error("offline"); });
  selected.select("missing", { cameraMake: "OLYMPUS" });
  await tick();
  assert.equal(selected.data()?.cameraMake, "OLYMPUS");
  assert.equal(selected.file(), null);
  assert.deepEqual(selected.settled(), { path: "missing", file: null });
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

test("显示变体覆盖优先于文件回读的 EXIF（尺寸 + 文件类型，切变体后 flowinfo 跟着变）", async () => {
  // `specs/display-size-source.md` §6.1：尺寸是「当前显示变体」的，不是源文件的；
  // 文件类型同理（崔总 2026-10-11）：基底是 RAW 的变体一律显示 RAW。
  const selected = createSelectedFileMetadata(
    async () => ({ width: 6000, height: 4000, kind: "image", ext: "jpeg" }) as FileExif,
  );
  selected.select(
    "photos/a.RW2",
    { widthPx: 6000, heightPx: 4000, format: "JPEG" },
    { width: 2604, height: 1952, base: "raw" },
  );
  assert.deepEqual(
    { w: selected.data()?.widthPx, h: selected.data()?.heightPx },
    { w: 2604, h: 1952 },
    "文件还没回来时就是变体尺寸",
  );
  assert.equal(selected.data()?.format, "RAW", "RAW 基底的变体显示 RAW（哪怕基准位图是 JPEG）");
  await tick();
  assert.deepEqual(
    { w: selected.data()?.widthPx, h: selected.data()?.heightPx },
    { w: 2604, h: 1952 },
    "文件回来后**也不允许**盖掉变体尺寸",
  );
  assert.equal(selected.data()?.format, "RAW", "文件回读也不盖掉变体基底判定的类型");
  selected.enrich({ lens: "LEICA DG 12-60" });
  assert.equal(selected.data()?.widthPx, 2604, "enrich 也不动尺寸覆盖");
  selected.select("photos/a.RW2", { widthPx: 6000, heightPx: 4000 });
  assert.deepEqual(
    { w: selected.data()?.widthPx, h: selected.data()?.heightPx },
    { w: 6000, h: 4000 },
    "覆盖传 null：回到文件自己的尺寸",
  );
});

test("来回切同一张不重复读 EXIF（RAW 补镜头名要等 worker，不能每点一次都等）", async () => {
  let reads = 0;
  const selected = createSelectedFileMetadata(async (path) => {
    reads += 1;
    return file(`lens of ${path}`);
  });
  selected.select("a.RW2");
  await tick();
  selected.select("b.RW2");
  await tick();
  selected.select("a.RW2");
  await tick();
  assert.equal(reads, 2, "a 读一次、b 读一次；回到 a 走进程内缓存");
  assert.equal(selected.data()?.lens, "lens of a.RW2", "缓存命中也要把值发布出去");
  assert.equal(selected.settled()?.path, "a.RW2");
});

test("SOOC 基底的变体不动文件类型（位图就该显示它自己的格式）", async () => {
  const selected = createSelectedFileMetadata(
    async () => ({ width: 6000, height: 4000 }) as FileExif,
  );
  selected.select(
    "paired.jpg",
    { widthPx: 6000, heightPx: 4000, format: "JPEG" },
    { width: 6000, height: 4000, base: "sooc" },
  );
  assert.equal(selected.data()?.format, "JPEG");
});
