/**
 * 画廊里的「数据库升级遮罩」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：升级遮罩只在**真要跑迁移**时才出现（`db://migration` 事件），
 * 而画廊页（以及冒烟脚本跑的 Chromium）里没有后端 —— 于是这个组件用
 * **真的状态机 + 假通知**把它驱动起来：
 *
 * * 真状态机：`features/migration/notice.ts`（生产同一份代码，含「按库种类记账」）；
 * * 假通知：按钮往状态机里塞一条 `MigrationNotice`。
 *
 * 于是 `scripts/ui-smoke.mjs` 能断言「遮罩出现 → 键盘被挡住 → 收到 Done 后消失」，
 * 人也能点着看。
 */

import { createSignal, Show } from "solid-js";
import { Button } from "../components/ui/Button.tsx";
import { MigrationGate, applyNotice, NO_MIGRATIONS, type MigrationMap } from "../features/migration/index.ts";
import type { MigrationNotice } from "../api/types.ts";

/** 一次典型的升级：`catalog.db` v3 → v4。 */
const CATALOG: MigrationNotice = {
  kind: "catalog",
  label: "照片库",
  from: 3,
  to: 4,
  running: true,
};

export function MigrationGateDemo() {
  const [notices, setNotices] = createSignal<MigrationMap>(NO_MIGRATIONS);
  const feed = (notice: MigrationNotice): void => {
    setNotices((previous) => applyNotice(previous, notice));
  };
  const running = (): boolean => notices().size > 0;

  return (
    <div class="flex flex-col gap-2" data-migration-demo>
      <div class="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={running()}
          onClick={() => feed(CATALOG)}
        >
          发一条升级通知
        </Button>
        <Button
          size="sm"
          disabled={!running()}
          onClick={() => feed({ ...CATALOG, running: false })}
        >
          发一条完成通知
        </Button>
        <span class="text-fs-1 text-fg-3">
          遮罩开着：{running() ? "是" : "否"}
        </span>
      </div>

      {/* 遮罩本身（生产同一份组件） */}
      <MigrationGate notices={notices()} />

      <Show when={running()}>
        <p class="text-fs-0 text-fg-3">
          （遮罩是全窗口的：它盖住整页，点在别处不会有任何反应）
        </p>
      </Show>
    </div>
  );
}
