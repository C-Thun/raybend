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

/** 图片区宽高比 3:2 —— 主流相机的横构图比例 */
export const TILE_IMAGE_ASPECT = 3 / 2;

/**
 * 图片区高度（不含字幕条）。
 * 取整到像素，避免半像素导致的图片发虚。
 */
export function tileImageHeight(cellWidth: number): number {
 if (!Number.isFinite(cellWidth) || cellWidth <= 0) return 0;
 return Math.round(cellWidth / TILE_IMAGE_ASPECT);
}

/** 整块 tile 的高度 = 图片区 + 字幕条（字幕条高度由密度档位给出） */
export function tileTotalHeight(
 cellWidth: number,
 captionHeight: number,
): number {
 const caption =
  Number.isFinite(captionHeight) && captionHeight > 0 ? captionHeight : 0;
 return tileImageHeight(cellWidth) + caption;
}

/**
 * 计算整个网格的行数（用于虚拟化的总高度估算）。
 * `count` 为照片总数。
 */
export function tileRowCount(count: number, columns: number): number {
 if (!Number.isFinite(count) || count <= 0) return 0;
 const cols = Math.max(1, Math.floor(columns));
 return Math.ceil(count / cols);
}
