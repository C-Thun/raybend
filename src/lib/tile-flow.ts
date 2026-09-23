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
 /** 列间距（`--tile-gap`；**不随密度变** —— tiles 不参与松紧调节，人类 2026-09-23 定） */
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
 * 缩放档位（人类指定：**17 档、最大 400**、越细越好）
 *
 * 2026-09-20 人类第二次重排：「明显减小 tiles 放大的最大尺寸……放到 400 上下就够了，
 * 然后把这 9 级过渡改成 **17 级**过渡，让尺寸变化变得更加细腻，方便适配不同的屏幕尺寸」。
 * 所以：512 → **400 封顶**；9 档 → **17 档**（约 ×1.074 一级，旧表约 ×1.19）。
 * 档位数是奇数的传统保留（存在正中间那档）。
 * ══════════════════════════════════════════════════════════════ */

/**
 * 17 档尺寸 —— **正方外框的边长**（2026-09-16 起格子是正方形）。
 *
 * 之所以是正方：照片宽高比千差万别（M43 是 4:3、竖拍是 3:4、还有全景），
 * 而「tile 尺寸可调 + 左列宽度可拖」要求**行高必须恒定** —— 否则拖动时行内成员一变，
 * 界面会扭成迪斯科舞厅。于是：**格子定形状（正方），照片在里面保比例居中**。
 *
 * 阶梯史（改它之前先读这里）：
 *   * 初版 9 档 `96,120,144,176,208,256,320,400,512`；
 *   * 2026-09-20 上午：最小档太小（一页太多图）⇒ 抬到 128 起、仍到 512 收；
 *   * 2026-09-20 下午：**400 封顶 + 17 档**，每级约 +7.4%；
 *   * **2026-09-23（本表）：320 封顶（−20%）+ 17 档，每级约 +5.9%** ——
 *     人类要求「最大尺寸缩小 20%，重排 17 档使过渡更平滑」：封顶降下来、
 *     每级比例从 7.4% 收到 5.9%，拖动滑块时相邻两档的跳跃感明显变小。
 */
export const TILE_SIZE_STEPS = [
  128, 136, 144, 152, 160, 170, 180, 192, 202, 214, 226, 240, 254, 270, 286, 302, 320,
] as const;

export type TileSizeStep = (typeof TILE_SIZE_STEPS)[number];

/**
 * 最大档（人类 2026-09-23：「最大尺寸边长缩小 20%」）。
 *
 * 自适应宽度（`fitTileSizeToRow`）**不许超过它** —— 超过时那个按钮直接无效
 * （见 `canFitRow`）：算不出能铺满的档，就不能假装铺得满。
 */
export const MAX_TILE_SIZE: number = TILE_SIZE_STEPS[TILE_SIZE_STEPS.length - 1];

/**
 * 默认档位下标：**254**（新表里的第 13 档）。
 *
 * 旧表的默认是 256 → 上一版是 260 → 这一版全表缩了 20%，按尺寸最接近的档取 254
 * （差 0.8%，肉眼无差）。阶梯均匀优先于保住某个具体数字。
 */
export const DEFAULT_TILE_STEP_INDEX = 12;

/**
 * 旧表（9 档、512 封顶）—— **只为把存过的档位下标换算成新表的等价尺寸**。
 *
 * 存储里存的是**下标**，而两张表的含义不同：旧 `8`（512）在新表里是 `228`。
 * 直接夹取会让「一直用最大档」的人忽然看到中等尺寸，所以按**尺寸最接近**换算
 * （`lib/display-prefs.ts` 的 `migrateTileStepIndex` 用它，读完就写回新下标）。
 */
export const LEGACY_TILE_SIZE_STEPS = [128, 152, 180, 216, 256, 304, 360, 432, 512] as const;

/** 把旧表的下标换算成新表里尺寸最接近的那一档（尺寸相等时取靠前的）。 */
export function migrateTileStepIndex(legacyIndex: number): number {
  if (!Number.isFinite(legacyIndex)) return DEFAULT_TILE_STEP_INDEX;
  const at = Math.min(
    LEGACY_TILE_SIZE_STEPS.length - 1,
    Math.max(0, Math.round(legacyIndex)),
  );
  const size = LEGACY_TILE_SIZE_STEPS[at];
  let best = 0;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (let i = 0; i < TILE_SIZE_STEPS.length; i += 1) {
    const distance = Math.abs(TILE_SIZE_STEPS[i] - size);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = i;
    }
  }
  return best;
}

