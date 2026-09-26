/**
 * tiles 网格的**数据源契约**（`components/ui/tiles/` 的一部分）。
 *
 * 网格只有**一个**（`features/photo-grid/PhotoGrid.tsx`），但两侧数据形状不同：
 *
 * | | 导入 | 浏览 |
 * | --- | --- | --- |
 * | 取数 | 整目录一次性拿到 | 按页取（未取回的格子是洞） |
 * | 顺序 | 就是拿到的顺序 | 片内按文件名自然序**置换**过 |
 * | id | 路径 | 资产 id |
 * | 标记 | 没有 | 星标 / 色标 / 旗标 / 锁 |
 *
 * 差异属于**数据层**，不该逼出第二个网格组件（人类 2026-09-19：
 * 「同一个东西两个组件本身就是 bug」）。所以：一份视图 + 一层适配。
 * 适配器住在各自 feature 里，**是数据胶水，不是组件**。
 */

import type { ThumbEntry } from "../thumb-queue.ts";
import type { RowSlice } from "./rows.ts";
import type { TileInfoMode } from "../../../lib/display-prefs.ts";

/** 网格里一张照片（两侧的公共形状）。只放渲染真正要用的字段。 */
export interface GridItem {
  /** 稳定 id：导入 = 路径；浏览 = 资产 id 的字符串 */
  id: string;
  /** 取图用的**绝对路径** */
  path: string;
  /** 完整文件名（`Tile` 自己去后缀显示主名） */
  fileName: string;
  /** 扩展名（小写，不含点）—— 信息条右端的标签 */
  ext?: string | null;
  /** 展示用宽高比（已应用方向、已夹取）；`undefined` = 元数据还没到 */
  aspect?: number;
  /** 展示的就是 RAW → 角标 `RAW` */
  isRaw?: boolean;
  /** 同一张还有 RAW → 角标 `+RAW` */
  hasRaw?: boolean;
  missing?: boolean;
  /** 被排除（只有导入侧有这个概念） */
  excluded?: boolean;
  /** 库内才有的标记；导入侧不传 */
  marks?: {
    rating: number;
    colorLabel: string | null;
    /** 赞/踩（看图态底部状态栏要用；导入侧不传） */
    likeState?: string | null;
    flag: "pick" | "reject" | null;
    locked: boolean;
  };
}

/** 网格状态机 */
export type GridStatus = "idle" | "loading" | "ready" | "error";

export interface TilesSource {
  /** 显示序下一共有几格 */
  count(): number;
  /** 只读显示序身份，允许分页洞参与组选择。 */
  idAt?(index: number): string | null;
  /** 附加区高度由适配层数据决定，像素值由唯一行模型汇总。 */
  extraHeight?(index: number, cellSize: number): number;
  /** 工作流手势适配；默认仍为统一 clickMode。 */
  invertedCtrl?: boolean;
  /** 显示序第 `index` 格；`null` = 这一页还没取回来（渲染成占位块） */
  itemAt(index: number): GridItem | null;
  /** 按 id 取（看图、锚定、键盘导航用） */
  itemById(id: string): GridItem | null;
  /** 保证 `[start, end)` 的数据都在（浏览按页取时实现；导入不实现） */
  ensureRange?(start: number, end: number): void | Promise<void>;
  /** 展示用宽高比（元数据没到时返回占位比例） */
  aspectOf(id: string): number;
  /** 真实宽高（看图缩放边界用）；不知道就是 `null` */
  naturalOf(id: string): { width: number; height: number } | null;
  /** 按需补读真实宽高（网格只为可见的那些读过） */
  ensureNatural(entries: readonly { id: string; path: string }[]): void | Promise<void>;
  /** 按时间分组时的显示序片；`undefined` = 平铺 */
  slices(): readonly RowSlice[] | undefined;
  status(): GridStatus;
  error(): string | null;
  reload(): void | Promise<void>;
  /** **换内容**的复位键（虚拟列表靠它回顶部） */
  scopeKey(): string;
  selection(): { ids: ReadonlySet<string>; anchor: string | null };
  select(id: string, mode: "replace" | "toggle" | "range"): void;
  /** 只挪当前锚点，不改变多选集合（对比视图点某一幅时用）。 */
  setAnchor(id: string): void;
  /**
   * 选中**显示序区间**里的所有照片（日 / 时间片标题上的那颗药丸）。
   *
   * 为什么给区间而不是 id 列表：浏览的时间线**包含还没取回来的页**，
   * 「这一天」不该漏掉它们 —— 而网格手上只有已加载的格子。
   * 区间 → id 的换算由数据源做（它知道置换与分页）。
   *
   * 语义是**整段开关**（全选中 → 全取消；否则 → 全选中），与「点单张照片」不同：
   * 不看修饰键。完整口径见 `lib/selection.ts::toggleGroupSelection`。
   */
  selectGroupRange(start: number, count: number): void;
  clearSelection(): void;
  /** 当前工作区自己的信息显示档位。 */
  infoMode(): TileInfoMode;
  tileStep(): number;
  setTileStep(step: number): void;
  commitTileStep(): void;
  thumb(path: string): ThumbEntry;
  requestThumb(path: string): void;
}
