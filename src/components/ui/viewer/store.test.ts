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
  type ViewerState,
  visibleRect,
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

test("clampPan：**内容尺寸未知时不夹取**（RAW 双击点开后拖不动的根因）", () => {
  /*
   * 内容是 0 时，原来的公式算出的上限是 0 —— 任何缩放下都拖不动。
   * RAW 的尺寸以前读不到（EXIF 读不了 RW2 的魔数），于是「双击点开 RAW 拖不动」。
   * 判据：尺寸未知时**原样放行**（不知道边界不等于不许移动）。
   */
  const viewport = { width: 1200, height: 800 };
  assert.deepEqual(
    clampPan({ pan: { x: 500, y: -300 }, content: { width: 0, height: 0 }, viewport }),
    { x: 500, y: -300 },
  );
  // 只有一个方向未知时也不夹（不能只修一半）
  assert.deepEqual(
    clampPan({
      pan: { x: 250, y: 120 },
      content: { width: 0, height: 3000 },
      viewport,
    }),
    { x: 250, y: 120 },
  );
});

test("clampPan：内容比视口大 → 边不许拖进来；比视口小 → 锁在中间", () => {
  const viewport = { width: 800, height: 600 };

  // 内容 2000×1600：可拖范围 = (2000−800)/2 = 600
  const big = clampPan({
    pan: { x: 5000, y: -5000 },
    content: { width: 2000, height: 1600 },
    viewport,
  });
  assert.equal(big.x, 600, "超出右边界 → 停在 +limit");
  assert.equal(big.y, -500, "超出上边界 → 停在 −limit（(1600−600)/2 = 500）");

  // 内容比视口小 → 只能居中
  const small = clampPan({
    pan: { x: 120, y: 80 },
    content: { width: 500, height: 400 },
    viewport,
  });
  assert.deepEqual(small, { x: 0, y: 0 });
});

test("clampPan：对比的画框也走同一条规则（画框 × 相对倍数 = 内容）", () => {
  const viewport = { width: 600, height: 400 };
  // 适配（内容 = 画框 600×300）：高度方向没得拖，宽度方向也没溢出
  assert.deepEqual(
    clampPan({ pan: { x: 80, y: -50 }, content: { width: 600, height: 300 }, viewport }),
    { x: 0, y: 0 },
  );
  // 放到 1.5 倍（内容 900×450）：x 上限 (900−600)/2 = 150，y 上限 (450−400)/2 = 25
  assert.deepEqual(
    clampPan({ pan: { x: 500, y: -500 }, content: { width: 900, height: 450 }, viewport }),
    { x: 150, y: -25 },
  );
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
  assert.equal(store.overviewImageUrl(), "blob:2", "总览只接完整的 Screen 图");
  assert.equal(store.overviewStatus(), "ready");
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
  assert.equal(store.imageUrl(), "blob:1", "主视图仍用网格小图兜住");
  assert.equal(store.overviewImageUrl(), null, "总览不能展示被 3:1 裁切的 grid 小图");
  assert.equal(store.overviewStatus(), "error");
  assert.equal(store.sharp(), false);
});

test("Screen 尚在加载时，主视图可先用 grid，右栏总览等待全图", async () => {
  let completeScreen: ((bytes: Uint8Array) => void) | undefined;
  const store = createViewerStore({
    loadThumb: async () => new Uint8Array([1]),
    loadScreen: () => new Promise<Uint8Array>((resolve) => { completeScreen = resolve; }),
    makeUrl: (bytes) => `blob:${bytes[0]}`,
    revokeUrl: () => undefined,
  });
  store.show(PHOTOS, 0);
  await flush();
  assert.equal(store.imageUrl(), "blob:1");
  assert.equal(store.overviewImageUrl(), null);
  assert.equal(store.overviewStatus(), "loading");
  completeScreen?.(new Uint8Array([2]));
  await flush();
  assert.equal(store.overviewImageUrl(), "blob:2");
  assert.equal(store.overviewStatus(), "ready");
});

