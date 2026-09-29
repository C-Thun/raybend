import assert from "node:assert/strict";
import test from "node:test";
import {
  LOCK_LEVELS,
  lockCycleTarget,
  lockFilterStep,
  lockStep,
} from "./marking-state.ts";

/* 合并锁键（2026-09-28）：档位、筛选档位与循环目标 —— 语义的唯一来源在 marking-state.ts */

test("lockStep：一致的级别就是档位；mixed / 无值 / 空都当 0", () => {
  assert.equal(lockStep([2, 2]), 2);
  assert.equal(lockStep([1, 1, 1]), 1);
  assert.equal(lockStep([0, 0]), LOCK_LEVELS.none);
  assert.equal(lockStep([null, null]), LOCK_LEVELS.none, "null（没写过锁）当未锁");
  assert.equal(lockStep([1, 2]), LOCK_LEVELS.none, "混合不当档位：从「设一级」重新开始");
  assert.equal(lockStep([2, null]), LOCK_LEVELS.none);
  assert.equal(lockStep([]), LOCK_LEVELS.none, "没选中也是 0（按钮本身会禁用）");
});

test("lockFilterStep：只认单档条件；空与组合都当未设", () => {
  assert.equal(lockFilterStep([LOCK_LEVELS.noDelete]), 1);
  assert.equal(lockFilterStep([LOCK_LEVELS.noEdit]), 2);
  assert.equal(lockFilterStep([]), LOCK_LEVELS.none);
  assert.equal(lockFilterStep(null), LOCK_LEVELS.none, "没这条件 = 不按锁筛选");
  assert.equal(lockFilterStep(undefined), LOCK_LEVELS.none);
  assert.equal(
    lockFilterStep([LOCK_LEVELS.noDelete, LOCK_LEVELS.noEdit]),
    LOCK_LEVELS.none,
    "组合条件不追求覆盖：当未设，点一下收敛成单档",
  );
});

test("lockCycleTarget：0→1→2→（再发 2，交给 toggle 语义解除）", () => {
  assert.equal(lockCycleTarget(0), 1);
  assert.equal(lockCycleTarget(1), 2);
  assert.equal(lockCycleTarget(2), 2, "对已到二级的再发 2 = 解除（mark-actions 的既有 toggle）");
  assert.equal(lockCycleTarget(99), 2, "越界值只会收敛到 2，不会发明新档");
});
