/**
 * 剪贴板（`easy copy` 范式的底层，DESIGN.md §12.1）。
 *
 * 为什么不用 Ark UI 的 `Clipboard`：那个组件需要自己渲染 `Clipboard.Trigger`，
 * 而我们要的是「整组信息本身可点」—— 也就是把触发器**当成内容**用，
 * 与 `asChild` 的用法拧着来，不如直接调 API 干净。
 *
 * 两级策略：
 *   ① `navigator.clipboard.writeText`（现代、异步、需要安全上下文）
 *   ② 隐藏 `textarea` + `document.execCommand("copy")`（旧路径兜底）
 *
 * 兜底不是洁癖：Tauri 的 product 页面在部分平台上**不是**安全上下文，
 * 那时 `navigator.clipboard` 会是 `undefined` —— 而「复制 EXIF」是天天要用的功能。
 *
 * 依赖以参数注入，这样纯逻辑部分可以被单元测试覆盖（不需要真的起一个 DOM）。
 */

export interface ClipboardDeps {
  /** 主动写入通道（默认取 `navigator.clipboard`） */
  writeText?: (text: string) => Promise<void>;
  /** 兜底通道（默认用隐藏 textarea + execCommand） */
  fallback?: (text: string) => boolean;
}

function defaultWriteText(): ((text: string) => Promise<void>) | undefined {
  // 正常在 WebView 里跑；单元测试在 Node 里跑时这里拿不到 navigator，直接返回 undefined
  const clipboard = globalThis.navigator?.clipboard;
  if (!clipboard?.writeText) return undefined;
  // 丢进 `navigator.clipboard` 的调用会以 navigator 为 this —— 直接解构会丢 this
  return (text: string) => clipboard.writeText(text);
}

function defaultFallback(text: string): boolean {
  const doc = globalThis.document;
  if (!doc) return false;
  const area = doc.createElement("textarea");
  area.value = text;
  // 放到视口外，避免滚动跳动；同时保持可选中（某些浏览器要求元素可见才能复制）
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.top = "-1000px";
  area.style.opacity = "0";
  doc.body.append(area);
  try {
    area.select();
    // execCommand 已被标记废弃，但它是**唯一**在没有安全上下文时的复制通道；
    // 现代浏览器走上面的 navigator.clipboard，这里只是兵底。
    return doc.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}

/**
 * 复制文本。返回是否成功。
 *
 * **不抛异常** —— 调用方拿到的就是「成功 / 没成功」，因为界面上的反馈只有两种：
 * 显示「已复制」，或者什么都不显示（不骗用户）。
 */
export async function copyText(
  text: string,
  deps: ClipboardDeps = {},
): Promise<boolean> {
  // 空内容不值得写剪贴板：那会把用户原有的剪贴板内容清掉
  if (!text) return false;

  const writeText = deps.writeText ?? defaultWriteText();
  if (writeText) {
    try {
      await writeText(text);
      return true;
    } catch {
      // 落到下面的兜底通道
    }
  }

  const fallback = deps.fallback ?? defaultFallback;
  try {
    return Boolean(fallback(text));
  } catch {
    return false;
  }
}