test("show：越界的下标会被夹回来（别信任调用方）", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 99);
  assert.equal(store.state().index, 2);
  store.show(PHOTOS, -5);
  assert.equal(store.state().index, 0);
});

test("show/goTo：元数据尺寸随当前照片切换，不沿用上一张", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  const photos: ViewerPhoto[] = [
    { ...PHOTOS[0]!, natural: { width: 4000, height: 3000 } },
    { ...PHOTOS[1]!, natural: { width: 3000, height: 4000 } },
  ];
  store.show(photos, 0);
  assert.deepEqual(store.state().natural, { width: 4000, height: 3000 });
  store.goTo(1);
  assert.deepEqual(store.state().natural, { width: 3000, height: 4000 });
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

test("focus：对比画幅切焦点时保留缩放与平移，只更新当前照片和尺寸", () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  const photos: ViewerPhoto[] = [
    { ...PHOTOS[0]!, natural: { width: 4000, height: 3000 } },
    { ...PHOTOS[1]!, natural: { width: 6000, height: 4000 } },
  ];
  store.setViewport({ width: 800, height: 600 });
  store.show(photos, 0);
  store.zoomTo(1);
  store.panBy(120, -80);
  const before = store.state();

  store.focus(1);

  assert.equal(store.current()?.id, "b");
  assert.equal(store.state().zoom, before.zoom);
  assert.deepEqual(store.state().pan, before.pan);
  assert.equal(store.state().fit, before.fit);
  assert.deepEqual(store.state().natural, { width: 6000, height: 4000 });
});

test("多图 URL：每幅照片持有自己的 URL，关闭时全部回收", async () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 0);

  await Promise.all(PHOTOS.map((photo) => store.ensureImage(photo)));
  const urls = PHOTOS.map((photo) => store.imageUrlFor(photo));
  assert.ok(urls.every((url) => url !== null));
  assert.equal(new Set(urls).size, PHOTOS.length, "不能把最后一张图的 URL 复用给全部画幅");

  store.close();
  for (const url of urls) {
    assert.ok(fake.revoked.includes(url!), `关闭时应回收多图 URL ${url}`);
  }
});

test("多图 URL：取不到其中一张时只让该幅为空，不串用别张图", async () => {
  const urls: string[] = [];
  const store = createViewerStore({
    loadScreen: async (path) => path === "/b.jpg" ? null : new Uint8Array([1]),
    makeUrl: () => {
      const url = `blob:multi-${urls.length + 1}`;
      urls.push(url);
      return url;
    },
    revokeUrl: () => undefined,
  });
  await Promise.all(PHOTOS.map((photo) => store.ensureImage(photo)));
  assert.ok(store.imageUrlFor(PHOTOS[0]!) !== null);
  assert.equal(store.imageUrlFor(PHOTOS[1]!), null);
  assert.ok(store.imageUrlFor(PHOTOS[2]!) !== null);
});

test("loadFor：预载写下的 URL 会被单图路径直接吃掉（不再走第二次 IPC）", async () => {
  let screenCalls = 0;
  const fake = fakeDeps();
  const store = createViewerStore({
    ...fake.deps,
    loadScreen: async (): Promise<Uint8Array> => {
      screenCalls += 1;
      return new Uint8Array([2]);
    },
  });
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  await flush();
  assert.equal(screenCalls, 1, "当前那张发一次");

  await store.ensureImage(PHOTOS[1]!);
  assert.equal(screenCalls, 2, "预载 b 发一次");

  const before = store.imageUrl();
  store.goTo(1);
  await flush();
  assert.equal(screenCalls, 2, "翻到预载过的那张不该再发 IPC");
  assert.equal(store.imageStatus(), "ready");
  assert.equal(store.imageUrl(), "blob:3", "用的就是预载那张的 URL");
  assert.ok(fake.revoked.includes(before!), "上一张的 URL 被回收");
});

