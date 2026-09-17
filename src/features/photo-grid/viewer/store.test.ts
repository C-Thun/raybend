/**
 * 看图的变换数学与生命周期。
 *
 * 这些数字**不能靠眼睛调**：锚点缩放、适配缩放、大图替换时的换算错一点，
 * 表现就是「滚轮一滚图跑了」或「大图上来画面跳一下」。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  clampPan,
  clampZoom,
  computeFitScale,
  createViewerStore,
  MAX_ZOOM,
  MIN_ZOOM,
  zoomPanAt,
  type ViewerPhoto,
} from "./store.ts";

const PHOTOS: ViewerPhoto[] = [
  { id: "a", path: "/a.jpg", fileName: "a.jpg" },
  { id: "b", path: "/b.jpg", fileName: "b.jpg" },
  { id: "c", path: "/c.jpg", fileName: "c.jpg" },
];

/** 假取图：先小图后大图；可让大图失败，用来验降级 */
function fakeDeps(options: { screenFails?: boolean } = {}) {
  const urls: string[] = [];
  const revoked: string[] = [];
  let counter = 0;
  return {
    urls,
    revoked,
    deps: {
      loadThumb: async (): Promise<Uint8Array> => new Uint8Array([1]),
      loadScreen: async (): Promise<Uint8Array | null> => {
        if (options.screenFails === true) return null;
        return new Uint8Array([2]);
      },
      makeUrl: (): string => {
        counter += 1;
        const url = `blob:${counter}`;
        urls.push(url);
        return url;
      },
      revokeUrl: (url: string): void => {
        revoked.push(url);
      },
    },
  };
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

test("computeFitScale：contain —— 宽高都放得下", () => {
  assert.equal(
    computeFitScale({ width: 800, height: 600 }, { width: 400, height: 300 }),
    2,
  );
  // 竖图：受高度限制
  assert.equal(
    computeFitScale({ width: 800, height: 600 }, { width: 300, height: 600 }),
    1,
  );
  // 任一尺寸未知 → 给 1（不产生 Infinity/NaN）
  assert.equal(computeFitScale({ width: 0, height: 0 }, { width: 10, height: 10 }), 1);
  assert.equal(computeFitScale({ width: 800, height: 600 }, { width: 0, height: 0 }), 1);
});

test("clampZoom：夹到上下限，非法值退回 1", () => {
  assert.equal(clampZoom(0.0001), MIN_ZOOM);
  assert.equal(clampZoom(999), MAX_ZOOM);
  assert.equal(clampZoom(Number.NaN), 1);
  assert.equal(clampZoom(0), 1);
  assert.equal(clampZoom(2), 2);
});

test("zoomPanAt：锚点下的那一点**不动**（滚轮缩放的关键性质）", () => {
  const viewport = { width: 800, height: 600 };
  const pan = { x: 30, y: -20 };
  const zoom = 1;
  const anchor = { x: 600, y: 100 };
  const nextZoom = 2;
  const nextPan = zoomPanAt({ pan, zoom, nextZoom, anchor, viewport });

  // 锚点处的图像坐标：imgPoint = (anchor − center − pan) / zoom
  const center = { x: 400, y: 300 };
  const imgPoint = {
    x: (anchor.x - center.x - pan.x) / zoom,
    y: (anchor.y - center.y - pan.y) / zoom,
  };
  // 用新 pan/zoom 再算回屏幕坐标，应当还是 anchor
  const screen = {
    x: center.x + nextPan.x + imgPoint.x * nextZoom,
    y: center.y + nextPan.y + imgPoint.y * nextZoom,
  };
  assert.ok(Math.abs(screen.x - anchor.x) < 1e-9, `x 漂了：${screen.x}`);
  assert.ok(Math.abs(screen.y - anchor.y) < 1e-9, `y 漂了：${screen.y}`);
});

test("clampPan：**原图尺寸未知时不夹取**（RAW 双击点开后拖不动的根因）", () => {
  /*
   * `natural` 是 0 时，原来的公式算出的上限是 0 —— 任何缩放下都拖不动。
   * RAW 的尺寸以前读不到（EXIF 读不了 RW2 的魔数），于是「双击点开 RAW 拖不动」。
   * 判据：尺寸未知时**原样放行**（不知道边界不等于不许移动）。
   */
  const viewport = { width: 1200, height: 800 };
  const unknown = { width: 0, height: 0 };
  assert.deepEqual(
    clampPan({ pan: { x: 500, y: -300 }, zoom: 2, viewport, natural: unknown }),
    { x: 500, y: -300 },
  );
  // 只有一个方向未知时也不夹（不能只修一半）
  assert.deepEqual(
    clampPan({
      pan: { x: 250, y: 120 },
      zoom: 1,
      viewport,
      natural: { width: 0, height: 3000 },
    }),
    { x: 250, y: 120 },
  );
});

test("clampPan：图比视口大 → 边不许拖进来；比视口小 → 锁在中间", () => {
  const viewport = { width: 800, height: 600 };
  const natural = { width: 1000, height: 800 };

  // 2 倍：可拖范围 = (2000−800)/2 = 600
  const big = clampPan({ pan: { x: 5000, y: -5000 }, zoom: 2, viewport, natural });
  assert.equal(big.x, 600, "超出右边界 → 停在 +limit");
  assert.equal(big.y, -500, "超出上边界 → 停在 −limit（(1600−600)/2 = 500）");

  // 0.5 倍：图比视口小 → 只能居中
  const small = clampPan({ pan: { x: 120, y: 80 }, zoom: 0.5, viewport, natural });
  assert.deepEqual(small, { x: 0, y: 0 });
});

test("show：打开、适配、先小图后大图（渐进），current/index 正确", async () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);

  assert.equal(store.state().active, true);
  assert.equal(store.current()?.fileName, "a.jpg");
  assert.equal(store.imageStatus(), "loading");

  await flush();
  assert.equal(store.imageStatus(), "ready");
  assert.equal(store.imageUrl(), "blob:2", "大图到了就用大图（小图只是先顶上）");
  assert.equal(store.sharp(), true);
  assert.ok(fake.urls.length >= 1);
});

