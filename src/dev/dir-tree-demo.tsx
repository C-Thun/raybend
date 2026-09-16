/**
 * 画廊里的「目录树」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：`DirTree` 是**通用组件**（导入工作区与将来的浏览侧共用一份），
 * 而它需要真文件系统才能跑起来 —— 浏览器里没有后端，所以这里用**假目录树**驱动，
 * 一次展示两种用法：
 *
 * * `data-variant="checkable"` —— 导入侧：传 `onToggleCheck`，有勾选圈；
 * * `data-variant="plain"` —— 浏览侧：**不传** `onToggleCheck`，没有勾选圈。
 *
 * 顺带让 `pnpm smoke:ui` 能验三件事（都是浏览器里唯一能验的地方）：
 *   1. 勾选圈的有无确实由「传没传回调」决定；
 *   2. **双击行名能展开/折叠**（与点箭头同效）;
 *   3. 滚动这棵树时的**样式重算/布局开销**（用来盯住「滚动发粘」这类回归）。
 */

import { createSignal } from "solid-js";
import type { DirEntry, Volume } from "../api/types.ts";
import { DirTree } from "../features/dir-tree/index.ts";

/** 每层几个子目录（够展开出几十行，用于滚动测量）。 */
const BREADTH = 6;
/** 到第几层为止还有子目录。 */
const MAX_DEPTH = 4;

const DEMO_VOLUMES: Volume[] = [
  { path: "D:\\", kind: "local", kindLabel: "本地磁盘" },
];

/** 造一棵「每层 6 个、共 4 层」的假目录树：路径 → 子目录。 */
function demoChildren(path: string): DirEntry[] {
  const segments = path.split("\\").filter(Boolean);
  if (segments.length >= MAX_DEPTH + 1) return [];
  const tail = segments[segments.length - 1] ?? "D";
  return Array.from({ length: BREADTH }, (_, index) => {
    const name = `${tail === "D:" ? "文件夹" : tail}-${index + 1}`;
    return { name, path: `${path}\\${name}` };
  });
}

export function DirTreeDemo() {
  const [selected, setSelected] = createSignal<string | null>("D:\\");
  const [checked, setChecked] = createSignal<Record<string, boolean>>({});

  // 假的后端：15ms 之后给出子目录（真实实现走 `dir_list` 命令）
  const loadDirs = (path: string): Promise<DirEntry[]> =>
    new Promise((resolve) => {
      setTimeout(() => {
        resolve(demoChildren(path));
      }, 15);
    });

  const shared = () => ({
    volumes: DEMO_VOLUMES,
    status: "ready" as const,
    error: null,
    loadDirs,
    isSelected: (path: string) => selected() === path,
    onSelect: (path: string) => setSelected(path),
  });

  return (
    <div class="grid grid-cols-2 gap-gap" data-demo="dir-tree">
      <div
        data-variant="checkable"
        class="flex h-72 min-h-0 flex-col rounded-ui bg-surface-main p-1"
      >
        <span class="shrink-0 px-1 text-fs-0 text-fg-3">
          导入变体（可勾选：勾选 = 加入待导入集合）
        </span>
        <DirTree
          {...shared()}
          isChecked={(path) => checked()[path] === true}
          onToggleCheck={(path, next) =>
            setChecked((prev) => ({ ...prev, [path]: next }))
          }
        />
      </div>

      <div
        data-variant="plain"
        class="flex h-72 min-h-0 flex-col rounded-ui bg-surface-main p-1"
      >
        <span class="shrink-0 px-1 text-fs-0 text-fg-3">
          浏览变体（**不可勾选**：不传 onToggleCheck 就没有勾选圈）
        </span>
        <DirTree {...shared()} />
      </div>
    </div>
  );
}
