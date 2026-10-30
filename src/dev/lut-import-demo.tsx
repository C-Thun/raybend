/**
 * 画廊里的「LUT 导入进度」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：LUT 导入弹窗只有在**编辑器打开且真有 LUT 目录**时才打得开，
 * 而画廊页（以及冒烟脚本跑的 Chromium）里没有后端 —— 于是这里用**真的进度组件**
 * （`features/editor/lut-import-progress.tsx`）+ 假的进度快照，把三种状态摆出来：
 *
 * * **扫描中**：还没有总数 → 进度条走不确定态；
 * * **复制中**：总数已知 → 计数 / 百分比 / 当前文件；
 * * **中止中**：点了取消，等当前文件做完。
 *
 * `scripts/ui-smoke.mjs` 对它的结构做断言；人也能点着看。
 */

import { createSignal, Show } from "solid-js";

import { Button } from "../components/ui/Button.tsx";
import { LutImportProgressView } from "../features/editor/lut-import-progress.tsx";
import type { LutImportProgress } from "../api/lut.ts";

const scan: LutImportProgress = {
  phase: "scan",
  done: 0,
  total: 0,
  current: null,
  imported: 0,
  duplicates: 0,
  skipped: 0,
  cancelled: false,
};

const copy: LutImportProgress = {
  phase: "copy",
  done: 7,
  total: 12,
  current: "Kodak 2383.cube",
  imported: 6,
  duplicates: 1,
  skipped: 0,
  cancelled: false,
};

export function LutImportDemo() {
  const [progress, setProgress] = createSignal<LutImportProgress>(scan);
  const [stopping, setStopping] = createSignal(false);

  return (
    <div data-lut-import-demo class="flex flex-col gap-3">
      <div class="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => {
            setStopping(false);
            setProgress(scan);
          }}
        >
          扫描中
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => {
            setStopping(false);
            setProgress(copy);
          }}
        >
          复制中 7/12
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setStopping(true)}
        >
          点取消（中止中）
        </Button>
        <Show when={stopping()}>
          <span class="text-fs-0 text-fg-3">已请求中止，等当前文件处理完</span>
        </Show>
      </div>

      <div class="w-105 rounded-ui bg-surface-layer p-5">
        <LutImportProgressView
          progress={progress()}
          stopping={stopping()}
          categoryName="胶片模拟"
        />
      </div>
    </div>
  );
}