test("show：大图取不到时，小图仍然显示（不是错误态）", async () => {
  const fake = fakeDeps({ screenFails: true });
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  await flush();
  assert.equal(store.imageStatus(), "ready");
  assert.equal(store.imageUrl(), "blob:1", "用网格小图兜住");
  assert.equal(store.sharp(), false);
});

test("show：越界的下标会被夹回来（别信任调用方）", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 99);
  assert.equal(store.state().index, 2);
  store.show(PHOTOS, -5);
  assert.equal(store.state().index, 0);
});

test("next/prev：到头就停住，不循环（循环会让人以为后面还有）", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 0);
  store.prev();
  assert.equal(store.state().index, 0);
  store.next();
  store.next();
  assert.equal(store.state().index, 2);
  store.next();
  assert.equal(store.state().index, 2);
});

test("toggleFit：适配 ↔ 100% ↔ 适配", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  store.setNatural({ width: 1600, height: 1200 });

  assert.equal(store.state().fit, true);
  assert.equal(store.state().zoom, 0.5, "800/1600 = 0.5 适配");

  store.toggleFit();
  assert.equal(store.state().fit, false);
  assert.equal(store.state().zoom, 1, "100%");

  store.toggleFit();
  assert.equal(store.state().fit, true);
  assert.equal(store.state().zoom, 0.5, "回到适配");
});

test("滚轮缩放会退出适配状态（否则下一次尺寸变化又被拉回去）", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  store.setNatural({ width: 800, height: 600 });
  assert.equal(store.state().fit, true);
  store.zoomBy(1.5);
  assert.equal(store.state().fit, false);
  assert.equal(store.state().zoom, 1.5);
});

test("换大图（原图尺寸变大）时画面**不跳**：适配重算、手动缩放按比例换算", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);

  // ① 适配状态：小图 400×300 → 适配 2；换 1600×1200 大图 → 适配 0.5（屏幕占比不变）
  store.setNatural({ width: 400, height: 300 });
  assert.equal(store.state().zoom, 2);
  store.setNatural({ width: 1600, height: 1200 });
  assert.equal(store.state().zoom, 0.5, "适配是按屏幕占比算的，换大图后占比不变");
  assert.equal(1600 * store.state().zoom, 800, "屏幕上仍然是 800 宽");

  // ② 手动缩放状态：zoom 按原图尺寸比例换算，屏幕上大小不变
  store.zoomTo(1); // 100%（相对 1600 宽 → 屏幕 1600）
  const before = 1600 * store.state().zoom;
  store.setNatural({ width: 3200, height: 2400 });
  assert.equal(3200 * store.state().zoom, before, "屏幕上的大小不该跳");
});

test("panBy：平移受夹取约束", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  store.setNatural({ width: 5000, height: 5000 });
  store.zoomTo(1);
  store.panBy(10_000, 10_000);
  assert.equal(store.state().pan.x, (5000 - 800) / 2);
});

test("close：复位并回收 URL（不泄漏 blob）", async () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 0);
  await flush();
  const used = store.imageUrl();
  assert.ok(used !== null);
  store.close();
  assert.equal(store.state().active, false);
  assert.equal(store.imageUrl(), null);
  assert.ok(fake.revoked.includes(used!), `应当回收 ${used}`);
});
