/**
 * **对比态**的数学（`BROWSE.md` §5.5/§5.7、`specs/M2-W2.md` 2.2–2.3）。
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
 * * **窗口内从左到右**也按选择先后：最早选中的在最左 ——
 *   例：Ctrl 依次点 A B C D E F ⇒ 窗口 `C D E F`，A/B 被挤掉。
 * * **锚点必含**：区间选择时锚点在中间，可能被挤出窗口 —— 那就挤掉窗口里最早的那张，
 *   把锚点放到末尾（它是用户最后一次动作的那张）。
 * * 批量选择（`Ctrl+A`、「日组全选」）没有「先后」可言：插入顺序 = 列表/组内顺序，
 *   于是窗口 = 这批的最后 4 张。行为确定、可解释，只是不是一张张点出来的意图。
 *
 * ## 画幅不一致时怎么办：**虚拟画布**（人类 2026-09-20 改的口径，取代旧的「扣等比例区域」）
 *
 * 旧口径是「以第一幅的画幅比例为准，对后面的图扣等比例区域」。**已废弃**，原因很具体：
 * 截掉的部分在**缩放后会露馅** —— 竖图打头时，横图被裁成竖比例，看着“没问题”，
 * 但鼠标一放大，用户就想看到被裁掉的那部分了（人类原话：「这一缩就露馅了」）。
 *
 * 新口径（人类 2026-09-20 定）：
 *
 * ```text
 * ┌─ 虚拟画布（宽 = 各图最大宽，高 = 各图最大高，单位就是原图像素）─┐
 * │      ┌─────┐                       │
 * │      │ 竖图 │   ← 居中放进画布         │
 * │      └─────┘                       │
 * │  ┌───────────┐   ← 小的图两头都挨不到边 │
 * │  └───────────┘                       │
 * └─────────────────────────────────┘
 * ```
 *
 * * **画布尺寸 = 最大宽 × 最大高**（例：6000×4000 与 3750×5000 ⇒ 画布 6000×5000）；
 * * 每张图**按自己的原图尺寸居中放进画布**（不裁、不缩、不拉伸）；
 * * **统一倍率**：一个倍率对所有图成立（比例不同也不存在“各自按比例”的问题），
 *   1 = 100% = 画布像素 1:1；
 * * 移动按**画布坐标**：顶多有些图被移出窗口，但总有图在窗口里（人类：可以接受）。
 *
 * 倍率不再是“相对画框的倍数”而是**绝对倍率**（画布像素 → CSS 像素），
 * 所以「 100%」就是 1:1、「适合窗口」就是把某张图 contain 进栏区。
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
 * 看细节不够；2×2 每张能大一倍。每种布局里**每格尺寸一致** ——
 * 栏区一样大，「适合窗口」的倍率才有确定含义。
 */
export function compareLayout(count: number): { columns: number; rows: number } {
  const safe = Number.isFinite(count) ? Math.floor(count) : 0;
  const n = Math.max(0, Math.min(COMPARE_MAX, safe));
  if (n >= 4) return { columns: 2, rows: 2 };
  if (n === 3) return { columns: 3, rows: 1 };
  return { columns: Math.max(1, n), rows: 1 };
}
/**
 * 一张图在**虚拟画布**里的落点。
 *
 * **对照片类型泛型**：`compareCanvas` 会把传进来的对象**原样**放进落点里，
 * 所以调用方拿到的 `placement.photo` 仍是它自己的类型（`fileName` / `path` 不丢）。
 */
export interface ComparePlacement<T extends ComparablePhoto = ComparablePhoto> {
  photo: T;
  /** 原图尺寸（画布坐标；尺寸未知时是 0） */
  natural: Size;
  /**
   * 左上角在画布里的位置。
   *
   * **居中**：小图在 X / Y 两个方向的两头都挨不到画布边（人类 2026-09-20：
   * 「因为图小，所以 x 轴和 y 轴各自的两头都挨不到边」）—— 居中也让所有图的
   * **中心**对齐，那正是对比时要看的参照点。
   */
  offset: { x: number; y: number };
}

/** 一整组对比的虚拟画布（见文件头）。 */
export interface CompareCanvas<T extends ComparablePhoto = ComparablePhoto> {
  /** 画布尺寸 = 各图**最大宽 × 最大高**（全部尺寸未知时是 0×0） */
  size: Size;
  placements: ComparePlacement<T>[];
}

/**
 * 算出这一组对比的虚拟画布与每张图的落点。
 *
 * `photos` 传 `compareIds(...)` 映射出来的结果（已按显示顺序、已截到上限）。
 * 尺寸未知（`natural` 缺失）的图照样占一个落点（`natural` 是 0），
 * 视图据此显示「还没读到这张的尺寸」，而不是编一个比例把画面拉歪。
 */
/**
 * 一张图的**有效**原图尺寸（两轴都必须 > 0）。
 *
 * 脏数据（`0` / 负数 / `NaN` / 缺 `natural`）一慨当「尺寸未知」—— 画布与落点都
 * 不该拿一个半瞎的尺寸去摆版面（否则会出现「宽 0、高 200」这种画布）。
 */
function validSize(photo: ComparablePhoto): Size {
  const natural = photo.natural;
  if (
    natural === undefined ||
    !(natural.width > 0) ||
    !(natural.height > 0) ||
    !Number.isFinite(natural.width) ||
    !Number.isFinite(natural.height)
  ) {
    return { width: 0, height: 0 };
  }
  return { width: natural.width, height: natural.height };
}

export function compareCanvas<T extends ComparablePhoto>(
  photos: readonly T[],
): CompareCanvas<T> {
  let width = 0;
  let height = 0;
  for (const photo of photos) {
    const natural = validSize(photo);
    if (natural.width > width) width = natural.width;
    if (natural.height > height) height = natural.height;
  }

  const placements = photos.map((photo): ComparePlacement<T> => {
    const natural = validSize(photo);
    return {
      photo,
      natural,
      offset: { x: (width - natural.width) / 2, y: (height - natural.height) / 2 },
    };
  });

  return { size: { width, height }, placements };
}

/**
 * 画布比例（宽 / 高）—— 视图用它把画布**铺进栏区**（`fitAspectWithin` 的输入）。
 *
 * 画布还没量出来（`0×0`）时给 1：调用方本来就该走空态，别在比例上除零。
 */
export function canvasAspect(canvas: CompareCanvas): number {
  const { width, height } = canvas.size;
  return width > 0 && height > 0 ? width / height : 1;
}

/**
 * **像素数最小**的那张 —— 进入对比 / 改变对比集合时的「适合窗口」以它为准
 * （人类 2026-09-20：「以尺寸最小的图片（不需要计算很精确，按像素数来就行）
 * 来算适合的放大倍率，类似代替用户在最小的图片上双击到适合画面尺寸」）。
 *
 * 尺寸未知的**不参与**（拿不到像素数就不能比较）；全都未知时返回 `null`。
 * 并列时取先遇到的那张（列表顺序 = 显示顺序，可复现）。
 */
export function smallestByPixels<T extends ComparablePhoto>(
  photos: readonly T[],
): T | null {
  let best: T | null = null;
  let bestPixels = Number.POSITIVE_INFINITY;
  for (const photo of photos) {
    const natural = validSize(photo);
    if (natural.width <= 0) continue;
    const pixels = natural.width * natural.height;
    if (pixels < bestPixels) {
      best = photo;
      bestPixels = pixels;
    }
  }
  return best;
}