test("loadFor：同一张正在预载时**等它**，不发重复请求（RAW 解码是串行的）", async () => {
  let screenCalls = 0;
  const resolvers: ((bytes: Uint8Array) => void)[] = [];
  const fake = fakeDeps();
  const store = createViewerStore({
    ...fake.deps,
    loadScreen: (): Promise<Uint8Array> => {
      screenCalls += 1;
      return new Promise((resolve) => {
        resolvers.push(resolve);
      });
    },
  });
  store.setViewport({ width: 800, height: 600 });
  store.show(PHOTOS, 0);
  await flush(); // loadThumb 是 async 的：loadScreen 要等一个微任务之后才发出来
  resolvers[0]!(new Uint8Array([2]));
  await flush();

  const prefetch = store.ensureImage(PHOTOS[1]!);
  await flush();
  assert.equal(screenCalls, 2, "预载 b 发了一次、挂在途");

  store.goTo(1);
  await flush();
  assert.equal(screenCalls, 2, "用户翻到 b：请求被合并，不许重复发");

  resolvers[1]!(new Uint8Array([3]));
  await prefetch;
  await flush();
  assert.equal(store.imageStatus(), "ready");
  assert.equal(store.imageUrl(), "blob:3");
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

// ─────────────────── 视野框（右栏预览上的那块矩形）───────────────────

function viewerState(overrides: Partial<ViewerState> = {}): ViewerState {
  return {
    active: true,
    photos: [],
    index: 0,
    zoom: 1,
    pan: { x: 0, y: 0 },
    fit: false,
    viewport: { width: 500, height: 250 },
    natural: { width: 1000, height: 500 },
    ...overrides,
  };
}

test("visibleRect：适配窗口时框就是整张", () => {
  const rect = visibleRect(viewerState({ zoom: 0.5 }));
  assert.deepEqual(rect, { x: 0, y: 0, width: 1000, height: 500 });
});

test("visibleRect：缩到比适配还小时也是整张（夹取的结果）", () => {
  const rect = visibleRect(viewerState({ zoom: 0.25 }));
  assert.deepEqual(rect, { x: 0, y: 0, width: 1000, height: 500 });
});

test("visibleRect：100% 居中时是视口那么大的一块，且居中", () => {
  const rect = visibleRect(viewerState({ zoom: 1 }));
  assert.deepEqual(rect, { x: 250, y: 125, width: 500, height: 250 });
});

test("visibleRect：往右下拖（pan 为正）时看到的是图像更靠左上的一块", () => {
  const rect = visibleRect(viewerState({ zoom: 1, pan: { x: 100, y: 50 } }));
  assert.deepEqual(rect, { x: 150, y: 75, width: 500, height: 250 });
});

test("visibleRect：拖出边界时被夹在图像内（不会出现负数或超界）", () => {
  const rect = visibleRect(viewerState({ zoom: 1, pan: { x: -10_000, y: -10_000 } }));
  assert.ok(rect !== null);
  assert.ok(rect.x >= 0 && rect.y >= 0);
  assert.ok(rect.x + rect.width <= 1000);
  assert.ok(rect.y + rect.height <= 500);
});

test("visibleRect：尺寸不全时返回 null（不画一个乱跳的框）", () => {
  assert.equal(visibleRect(viewerState({ viewport: { width: 0, height: 250 } })), null);
  assert.equal(visibleRect(viewerState({ natural: { width: 0, height: 0 } })), null);
  assert.equal(visibleRect(viewerState({ zoom: 0 })), null);
  assert.equal(visibleRect(viewerState({ zoom: Number.NaN })), null);
});

test("visibleRect：与适配倍率一致（fit 状态下框正好等于整张）", () => {
  const viewport = { width: 800, height: 600 };
  const natural = { width: 4000, height: 3000 };
  const fit = computeFitScale(viewport, natural);
  const rect = visibleRect(
    viewerState({ viewport, natural, zoom: fit, pan: { x: 0, y: 0 } }),
  );
  assert.ok(rect !== null);
  assert.equal(Math.round(rect.width), natural.width);
  assert.equal(Math.round(rect.height), natural.height);
});

test("定稿切换重取当前图时保留缩放和旧帧，新的 Screen 到达后原位替换", async () => {
  let finish: ((bytes: Uint8Array) => void) | undefined;
  let calls = 0;
  const store = createViewerStore({
    loadScreen: () => {
      calls += 1;
      if (calls === 1) return Promise.resolve(new Uint8Array([1]));
      return new Promise<Uint8Array>((resolve) => { finish = resolve; });
    },
    makeUrl: (bytes) => `blob:${bytes[0]}`,
    revokeUrl: () => undefined,
  });
  store.setViewport({ width: 800, height: 600 });
  store.show([{ ...PHOTOS[0]!, natural: { width: 1600, height: 1200 } }], 0);
  await flush();
  store.zoomTo(1);
  const before = store.state();
  assert.equal(store.imageUrl(), "blob:1");
  store.reloadCurrent();
  assert.equal(store.imageUrl(), "blob:1", "旧帧留着直到新图解码好");
  assert.equal(store.state().zoom, before.zoom);
  assert.deepEqual(store.state().pan, before.pan);
  finish?.(new Uint8Array([2]));
  await flush();
  assert.equal(store.imageUrl(), "blob:2");
  assert.equal(store.state().zoom, before.zoom);
});


test("源变化失效非锚点比较图，保留无关图片 URL", async () => {
  const fake = fakeDeps();
  const store = createViewerStore(fake.deps);
  store.show(PHOTOS, 0); await flush();
  await Promise.all(PHOTOS.map((photo) => store.ensureImage(photo)));
  const changed = PHOTOS[1]!;
  const before = store.imageUrlFor(changed);
  const unchanged = store.imageUrlFor(PHOTOS[0]!);
  store.invalidatePaths([changed.path]);
  assert.equal(store.imageUrlFor(changed), null);
  assert.equal(store.imageUrlFor(PHOTOS[0]!), unchanged);
  assert.ok(fake.revoked.includes(before!));
  await flush();
  assert.notEqual(store.imageUrlFor(changed), null, "源变更自动重取比较图，不等待重新打开视图");
  assert.notEqual(store.imageUrlFor(changed), before);
});


test("源变化只取消受影响的在途比较图，旧结果不能覆盖新图", async () => {
  const pending = new Map<string, ((bytes: Uint8Array) => void)[]>();
  const revoked: string[] = [];
  const store = createViewerStore({
    loadScreen: (path) => new Promise<Uint8Array>((resolve) => {
      const list = pending.get(path) ?? []; list.push(resolve); pending.set(path, list);
    }),
    makeUrl: (bytes) => `blob:${bytes[0]}`,
    revokeUrl: (url) => revoked.push(url),
  });
  store.show(PHOTOS, 0);
  pending.get("/a.jpg")![0]!(new Uint8Array([1])); await flush();
  const b = store.ensureImage(PHOTOS[1]!);
  const c = store.ensureImage(PHOTOS[2]!);
  store.invalidatePaths(["/b.jpg"]);
  assert.equal(pending.get("/b.jpg")!.length, 2);
  pending.get("/b.jpg")![1]!(new Uint8Array([4])); await flush();
  pending.get("/b.jpg")![0]!(new Uint8Array([2]));
  pending.get("/c.jpg")![0]!(new Uint8Array([3]));
  await Promise.all([b, c]);
  assert.equal(store.imageUrlFor(PHOTOS[1]!), "blob:4");
  assert.equal(store.imageUrlFor(PHOTOS[2]!), "blob:3");
  assert.ok(!revoked.includes("blob:3"));
});
