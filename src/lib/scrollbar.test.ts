/**
 * `measureScrollbarWidth` 的单测。
 *
 * 这个仓库的测试是零依赖的 `node --test`（没有 jsdom），所以用**最小替身**：
 * 只验真正要保证的三件事 —— 无 DOM 不抛、量的是 `offsetWidth − clientWidth`、
 * 探针用完必须摘掉（不能往 body 里留垃圾）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { measureScrollbarWidth } from "./scrollbar.ts";

interface FakeProbe {
  style: { cssText: string };
  offsetWidth: number;
  clientWidth: number;
  removed: boolean;
  remove: () => void;
}

/** 造一个「滚动条宽 `scrollbar` 像素」的假 document，并记录被插进来的探针 */
function fakeDocument(scrollbar: number): { doc: Document; probes: FakeProbe[] } {
  const probes: FakeProbe[] = [];
  const doc = {
    body: {
      appendChild(element: FakeProbe) {
        probes.push(element);
      },
    },
    createElement() {
      const probe: FakeProbe = {
        style: { cssText: "" },
        offsetWidth: 100 + scrollbar,
        clientWidth: 100,
        removed: false,
        remove() {
          probe.removed = true;
        },
      };
      return probe;
    },
  };
  return { doc: doc as unknown as Document, probes };
}

/** 临时把 `globalThis.document` 换成替身，返回还原函数 */
function withDocument(doc: Document): () => void {
  const holder = globalThis as { document?: Document };
  const original = holder.document;
  holder.document = doc;
  return () => {
    holder.document = original;
  };
}

test("无 DOM（Node 环境）时返回 fallback，不抛", () => {
  assert.equal(measureScrollbarWidth(), 8);
  assert.equal(measureScrollbarWidth(12), 12);
});

test("有 DOM 时量的是 offsetWidth − clientWidth，探针用完就摘掉", () => {
  const { doc, probes } = fakeDocument(10);
  const restore = withDocument(doc);
  try {
    assert.equal(measureScrollbarWidth(), 10);
    assert.equal(probes.length, 1, "只插一个探针");
    assert.equal(probes[0]?.removed, true, "探针必须摘掉（别往 body 里留垃圾）");
    assert.match(probes[0]?.style.cssText ?? "", /overflow:scroll/, "探针必须强制出滚动条");
    assert.match(probes[0]?.style.cssText ?? "", /position:absolute/);
  } finally {
    restore();
  }
});

test("量出来是 0（没有经典滚动条）时退回 fallback —— 0 不能当宽度用", () => {
  const { doc, probes } = fakeDocument(0);
  const restore = withDocument(doc);
  try {
    assert.equal(measureScrollbarWidth(8), 8);
    assert.equal(probes[0]?.removed, true, "即使退回 fallback，探针也要摘掉");
  } finally {
    restore();
  }
});
