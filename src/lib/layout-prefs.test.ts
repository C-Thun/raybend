/**
 * `layout-prefs` 的单测：范围夹取、垃圾值回落、读写往返、拖拽结束落盘。
 *
 * 这些看着琐碎，但它们是**唯一**保证「比例不会被自己的存储搞坏」的地方 ——
 * 存储里的一个坏值就可能让左列宽度变成 0（界面看着像坏了）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  createLayoutStore,
  DEFAULT_LAYOUT,
  LAYOUT_BOUNDS,
  LAYOUT_STORAGE_KEY,
  readLayout,
  sanitizeLayout,
  writeLayout,
  type LayoutStorage,
} from "./layout-prefs.ts";

/** 内存存储替身（并且能模拟「存储不可用」）。 */
function memoryStorage(initial: Record<string, string> = {}): LayoutStorage & {
  data: Map<string, string>;
} {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
}

test("sanitizeLayout：非法输入一律回落默认，绝不抛错", () => {
  for (const garbage of [null, undefined, 42, "不是对象", [], true]) {
    assert.deepEqual(sanitizeLayout(garbage), DEFAULT_LAYOUT, `${String(garbage)}`);
  }
});

test("sanitizeLayout：超范围的值被夹到边界（而不是原样接受）", () => {
  const tooSmall = sanitizeLayout({ leftRatio: -3, recentRatio: 0 });
  assert.equal(tooSmall.leftRatio, LAYOUT_BOUNDS.leftRatio.min);
  assert.equal(tooSmall.recentRatio, LAYOUT_BOUNDS.recentRatio.min);

  const tooBig = sanitizeLayout({ leftRatio: 9, recentRatio: 5 });
  assert.equal(tooBig.leftRatio, LAYOUT_BOUNDS.leftRatio.max);
  assert.equal(tooBig.recentRatio, LAYOUT_BOUNDS.recentRatio.max);
});

test("sanitizeLayout：只给一半时另一半用默认值", () => {
  const half = sanitizeLayout({ leftRatio: 0.4 });
  assert.equal(half.leftRatio, 0.4);
  assert.equal(half.recentRatio, DEFAULT_LAYOUT.recentRatio);
});

test("readLayout：没存过 / 存了垃圾 / 存了空串，都安全回到默认", () => {
  assert.deepEqual(readLayout(memoryStorage()), DEFAULT_LAYOUT);
  assert.deepEqual(
    readLayout(memoryStorage({ [LAYOUT_STORAGE_KEY]: "{不是 json" })),
    DEFAULT_LAYOUT,
  );
  assert.deepEqual(readLayout(memoryStorage({ [LAYOUT_STORAGE_KEY]: "" })), DEFAULT_LAYOUT);
  assert.deepEqual(readLayout(undefined), DEFAULT_LAYOUT);
});

test("write → read 往返一致（这就是「重启后还原」的核心）", () => {
  const storage = memoryStorage();
  const prefs = { leftRatio: 0.37, recentRatio: 0.55 };
  writeLayout(prefs, storage);
  assert.deepEqual(readLayout(storage), prefs);
});

test("存储不可用时：读给默认、写不抛错", () => {
  assert.doesNotThrow(() => {
    writeLayout({ leftRatio: 0.3, recentRatio: 0.3 }, undefined);
  });
  assert.deepEqual(readLayout(undefined), DEFAULT_LAYOUT);
});

test("店：拖拽结束写入比例、并立刻落盘；读出来是夹取后的值", () => {
  const storage = memoryStorage();
  const store = createLayoutStore({ storage });
  assert.deepEqual(store.prefs(), DEFAULT_LAYOUT);

  store.setLeftRatio(0.41);
  assert.equal(store.prefs().leftRatio, 0.41);
  assert.equal(store.prefs().recentRatio, DEFAULT_LAYOUT.recentRatio, "不该动另一项");
  assert.deepEqual(readLayout(storage), store.prefs(), "落盘了");

  store.setRecentRatio(0.6);
  assert.equal(store.prefs().recentRatio, 0.6);

  // 越界：写进去的是夹取后的值，存储里也不会出现坏值
  store.setLeftRatio(0.99);
  assert.equal(store.prefs().leftRatio, LAYOUT_BOUNDS.leftRatio.max);
  assert.equal(readLayout(storage).leftRatio, LAYOUT_BOUNDS.leftRatio.max);
});

test("店：换一个实例能读回上一个实例写的比例（模拟重启）", () => {
  const storage = memoryStorage();
  createLayoutStore({ storage }).setLeftRatio(0.33);
  const restarted = createLayoutStore({ storage });
  assert.equal(restarted.prefs().leftRatio, 0.33);
});

test("值没变时不写、也不更新信号（防 resize 回路：真机启动卡顿 + 最大化卡死）", () => {
  const storage = memoryStorage();
  const store = createLayoutStore({ storage });
  store.setLeftRatio(0.31);
  const before = store.prefs();

  // 同一个值再写：引用必须**不变**（信号没更新，上层就不会重渲染）
  store.setLeftRatio(0.31);
  assert.equal(store.prefs(), before, "同一个比例不该触发新的对象");

  // 越界值被夹到边界后与当前相同，同样不该动
  store.setLeftRatio(0.5);
  const atMax = store.prefs();
  store.setLeftRatio(9);
  assert.equal(store.prefs(), atMax, "夹取后仍是同一个值，就不该动");
});
