/**
 * `easy destroy`（DESIGN.md §12.2）的单元测试。
 *
 * 这是**会真的动数据**的一条路径，所以要覆盖：
 * Shift 跳过 / 不跳过 / 取消不执行 / 确认才执行 / 重复请求的覆盖行为 /
 * 执行中抛错不能把弹窗卡在「已确认」态。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { createEasyDestroy, shouldSkipConfirm } from "./easy-destroy.ts";

test("shouldSkipConfirm：只认 shiftKey", () => {
  assert.equal(shouldSkipConfirm({ shiftKey: true }), true);
  assert.equal(shouldSkipConfirm({ shiftKey: false }), false);
  assert.equal(shouldSkipConfirm({}), false);
  assert.equal(shouldSkipConfirm(undefined), false, "普通点击（没有事件对象）不该跳过");
  assert.equal(shouldSkipConfirm(null), false);
});

test("按 Shift 点击 → 直接执行，且不弹确认框", () => {
  const destroy = createEasyDestroy();
  let runs = 0;
  const outcome = destroy.request("确定要移除吗？", () => {
      runs++;
    }, { shiftKey: true });

  assert.equal(outcome, "ran");
  assert.equal(destroy.pending(), null, "跳过的操作不该留下弹窗");
  assert.equal(runs, 1);
});

test("普通点击 → 挂起，先不执行", () => {
  const destroy = createEasyDestroy();
  let runs = 0;
  const outcome = destroy.request("确定要移除吗？", () => {
      runs++;
    }, {
    shiftKey: false,
  });

  assert.equal(outcome, "asked");
  assert.equal(runs, 0, "没有确认就不该动数据");
  assert.deepEqual(destroy.pending()?.message, "确定要移除吗？");
});

test("确认 → 执行并关闭弹窗", () => {
  const destroy = createEasyDestroy();
  let runs = 0;
  destroy.request("移除 A", () => {
      runs++;
    });
  assert.equal(runs, 0);

  destroy.confirm();
  assert.equal(runs, 1);
  assert.equal(destroy.pending(), null);
});

test("取消 → 不执行并关闭弹窗", () => {
  const destroy = createEasyDestroy();
  let runs = 0;
  destroy.request("移除 A", () => {
      runs++;
    });
  destroy.cancel();

  assert.equal(runs, 0);
  assert.equal(destroy.pending(), null);
});

test("重复确认 / 无请求时确认：不会执行两次、也不抛错", () => {
  const destroy = createEasyDestroy();
  let runs = 0;
  destroy.confirm();
  assert.equal(runs, 0, "没有待确认请求时 confirm 是空操作");

  destroy.request("移除 A", () => {
      runs++;
    });
  destroy.confirm();
  destroy.confirm();
  destroy.cancel();

  assert.equal(runs, 1, "确认一次只能执行一次");
});

test("连续发起两次 → 后一个覆盖前一个（不留过期文案）", () => {
  const destroy = createEasyDestroy();
  let aRuns = 0;
  let bRuns = 0;

  destroy.request("移除 A", () => {
      aRuns++;
    });
  destroy.request("移除 B", () => {
      bRuns++;
    });
  assert.equal(destroy.pending()?.message, "移除 B");

  destroy.confirm();
  assert.equal(bRuns, 1);
  assert.equal(aRuns, 0, "被覆盖的请求不该在确认后突然执行");
  assert.equal(destroy.pending(), null);
});

test("标题可选：不传就是 undefined，不是空串", () => {
  const destroy = createEasyDestroy();
  destroy.request("移除 A", () => {});
  assert.equal(destroy.pending()?.title, undefined);
  destroy.cancel();

  destroy.request("移除 A", () => {}, undefined, "确认移除");
  assert.equal(destroy.pending()?.title, "确认移除");
});

test("run 是异步的：确认后弹窗立刻关闭，执行在后台完成", async () => {
  const destroy = createEasyDestroy();
  let done = false;
  destroy.request("移除 A", async () => {
    await new Promise((resolve) => setTimeout(resolve, 5));
    done = true;
  });

  destroy.confirm();
  assert.equal(destroy.pending(), null, "确认后不该继续挡着界面");
  assert.equal(done, false, "异步执行还没跑完");

  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(done, true);
});

test("run 抛错也不会把弹窗留在「已确认但未生效」的状态", async () => {
  const destroy = createEasyDestroy();
  destroy.request("移除 A", () => {
    throw new Error("磁盘拒绝了这次移除");
  });

  destroy.confirm();
  assert.equal(destroy.pending(), null);

  // 让异步链里的异常有机会冒出来；这里只关心状态，不关心控制台噪声
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(destroy.pending(), null);
});