/**
 * 把任意档位位置夹到合法范围。
 *
 * 位置允许是小数：`10.5` 表示正好落在第 10、11 个预设档之间。状态栏的
 * 「适合窗口」会算出这种值，若在这里取整，按钮一按完滑块与 tile 就会互相跳。
 */
export function clampTileStepIndex(index: number): number {
 if (!Number.isFinite(index)) return DEFAULT_TILE_STEP_INDEX;
 return Math.min(TILE_SIZE_STEPS.length - 1, Math.max(0, index));
}

/**
 * 取档位位置对应的单元格宽度。整数位置命中预设档，小数位置在相邻两档间线性插值。
 */
export function tileSizeAt(index: number): number {
 const at = clampTileStepIndex(index);
 const lower = Math.floor(at);
 const upper = Math.ceil(at);
 const from = TILE_SIZE_STEPS[lower] ?? TILE_SIZE_STEPS[0];
 const to = TILE_SIZE_STEPS[upper] ?? TILE_SIZE_STEPS[TILE_SIZE_STEPS.length - 1];
 if (lower === upper) return from;
 return from + (to - from) * (at - lower);
}

/**
 * 实际尺寸 → 连续档位位置（`tileSizeAt` 的反函数）。
 *
 * 用于「适合窗口」：计算结果通常不正好命中 17 个预设值，必须把它放到两档之间，
 * 才能让按钮、进度条与后续缩放共用同一条连续坐标。
 */
export function tilePositionForSize(size: number): number {
 if (!Number.isFinite(size)) return DEFAULT_TILE_STEP_INDEX;
 if (size <= TILE_SIZE_STEPS[0]) return 0;
 const last = TILE_SIZE_STEPS.length - 1;
 if (size >= TILE_SIZE_STEPS[last]) return last;
 for (let index = 0; index < last; index += 1) {
  const from = TILE_SIZE_STEPS[index];
  const to = TILE_SIZE_STEPS[index + 1];
  if (size > to) continue;
  return index + (size - from) / (to - from);
 }
 return last;
}

/**
 * 从任意连续位置走到相邻的**预设档**。加减命令与 Ctrl+滚轮用它，避免从 10.6
 * 直接 `+1` 跳到 11.6（那会永远错开预设档）。
 */
export function nextTilePresetPosition(position: number, direction: -1 | 1): number {
 const at = clampTileStepIndex(position);
 if (direction > 0) return Math.min(TILE_SIZE_STEPS.length - 1, Math.floor(at + 1e-9) + 1);
 return Math.max(0, Math.ceil(at - 1e-9) - 1);
}

/**
 * 按当前列数把一行的右侧余量均匀吃掉，返回刚好铺满的 cell 尺寸。
 * 正常情况只会放大；单格已经横向溢出时会收小到容器宽，极端窄窗口也能自救。
 */
export function fitTileSizeToRow(input: TileFlowInput): number {
 const flow = computeTileFlow(input);
 const gap = Number.isFinite(input.gap) ? Math.max(0, input.gap) : 0;
 const width = Number.isFinite(input.containerWidth) ? Math.max(0, input.containerWidth) : 0;
 if (width <= 0) return input.cellWidth;
 return (width - (flow.columns - 1) * gap) / flow.columns;
}

/**
 * 「横向适合窗口」**能不能做**（人类 2026-09-23 定）。
 *
 * 判据：算出来的格宽不得超过最大档（`MAX_TILE_SIZE`）。
 * 超过时按钮**无效** —— 那时只有「少放几列」才铺得满，而列数是按当前格宽算出来的，
 * 不能为了铺满而凭空改列数（那会让网格瞬间跳一次列）。
 * 所以宁可老实说「这个按钮现在干不了这件事」。
 *
 * 窗口很宽、照片很少时就会碰上（例如 4 张照片摆在 2560 宽的屏上）。
 */
export function canFitRow(input: TileFlowInput): boolean {
 // 容器还没量到（宽 0）：不是「能做」—— 没有宽度事实就不该声称铺得满
 if (!Number.isFinite(input.containerWidth) || input.containerWidth <= 0) return false;
 const fitted = fitTileSizeToRow(input);
 if (!Number.isFinite(fitted) || fitted <= 0) return false;
 return fitted <= MAX_TILE_SIZE;
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
