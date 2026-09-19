import assert from "node:assert/strict";
import test from "node:test";

import {
  isViewerControlTarget,
  viewerControlsVisible,
} from "./interaction.ts";

test("isViewerControlTarget：按钮内的图标也算控件，普通画面不算", () => {
  let selector = "";
  const iconInsideButton = {
    closest: (value: string): object | null => {
      selector = value;
      return {};
    },
  } as unknown as EventTarget;
  assert.equal(isViewerControlTarget(iconInsideButton), true);
  assert.match(selector, /button/);

  const canvas = {
    closest: (): null => null,
  } as unknown as EventTarget;
  assert.equal(isViewerControlTarget(canvas), false);
  assert.equal(isViewerControlTarget(null), false);
});

test("viewerControlsVisible：左上与右下触发，中央和离场不触发", () => {
  const viewport = { width: 1200, height: 800 };
  assert.equal(viewerControlsVisible({ x: 80, y: 80 }, viewport), true);
  assert.equal(viewerControlsVisible({ x: 900, y: 750 }, viewport), true);
  assert.equal(viewerControlsVisible({ x: 500, y: 400 }, viewport), false);
  assert.equal(viewerControlsVisible(null, viewport), false);
});
