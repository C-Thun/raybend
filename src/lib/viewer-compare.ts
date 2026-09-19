/**
 * **对比态**的数学（`BROWSE.md` §5.5/§5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * ## 状态从哪来：不需要「进入对比」这个动作
 *
 * 对比态不是开关，而是**选择状态的直接映射**（人类 2026-09-18 的口述：
 * 「胶片带中多选 → **自然进入**多图对比；反选图片 → **自然从对比中移除**」）：
 *
 * ```text
 * 看图模式 + 选中 ≥ 2 张  ⇒ 对比态
 * 反选到只剩 1 张         ⇒ 自动退出对比（回到单张看图）
 * ```
 *
 * 所以这里没有任何 `enterCompare()` / `exitCompare()` —— 只有 `compareIds()`：
 * 「现在该对比哪几张」。
 *
 * ## 超过 4 张：只对比**最近选中的 4 张**（人类 2026-09-19 定）
 *
 * 「任何状态下，选中超过 4 张时，仅对比最近选中的 4 张；不支持超过 4 张的照片对比。」
 *
 * * **最近**从哪看：`SelectionState.ids` 是个 `Set`，而 `toggle` / `range` / `extendSelection`
 *   全都是「`new Set(旧)` 再 `add`」—— 新增的落在**末尾**，所以 **Set 的插入顺序就是选择先后**
 *   （`pruneSelection` 也保序）。不新增字段、不动选择内核。
 * * **窗口内从左到右**也按选择先后：最早选中的在最左，它就是画幅基准 ——
 *   例：Ctrl 依次点 A B C D E F ⇒ 窗口 `C D E F`，基准 C，A/B 被挤掉。
 * * **锚点必含**：区间选择时锚点在中间，可能被挤出窗口 —— 那就挤掉窗口里最早的那张，
 *   把锚点放到末尾（它是用户最后一次动作的那张）。
 * * 批量选择（`Ctrl+A`、「日组全选」）没有「先后」可言：插入顺序 = 列表/组内顺序，
 *   于是窗口 = 这批的最后 4 张。行为确定、可解释，只是不是一张张点出来的意图。
 *
 * ## 画幅不一致时怎么办：以**第一幅**为准扣等比例区域
 *
 * 人类原话：「**画幅比例不一致时以第一幅图的画幅比例为准**，对后面的图**扣等比例区域**
 * 做位移同步（例：第一张 4:3、后面是 3:2 → 只能对比『3:2 区域中间抽出来的 4:3』那部分）」。
 * 这里的「第一幅」= 对比窗口里**最早选中**的那张（见上）。
 *
 * * **缩放同步**：同一个倍率对所有画幅都成立（都是同一个画框）；
 * * **位移按百分比同步**：所有画幅的扣取区都映射到**同一个画框**，所以「同一个 CSS 像素
 *   位移」对每一幅而言就是「同一个画框百分比」—— 不需要再单独换算百分比。
 *
 * 这两条正是 `plans/M2-W2.md` 2.3 的两条硬要求。
 *
 * ## 两层盒子（2026-09-20 人类纠正后的口径，别再混）
 *
 * ```text
 * ┌─ 窗口（= 分栏分到的栏区）─────────┐   ← 可见/裁剪的边界（`overflow: hidden`），有底色与描边
 * │        ┌─ 画框（等比例，居中）─┐   │   ← 基准比例的盒子：所有扣取区映射到这里
 * │        │      图片内容          │   │   ← 放大时画框变大，由窗口裁掉多余部分
 * │        └──────────────────────┘   │
 * └──────────────────────────────────┘
 * ```
 *
 * 人类原话：「每个分栏方法内分到的栏区都**像个窗口一样**……图片比例不能变化和拉伸，
 * 但在这个窗口内可以任意缩放，放大时图片变大，在各自整个窗口内都可以看到图」。
 * 所以：**画框不是裁剪边界**，它只是「扣取区在适配时的大小与位置」；放大后画框
 * 可以大于窗口（这时才是窗口在裁）。适配时画框在窗口里居中，窗口剩下的地方是底色。
 */

/**
 * 对比数学只关心两件事：**这张是谁**（`id`）与**它的原始尺寸**（`natural`）。
 *
 * 刻意**不** import UI 层里那个照片类型 —— 那是 `components/` 的定义，
 * 而 `lib/` 只能向下依赖（`ARCHITECTURE.md` §1，`pnpm lint:arch` 会拦）。
 * 声明「我需要的形状」而不是「我要谁的类型」，两侧（import / browse）就都能用它：
 * 真实照片对象在结构上满足它，直接传进来即可。
 */
