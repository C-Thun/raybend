/**
 * 焦点急救 —— 把焦点**主动**从当前元素上摘掉。
 *
 * ## 为什么需要它（2026-09-16 人类实测反馈）
 *
 * 现象：点弹窗外面那片空白（遮罩）关掉弹窗之后，**触发弹窗的那个按钮上留了一圈焦点圈**。
 * 看起来像样式 bug，其实根因是**焦点从没移动过**：
 *
 *   * 点按钮 → 按钮拿到焦点（`activeElement` = 按钮）；
 *   * 点遮罩 → 弹窗关闭，但**没有任何一步把焦点移走**，`activeElement` 还是那个按钮；
 *   * 浏览器把这种「程序性保留的焦点」按键盘焦点处理 → 画出 `:focus-visible` 的圈。
 *
 * 所以这**不是**「鼠标点击也会出圈」的问题，用「输入模态判断」那类办法治不了（试过，不成立）。
 * 人类给的处方很准：**关闭之前先把焦点清掉**。
 *
 * ## 用法纪律（顺序要紧）
 *
 * **先清焦点，再关闭** —— 反了会让圈先画出来再消失，闪一下更难看。
 *
 * 两处入口：
 *   1. {@link installEscapeBlur} 装在启动处：**任何时候按 Esc 都先清焦点**（捕获期，抢在
 *      组件自己的 Esc 处理之前）—— 一处收口，不用满世界打补丁；
 *   2. 弹窗封装层（`components/ui/Dialog.tsx`）在「点遮罩」时先调 {@link blurActive}。
 */

/**
 * 把焦点从当前元素上摘掉（清完焦点落在 `<body>` 上，不会画圈）。
 *
 * 对以下情况是安全的：
 *   * 没有任何元素持有焦点（`activeElement` 为 `null`）；
 *   * 焦点已经在 `<body>` 上（无事可做）；
 *   * 元素已经被卸载 / `blur()` 抛错 —— 捕获后忽略，绝不让「清焦点」本身把流程搞崩。
 */
export function blurActive(doc: Document = document): void {
  const active = doc.activeElement as HTMLElement | null;
  if (active === null || active === doc.body) return;
  try {
    active.blur();
  } catch {
    // 元素已卸载之类：焦点本来就不在了，忽略
  }
}

/**
 * 装一个全局的「按 Esc 先清焦点」。
 *
 * * 用**捕获期**（第三个参数 `true`）：抢在弹窗/菜单自己的 Esc 处理之前清掉，
 *   这样它们随后关闭时就不会留下圈；
 * * 对 Esc 本身**不拦截**（不 `preventDefault`、不 `stopPropagation`）——
 *   该关的弹窗照常关，这里只负责清焦点。
 *
 * @returns 卸载函数（测试与热重载用）
 */
export function installEscapeBlur(target: Window = window): () => void {
  const onKeyDown = (event: KeyboardEvent): void => {
    if (event.key === "Escape") blurActive(target.document);
  };
  target.addEventListener("keydown", onKeyDown, true);
  return () => target.removeEventListener("keydown", onKeyDown, true);
}
