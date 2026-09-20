import assert from "node:assert/strict";
import test from "node:test";
import { createRoot, createSignal } from "solid-js";

import { createViewerStore, type ViewerPhoto } from "../../components/ui/viewer/store.ts";
import { focusSelection, type SelectionState } from "../../lib/selection.ts";
import { createPhotoViewingController } from "./viewing.ts";

const PHOTOS: ViewerPhoto[] = [
  { id: "a", path: "/a.jpg", fileName: "a.jpg" },
  { id: "b", path: "/b.jpg", fileName: "b.jpg" },
  { id: "c", path: "/c.jpg", fileName: "c.jpg" },
];

test("共享看图控制器：两侧共用对比派生、锚点、命令请求与四态", () => {
  createRoot((dispose) => {
    const viewer = createViewerStore({
      loadThumb: async () => null,
      loadScreen: async () => null,
    });
    const [selection, setSelection] = createSignal<SelectionState>({
      ids: new Set(["a", "b"]),
      anchor: "a",
    });
    // Node 测试加载的是 Solid 的 server build：memo 在创建时求值，所以先把查看列表就位。
    viewer.show(PHOTOS, 0);
    const controller = createPhotoViewingController({
      viewer,
      selection,
      setAnchor: (id) => setSelection((current) => focusSelection(current, id)),
      naturalOf: (id) => (id === "b" ? { width: 3000, height: 4000 } : null),
      ensureNatural: () => undefined,
    });

    assert.equal(controller.comparing(), true);
    assert.deepEqual(controller.comparedIds(), ["a", "b"]);
    assert.deepEqual(controller.comparePhotos()[1]?.natural, {
      width: 3000,
      height: 4000,
    });

    controller.focusComparePhoto(PHOTOS[1] as ViewerPhoto);
    assert.equal(viewer.current()?.id, "b");
    assert.equal(selection().anchor, "b");
    assert.deepEqual([...selection().ids], ["a", "b"], "聚焦不能拆散对比选择");

    controller.toggleCompareStrip();
    assert.deepEqual(controller.filmOnlyIds(), ["a", "b"]);

    assert.equal(controller.openRequest(), 0);
    controller.requestOpen();
    controller.requestOpen();
    assert.equal(controller.openRequest(), 2);

    assert.equal(controller.chrome(), "default");
    controller.cycleChrome();
    assert.notEqual(controller.chrome(), "default");
    controller.resetChrome();
    assert.equal(controller.chrome(), "default");

    dispose();
  });
});