export interface ComparablePhoto {
  id: string;
  /**
   * 原图尺寸。口径与 `ViewerPhoto.natural` **完全一致**：可选，但一旦有就非 null
   * （还没读到元数据时就是「没有」）。尺寸未知时对比退回「整张/零」，不编比例。
   */
  natural?: { width: number; height: number };
}

/** 同时对比的**上限**（人类 2026-09-18：对比 2–4 张） */
export const COMPARE_MAX = 4;

export interface Size {
  width: number;
  height: number;
}

export interface CropRect extends Size {
  x: number;
  y: number;
}

/**
 * 把固定画幅比例完整放进一个格子里（contain），返回不变形的实际画框尺寸。
 *
 * Compare 不能只写 `height: 100%; max-width: 100%`：窗口变窄后宽度会被夹，若高度仍是
 * 100%，空盒子自己的 `aspect-ratio` 在不同布局引擎里会被约束打破，图片就跟着拉伸。
 */
export function fitAspectWithin(bounds: Size, aspect: number): Size {
  if (
    !(bounds.width > 0) ||
    !(bounds.height > 0) ||
    !(aspect > 0) ||
    !Number.isFinite(aspect)
  ) {
    return { width: 0, height: 0 };
  }
  if (bounds.width / bounds.height > aspect) {
    return { width: bounds.height * aspect, height: bounds.height };
  }
  return { width: bounds.width, height: bounds.width / aspect };
}

/**
 * 算出「现在该对比哪几张」——**最近选中的 `COMPARE_MAX` 张**，按选择先后排。
 *
 * * `selected` 传 `store.selection().ids`（`Set` 的插入顺序 = 选择先后）；
 * * `orderedIds` 是当前列表的显示顺序，只用来**判定 id 还在不在当前列表里**
 *   （换目录后可能残留旧选择）；
 * * `anchor` 是「最后一次点中的那张」（`store.selection().anchor`）：它在窗口外时
 *   挤掉窗口里最早的那张，自己放到末尾 —— 区间选择会让锚点落在中间。
 *
 * 少于 2 张（或当前列表里不足 2 张）返回空数组 —— 调用方据此回到单张看图。
 */
export function compareIds(
  selected: ReadonlySet<string>,
  orderedIds: readonly string[],
  anchor: string | null = null,
): string[] {
  if (selected.size < 2) return [];
  const present = new Set(orderedIds);
  // 选择先后：Set 的迭代顺序就是插入顺序（见文件头的说明）
  const recent: string[] = [];
  for (const id of selected) {
    if (present.has(id)) recent.push(id);
  }
  if (recent.length < 2) return [];

  // 窗口 = 最近选中的那几张（不够上限时就是全部）
  let window = recent.slice(-COMPARE_MAX);
  if (anchor !== null && recent.includes(anchor) && !window.includes(anchor)) {
    window = [...window.slice(1), anchor];
  }
  return window;
}

/**
 * 布局：几张画幅怎么摆（人类 2026-09-19 定：2 张一排 / 3 张一排三个 / 4 张 2×2）。
 *
 * 为啥不是「永远一排」：1600 宽的窗口减去两侧栏后，4 张一排每张只剩 ≈240px，
 * 看细节不够；2×2 每张能大一倍。每种布局里**每格尺寸一致**，
 * 所以「所有画幅扣成同一个比例」仍然成立（格子大小不影响比例与百分比同步）。
 */
export function compareLayout(count: number): { columns: number; rows: number } {
  const safe = Number.isFinite(count) ? Math.floor(count) : 0;
  const n = Math.max(0, Math.min(COMPARE_MAX, safe));
  if (n >= 4) return { columns: 2, rows: 2 };
  if (n === 3) return { columns: 3, rows: 1 };
  return { columns: Math.max(1, n), rows: 1 };
}

/**
 * 基准比例 = **第一幅**的宽高比。
 *
 * 尺寸未知（`natural` 缺失）时返回 `null` —— 调用方退回「各自用自己的比例」，
 * 而不是编一个比例出来（编错了整屏都会歪）。
 */
export function baselineAspect(photos: readonly ComparablePhoto[]): number | null {
  const first = photos[0];
  const size = first?.natural;
  if (!size || size.width <= 0 || size.height <= 0) return null;
  return size.width / size.height;
}

/**
 * 在**一张图自己的像素**里，居中扣出比例为 `aspect` 的那块。
 *
 * 三种情况都要对（都有单测）：
 * * 比基准**宽** → 扣两侧（保留高度）；
 * * 比基准**窄/高** → 扣上下（保留宽度）；
 * * 比例已经一致 → 整张，一点不扣（不许出现 ±0.5px 的滑动）；
 * * 尺寸未知或非法 → 原样返回（不猜）。
 */
