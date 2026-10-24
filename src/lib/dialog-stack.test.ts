/**
 * 弹窗栈的测试：任何时刻**恰好一个**弹窗画遮罩，且顺序按打开先后。
 *
 * Solid 的客户端构建才有同步的 createComputed 行为，所以按 `action-slot.test.ts`
 * 的做法起一个 `--conditions=browser` 子进程。
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("弹窗栈：只有最底层画遮罩，关闭/重开/卸载后恰好一个", () => {
  const url = new URL("./dialog-stack.ts", import.meta.url).href;
  const result = spawnSync(
    process.execPath,
    [
      "--conditions=browser",
      "--input-type=module",
      "-e",
      `
        import assert from 'node:assert/strict';
        import { createRoot, createSignal } from 'solid-js';
        import { createDialogLayer } from ${JSON.stringify(url)};

        createRoot((dispose) => {
          const [a, setA] = createSignal(false);
          const [b, setB] = createSignal(false);
          const [c, setC] = createSignal(false);
          const la = createDialogLayer(a);
          const lb = createDialogLayer(b);
          const lc = createDialogLayer(c);
          const scrims = () => [la, lb, lc].filter((layer) => layer.paintsScrim()).length;

          assert.equal(la.depth(), 0, '关闭时深度为 0');
          assert.equal(scrims(), 0, '没有弹窗时没人画遮罩');

          setA(true);
          assert.equal(la.depth(), 1);
          assert.equal(la.paintsScrim(), true, '唯一的弹窗画遮罩');

          setB(true);
          assert.equal(la.depth(), 1);
          assert.equal(lb.depth(), 2);
          assert.equal(la.paintsScrim(), true, '底层继续画');
          assert.equal(lb.paintsScrim(), false, '二层不画第二层遮罩');
          assert.equal(scrims(), 1);

          setC(true);
          assert.equal(lc.depth(), 3);
          assert.equal(scrims(), 1, '三层仍然只有一层遮罩');

          setA(false);
          assert.equal(lb.depth(), 1);
          assert.equal(lb.paintsScrim(), true, '底层关闭后，二层接管遮罩');
          assert.equal(lc.depth(), 2);
          assert.equal(scrims(), 1);

          setB(false);
          assert.equal(lc.paintsScrim(), true);
          assert.equal(scrims(), 1);

          setA(true);
          assert.equal(lc.depth(), 1);
          assert.equal(la.depth(), 2);
          assert.equal(lc.paintsScrim(), true, '重开的弹窗排到最后 = 最上层');
          assert.equal(la.paintsScrim(), false);
          assert.equal(scrims(), 1);

          dispose();
          assert.equal(la.depth(), 0, '卸载后出栈');
          assert.equal(lc.depth(), 0);
          assert.equal(scrims(), 0);
        });
      `,
    ],
    { encoding: "utf8", timeout: 5000 },
  );
  assert.equal(result.status, 0, result.stderr || String(result.error));
});
