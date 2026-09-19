/**
 * 路径拼接的小工具（**纯函数**，两侧共用一份）。
 *
 * 为什么单独成文件：库里出现的路径有两种写法 —— 库内相对路径统一用 `/`
 * （`photos/2026-08-15/MY0001.JPG`），而库根本身是**系统路径**（Windows 上是
 * `C:\photos\lib`、Linux 上是 `/mnt/photos/lib`）。把相对路径接到根上时，
 * 分隔符要**跟着根走**，否则 Windows 上会拼出 `C:\photos\lib/photos/...` 这种混合写法 ——
 * 大多数 Windows API 能忍，但拿去比对字符串或显示时就会漏。
 *
 * 这个函数原先在浏览网格里、工作区里各写了一遍（`absPath`），2026-09-19 收成一处。
 */

/** 分隔符：反斜杠结尾（Windows 风格）还是斜杠结尾 */
function separatorOf(root: string): string {
  return root.includes("\\") && !root.includes("/") ? "\\" : "/";
}

/**
 * 把库内相对路径接到库根上。
 *
 * 边界：
 *   * 根本身以分隔符结尾（`C:\lib\`、`/lib/`）→ 不重复加；
 *   * 相对路径以分隔符开头（`/photos/x`）→ 去掉再拼（避免 `lib//photos`）；
 *   * 空根 → 返回相对路径原样（调用方负责判断根有没有拿到）。
 */
export function joinPath(root: string, relative: string): string {
  if (root === "") return relative;
  const sep = separatorOf(root);
  const left = root.endsWith("/") || root.endsWith("\\") ? root.slice(0, -1) : root;
  /*
   * 相对路径里的分隔符**统一成与根同款**。
   *
   * 库内相对路径一律是 `/`（schema 的约定），而库根在 Windows 上是 `C:\...` ——
   * 直接拼会得到 `C:\lib/photos/a.jpg` 这种混合写法。Windows API 大多能忍，
   * 但拿去显示或做字符串比对（缓存键、去重）就会与「同一个文件的另一种写法」不相等。
   */
  const right = relative.replace(/^[\\/]+/, "").replace(/[\\/]+/g, sep);
  return `${left}${sep}${right}`;
}
