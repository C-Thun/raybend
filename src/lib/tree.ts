/**
 * 目录树的行模型与路径身份键。
 *
 * 这个文件不渲染任何东西 —— 它解决两个**容易实现错**的问题：
 *
 * 1. **可见行 = 展开状态的结果**（DESIGN.md §12.4.1）
 *    树里「某一层没展开时，那些行根本不存在于视图中」。
 *    先把可见行摊平成一个数组，渲染层就是纯映射，不需要递归组件里做条件判断，
 *    也天然满足「选中不强制展开」——因为摊平只看展开集合，**不看选中集合**。
 *
 * 2. **路径身份键**（AGENTS.md §7.3）
 *    跨面板的选中同步（`最近` ↔ `来源树`）要判断「这是不是同一个目录」。
 *    直接比字符串会在三处翻车：
 *      - Windows 大小写不敏感：`D:\Photos` 与 `d:\photos` 是同一个目录
 *      - 分隔符两种写法：`D:\A\B` 与 `D:/A/B` 指向同一处
 *      - macOS 用 NFD、Windows/Linux 用 NFC：`café` 的两张码点序列长度都不同
 *    所以比较一律走 `pathKey()`，**禁止**直接 `===` 比路径。
 */

/** 只要带上 `children` 就能被摊平 —— 用结构类型，不绑死具体树节点形状 */
export interface TreeLike<T> {
  children?: readonly T[];
}

export interface FlatRow<T> {
  node: T;
  /** 从 0 开始（顶层 = 0） */
  depth: number;
  hasChildren: boolean;
}

export interface FlattenOptions<T> {
  /** 取子节点；默认读 `node.children`。为了兼容 Ark UI 的 collection 等形状 */
  childrenOf?: (node: T) => readonly T[] | undefined;
  /**
   * 深度上限。防的是**环形引用**（`a.children` 里又指回 `a`）——
   * 目录树理论上不会环，但数据来自扫描器，防御性留一道闸。
   * 超出的层级不再展开（当作叶子）。
   */
  maxDepth?: number;
}

const DEFAULT_MAX_DEPTH = 64;

/**
 * 按展开状态深度优先摊平，**折叠节点的子树整棵不出现**。
 *
 * `isExpanded` 只看用户的展开操作 —— 调用方**不要**把「选中」并进去，
 * 那会变成「为显示选中而强制展开」（DESIGN.md §12.4.1 明确禁止）。
 */
export function flattenVisible<T>(
  nodes: readonly T[],
  isExpanded: (node: T) => boolean,
  options: FlattenOptions<T> = {},
): FlatRow<T>[] {
  const childrenOf =
    options.childrenOf ??
    ((node: T) => (node as TreeLike<T>).children as readonly T[] | undefined);
  const maxDepth =
    Number.isFinite(options.maxDepth) && (options.maxDepth ?? 0) >= 0
      ? Math.floor(options.maxDepth as number)
      : DEFAULT_MAX_DEPTH;

  const rows: FlatRow<T>[] = [];

  // 显式栈而不是递归：树的深度由用户目录决定，递归写法的栈深度不可控
  const walk = (list: readonly T[], depth: number): void => {
    for (const node of list) {
      const children = childrenOf(node);
      const hasChildren = Array.isArray(children) && children.length > 0;
      rows.push({ node, depth, hasChildren });
      if (hasChildren && isExpanded(node) && depth < maxDepth) {
        walk(children as readonly T[], depth + 1);
      }
    }
  };

  walk(nodes, 0);
  return rows;
}

/**
 * 行的起始缩进（px）。
 *
 * 缩进是**间距类**尺寸（DESIGN.md §8.1：密度只影响间距），所以按当前档位传入的
 * 单位值计算；非法输入不产出 NaN（会一路传到布局里炸掉整列宽）。
 */
export function indentPx(depth: number, unitPx: number): number {
  const d = Number.isFinite(depth) ? Math.max(0, Math.trunc(depth)) : 0;
  const unit = Number.isFinite(unitPx) ? Math.max(0, unitPx) : 0;
  return d * unit;
}

export interface PathKeyOptions {
  /**
   * 是否折叠大小写。默认 `true`。
   *
   * 真相是**平台相关**的（AGENTS.md §7.3）：Windows 不敏感、macOS 不敏感、Linux 敏感。
   * 默认取 `true` 是因为本项目 Windows 优先；Linux 侧要由调用方显式传 `false`。
   */
  caseFold?: boolean;
}

/**
 * 路径身份键：比较「这两个路径是不是同一个目录」时**只用它**。
 *
 * 处理链：NFC 规范化 → 分隔符统一成 `/` → 去尾部分隔符 → 合并重复分隔符 → （可选）折叠大小写。
 *
 * 注意 `D:\` 与 `D:/` 都会得到 `d:`（驱动器根节点自身），这是有意的：
 * 树里的驱动器节点与路径字符串必须能对上。
 */
export function pathKey(
  input: string,
  options: PathKeyOptions = {},
): string {
  // 路径可能来自 Rust 侧反序列化的 JSON —— 类型声明拦不住运行时的 null/undefined
  if (!input) return "";

  const caseFold = options.caseFold ?? true;
  let key = input.normalize("NFC").replace(/\\/g, "/").trim();

  // 合并重复分隔符（UNC 的 `//server/share` 前导双斜杠要保留）
  const isUnc = key.startsWith("//");
  const isAbsolute = key.startsWith("/");
  let prefix = "";
  let body = key;
  if (isUnc) {
    prefix = "//";
    body = key.replace(/^\/+/, "");
  } else if (isAbsolute) {
    prefix = "/";
    body = key.replace(/^\/+/, "");
  }
  // 整串都是斜杠时（`/` / `//` / `///`）body 为空 —— 那就只剩下前缀本身
  key = body ? prefix + body.replace(/\/{2,}/g, "/") : prefix;

  // 去尾部分隔符；但不要把一个根路径（`/`、`//`、`D:/`）塌成空串：
  // 剥完变成空串就说明「去掉的全部都是根」，那就把根留着。
  const stripped = key.replace(/\/+$/, "");
  if (stripped) key = stripped;

  return caseFold ? key.toLowerCase() : key;
}

/** 两条路径是否指向同一处（跨面板选中同步用） */
export function samePath(
  a: string | null | undefined,
  b: string | null | undefined,
  options: PathKeyOptions = {},
): boolean {
  if (a == null || b == null) return a == null && b == null;
  return pathKey(a, options) === pathKey(b, options);
}