export function cropToAspect(size: Size, aspect: number): CropRect {
  const { width, height } = size;
  if (!(width > 0) || !(height > 0) || !(aspect > 0) || !Number.isFinite(aspect)) {
    return { x: 0, y: 0, width: Math.max(0, width), height: Math.max(0, height) };
  }
  const current = width / height;
  // 比例一致（含浮点误差）：整张，别为了 1e-9 的差别去裁一像素
  if (Math.abs(current - aspect) <= 1e-6) {
    return { x: 0, y: 0, width, height };
  }
  if (current > aspect) {
    // 太宽：扣两侧
    const cropped = height * aspect;
    return { x: (width - cropped) / 2, y: 0, width: cropped, height };
  }
  // 太窄/太高：扣上下
  const cropped = width / aspect;
  return { x: 0, y: (height - cropped) / 2, width, height: cropped };
}

/**
 * 一帧：某张照片 + 它在自己像素里要显示的那块 + 它在**画框**里的摆法。
 *
 * **对照片类型泛型**：`compareGeometry` 会把传进来的对象**原样**放进帧里，
 * 所以调用方拿到的 `frame.photo` 仍是它自己的类型（`fileName` / `path` 这些字段不丢）。
 */
export interface CompareFrame<T extends ComparablePhoto = ComparablePhoto> {
  photo: T;
  /** 扣出来的那块（比例 = 基准比例；尺寸未知时是 0） */
  crop: CropRect;
  /** 整幅图在画框里的显示尺寸（CSS px，**适配时**，即相对倍数 = 1） */
  image: Size;
  /** 整幅图左上角相对**画框**左上角的偏移（CSS px，适配时；≤ 0） */
  imageOffset: { x: number; y: number };
}

/**
 * 一整组对比的几何：画框 + 每帧的扣取区与图片摆放。
 *
 * `frame` 是**等比例画框**在适配时的大小（居中放在窗口里，见文件头「两层盒子」）；
 * 放大/平移是视图在它之上叠的变换（`translate(pan) scale(rel)`），几何本身不变。
 */
export interface CompareGeometry<T extends ComparablePhoto = ComparablePhoto> {
  /** 等比例画框——所有画幅共用（`rel = 1` 时的大小） */
  frame: Size;
  /**
   * 基准图 **1:1**（原图像素 : CSS 像素 = 1）对应的**相对倍数**；尺寸未知 = `null`。
   *
   * 为什么用相对倍数而不是绝对倍率：对比里各图像素尺寸本来就不同
   * （4000×3000 与 6000×4000），「所有画幅同一个倍率」只能是相对画框的倍数。
   * 需要「100%」时（双击、读数）换算到**基准那幅**：`rel = oneToOneRel`。
   */
  oneToOneRel: number | null;
  frames: CompareFrame<T>[];
}

/**
 * 算出窗口（栏区）当前该显示的一组几何（**以第一幅的比例为准**）。
 *
 * `photos` 传 `compareIds(...)` 映射出来的结果（已经按显示顺序、已截到上限）；
 * `pane` 是**分栏分到的栏区**（窗口）的 CSS 尺寸 —— 不是画框、不是视口。
 *
 * 尺寸未知（`natural` 缺失）时：画框算不出来（`frame = 0×0`），该帧的 `crop` 与 `image`
 * 也是 0 —— 视图据此显示「还没读到这张的尺寸」，而不是编一个比例把画面拉歪。
 */
export function compareGeometry<T extends ComparablePhoto>(
  photos: readonly T[],
  pane: Size,
): CompareGeometry<T> {
  const aspect = baselineAspect(photos);
  const frame = aspect === null ? { width: 0, height: 0 } : fitAspectWithin(pane, aspect);

  const frames = photos.map((photo): CompareFrame<T> => {
    const natural = photo.natural;
    const known =
      natural !== undefined && natural.width > 0 && natural.height > 0;
    const crop =
      aspect === null || !known
        ? { x: 0, y: 0, width: 0, height: 0 }
        : cropToAspect(natural, aspect);
    // 扣取区正好铺满画框：画框宽 = 扣取宽 × base
    const base = crop.width > 0 ? frame.width / crop.width : 0;
    return {
      photo,
      crop,
      image: {
        width: (natural?.width ?? 0) * base,
        height: (natural?.height ?? 0) * base,
      },
      // 扣取区居中在整幅图里，所以两边的偏移都是负的（图片比画框大）
      imageOffset: { x: -crop.x * base, y: -crop.y * base },
    };
  });

  const baseline = frames[0];
  const oneToOneRel =
    baseline !== undefined && baseline.crop.width > 0 && frame.width > 0
      ? baseline.crop.width / frame.width
      : null;

  return { frame, oneToOneRel, frames };
}
