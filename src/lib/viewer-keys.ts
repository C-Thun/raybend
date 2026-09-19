/**
 * 浏览模式的**键盘 → 意图**映射（`BROWSE.md` §5.2/§5.6、`plans/M2-W2-tail.md` 5.2）。
 *
 * 为什么单独一个纯函数：键盘表最容易「加一条忘一条」或者把修饰键判错，
 * 而它的每一条分支都要跨好几个模块（store / viewer / 弹窗）——
 * 做成「事件 → 意图」的纯映射之后，**表本身**可以逐条测，视图那边只剩一个 switch。
 *
 * ## 键位表（人类口述 + Lightroom 习惯）
 *
 * | 键 | 意图 |
 * | --- | --- |
 * | `←` / `→` | 看图时切上一张 / 下一张；网格里**移动「当前那张」** |
 * | `0`–`5` | 打 0–5 星（`0` = 清零） |
 * | `P` / `X` | 旗标：留下 / 弃掉 |
 * | `U` | 取消旗标（**不是**清掉星级/色标 —— 见下面的说明） |
 * | `Enter` | 网格里进看图（看图里的 Enter 由看图件自己接） |
 * | `Esc` | 看图时退出；网格里取消所有选中 |
 * | `Delete` | 删除选中的照片（走回收站、要确认） |
 *
 * ## 两处要说明的取舍
 *
 * * **`U` 只清旗标**：人类的口述是「取消标记」，而旗标是「临时工作集」里最常反悔的那一项
 *   （`BROWSE.md` §3.2：旗标只在内存、关软件即清空）。星级与色标是持久元数据，
 *   一次误按就抹掉一批太狠 —— 它们各自有「再点一次同一个值 = 清零」的入口。
 * * **`Delete` 不给 `Shift` 快通道**（人类 2026-09-19 的批注）：删除支持多选批量，
 *   而 `easy destroy` 那套是给「不做批量界面、又要连续快速删单张」的场景准备的。
 */

/** 键盘意图（`null` = 这个键不归浏览模式管） */
export type BrowseKeyIntent =
  | { kind: "viewer-prev" }
  | { kind: "viewer-next" }
  | { kind: "move"; delta: -1 | 1 }
  | { kind: "rating"; value: number }
  | { kind: "flag"; value: "pick" | "reject" | null }
  | { kind: "open-viewer" }
  | { kind: "close-viewer" }
  | { kind: "clear-selection" }
  | { kind: "select-all" }
  | { kind: "delete" }
  | null;

export interface KeyLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

export interface KeyContext {
  /** 正在看图（单张或对比） */
  viewing: boolean;
  /** 当前有选中（打标 / 删除要先有东西可操作） */
  hasSelection: boolean;
}

/**
 * 事件 + 上下文 → 意图。
 *
 * 修饰键的纪律：**带 `Ctrl`/`Cmd`/`Alt` 的组合一律不管**（那是快捷键体系的地方，
 * 而且 `Ctrl+0`…`Ctrl+5` 这种将来可能要留给别的功能）。
 */
export function browseKeyIntent(event: KeyLike, context: KeyContext): BrowseKeyIntent {
  /*
   * `Ctrl/Cmd + A` = **全选**（人类 2026-09-20：「tiles 里要支持 ctrl+a 全选，
   * 即使未显示的部分也要设置选中状态」）。
   *
   * 这是**唯一**允许带修饰键的一条：下面的纪律（Ctrl/Cmd/Alt 组合一律不管）是为了
   * 把 `Ctrl+0…5` 这类留给将来的快捷键体系，而 `Ctrl+A` 是全平台通用的「全选」，
   * 不接反而会被浏览器抢去做**文本选择**（页面会蓝一片）。
   */
  if (
    (event.ctrlKey === true || event.metaKey === true) &&
    (event.key === "a" || event.key === "A")
  ) {
    return { kind: "select-all" };
  }
  if (event.ctrlKey === true || event.metaKey === true || event.altKey === true) return null;

  switch (event.key) {
    case "ArrowLeft":
      return context.viewing ? { kind: "viewer-prev" } : { kind: "move", delta: -1 };
    case "ArrowRight":
      return context.viewing ? { kind: "viewer-next" } : { kind: "move", delta: 1 };
    case "Enter":
      // 看图里的 Enter（退出）由看图件自己接；这里只管「网格里进看图」
      return context.viewing ? null : { kind: "open-viewer" };
    case "Escape":
      return context.viewing ? { kind: "close-viewer" } : { kind: "clear-selection" };
    case "Delete":
      return context.hasSelection ? { kind: "delete" } : null;
    case "Backspace":
      // macOS 上 Delete 键就是 Backspace（Windows 上是 Delete，两条都认）
      return context.hasSelection ? { kind: "delete" } : null;
    case "p":
    case "P":
      return { kind: "flag", value: "pick" };
    case "x":
    case "X":
      return { kind: "flag", value: "reject" };
    case "u":
    case "U":
      return { kind: "flag", value: null };
    default:
      break;
  }

  // 数字键：0–5 打星（别的数字不管）
  if (/^[0-5]$/.test(event.key)) {
    return { kind: "rating", value: Number(event.key) };
  }
  return null;
}

/**
 * 这个事件**该不该被浏览模式接**。
 *
 * 三条否掉的：正在输入框里打字（不然 `p` 会被当成旗标、数字会被当成打星）、
 * 已经有模态开着（弹窗里的键归弹窗）、事件已被别人处理过。
 */
export function shouldHandleKey(target: EventTarget | null, hasModal: boolean): boolean {
  if (hasModal) return false;
  const element = target as HTMLElement | null;
  if (element === null) return true;
  const tag = element.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return false;
  if (element.isContentEditable) return false;
  return true;
}
