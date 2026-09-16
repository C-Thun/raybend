/**
 * Tile 流换行布局（DESIGN.md §12.6）。
 *
 * 规则（用户明确指定）：
 *   1. **除最后一行外，每行列数相同** —— 前提是 tile 尺寸固定，不随容器变化
 *   2. 列数 `N = max(1, floor((W + gap) / (cellW + gap)))`
 *   3. 剩余空间 `L = W − (N·cellW + (N−1)·gap)` **全部作为右边距**
 *   4. **不做居中、不做拉伸填空**
 *
 * 为什么不允许拉伸：拉伸会让 tile 尺寸随窗口宽度连续变化，破坏「网格是均质照片阵列」
 * 这一认知，也让用户在缩放窗口时不断看到照片变大变小。留白则稳定，且让视线有喘息处。
 */

export interface TileFlowInput {
 /** 容器内容宽（**必须已扣掉容器自身的左右内边距**） */
 containerWidth: number;
 /** 单元格宽（含图片与字幕的整块宽度） */
 cellWidth: number;
 /** 列间距（由密度档位决定：紧凑 4 / 宽松 8） */
 gap: number;
}

export interface TileFlow {
 /** 每行列数（除最后一行）。恒 ≥ 1 */
 columns: number;
 /**
  * 右侧剩余空间，**全部作为右边距**。
  * 当容器比一个单元格还窄时为**负数** —— 此时单元格横向溢出，
  * 由调用方决定是滚动还是收紧 cellWidth（见 `overflows`）。
  */
 remainder: number;
 /** 单行放不下一个单元格（`remainder < 0`） */
 overflows: boolean;
}

/**
 * 计算一行能放几个 tile、以及右侧留白多少。
 *
 * @example
 * computeTileFlow({ containerWidth: 960, cellWidth: 150, gap: 8 })
 * // → { columns: 6, remainder: 20, overflows: false }
 */
export function computeTileFlow(input: TileFlowInput): TileFlow {
 const containerWidth = Number.isFinite(input.containerWidth)
  ? input.containerWidth
  : 0;
 const cellWidth = Number.isFinite(input.cellWidth) ? input.cellWidth : 0;
 // 负间距是无意义输入，按 0 处理而不是让公式产出一个诡异的列数
 const gap = Number.isFinite(input.gap) ? Math.max(0, input.gap) : 0;

 // 退化：单元格宽非法 → 一行一个，剩余为容器宽（不做除零）
 if (cellWidth <= 0) {
  return { columns: 1, remainder: containerWidth, overflows: false };
 }

 const columns = Math.max(
  1,
  Math.floor((containerWidth + gap) / (cellWidth + gap)),
 );
 const used = columns * cellWidth + (columns - 1) * gap;
 const remainder = containerWidth - used;

 return { columns, remainder, overflows: remainder < 0 };
}

/* ══════════════════════════════════════════════════════════════
 * 缩放档位（用户指定：奇数档、最大 512、倾向多档）
 *
 * 9 档 + 最大值 512 + 奇数个 → **存在一个中间档**，且它适合作为默认值。
 * 取值按约 1.25 倍递增，并取整齐的数字，便于在界面上显示与记忆。
 * ══════════════════════════════════════════════════════════════ */

/**
 * 9 档尺寸 —— **正方外框的边长**（2026-09-16 起格子是正方形）。
 *
 * 之所以是正方：照片宽高比千差万别（M43 是 4:3、竖拍是 3:4、还有全景），
 * 而「tile 尺寸可调 + 左列宽度可拖」要求**行高必须恒定** —— 否则拖动时行内成员一变，
 * 界面会扭成迪斯科舞厅。于是：**格子定形状（正方），照片在里面保比例居中**。
 */
export const TILE_SIZE_STEPS = [
 96, 120, 144, 176, 208, 256, 320, 400, 512,
] as const;

export type TileSizeStep = (typeof TILE_SIZE_STEPS)[number];

/** 默认档位下标：正中间那档（208px）。奇数档位的意义就在这里。 */
export const DEFAULT_TILE_STEP_INDEX = (TILE_SIZE_STEPS.length - 1) / 2;

/** 把任意下标夹到合法范围（滑块拖到边界外、配置读到脏值都要经过它） */
export function clampTileStepIndex(index: number): number {
 if (!Number.isFinite(index)) return DEFAULT_TILE_STEP_INDEX;
 return Math.min(TILE_SIZE_STEPS.length - 1, Math.max(0, Math.round(index)));
}

/** 取某一档的单元格宽度 */
export function tileSizeAt(index: number): TileSizeStep {
 return TILE_SIZE_STEPS[clampTileStepIndex(index)];
}

/**
 * 元信息还没到时的**占位比例**（3:2，主流横构图）。
 *
 * 元信息迟到不会重排网格：外框是正方、边长由档位决定，照片只是在框里长大/缩小。
 */
export const DEFAULT_DISPLAY_ASPECT = 3 / 2;

/**
 * 展示用的最大宽高比（3:1 与 1:3）—— **必须与 Rust 侧 `MAX_DISPLAY_ASPECT` 一致**。
 *
 * 超出这个范围的照片（全景、接片）在网格里按 3:1 居中截取显示；
 * 原图不受影响，看图（`Screen` 档）永远看完整的。
 */
export const MAX_DISPLAY_ASPECT = 3;

/**
 * 算出「展示用」的宽高比：方向已由后端应用（竖图就是 w < h），
 * 超出 3:1 / 1:3 的按 3:1 夹取；尺寸未知（`0`）时退回默认占位比例。
 */
export function clampDisplayAspect(width: number, height: number): number {
 if (
  !Number.isFinite(width) ||
  !Number.isFinite(height) ||
  width <= 0 ||
  height <= 0
 ) {
  return DEFAULT_DISPLAY_ASPECT;
 }
 const aspect = width / height;
 if (aspect > MAX_DISPLAY_ASPECT) return MAX_DISPLAY_ASPECT;
 if (aspect < 1 / MAX_DISPLAY_ASPECT) return 1 / MAX_DISPLAY_ASPECT;
 return aspect;
}

/**
 * 一行 tile 的高度 = **正方外框的边长**（信息条是覆盖层，不占高度）。
 *
 * 保留成函数是为了让「行高从哪来」只有一个出处：视图、虚拟化、
 * 以及测试都读它，别各自算。
 */
export function tileRowHeight(cellSize: number): number {
 if (!Number.isFinite(cellSize) || cellSize <= 0) return 0;
 return Math.round(cellSize);
}

/**
 * 计算整个网格的行数（用于虚拟化的总高度估算）。
 * `count` 为照片总数。
 */
export function tileRowCount(count: number, columns: number): number {
 if (!Number.isFinite(count) || count <= 0) return 0;
 // 列数非法时退化为「每行一张」。
 // 注意不能只写 Math.max(1, Math.floor(columns)) —— NaN 会穿透 Math.max，
 // 结果整个变成 NaN，进而让虚拟化的总高度算不出来。
 const cols = Number.isFinite(columns) ? Math.max(1, Math.floor(columns)) : 1;
 return Math.ceil(count / cols);
}

/** 网格里按方向键时，选中应当落到第几项（`null` = 不动） */
export function nextIndexForArrow(input: {
  /** 当前选中的下标（不在列表里时传 -1） */
  from: number;
  count: number;
  columns: number;
  key: "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown";
}): number | null {
  const { from, count, columns, key } = input;
  if (count <= 0 || from < 0 || from >= count) return null;
  const step =
    key === "ArrowLeft"
      ? -1
      : key === "ArrowRight"
        ? 1
        : key === "ArrowUp"
          ? -Math.max(1, columns)
          : Math.max(1, columns);
  /*
   * 上下移动按**整行**跳（列数由换行算法给出）。越界就**停在原地**：
   * 从第一张按左、从最后一张按右都不动 —— 不绕到另一端（绕过去像是选中莫名跳走了）。
   */
  const next = from + step;
  if (next < 0 || next >= count) return null;
  return next;
}
