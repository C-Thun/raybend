/**
 * `shortpath` —— 路径缩写（DESIGN.md §12.3）。
 *
 * 形态：
 *   Windows   盘符 + 中间各级目录首字母 + 最后一级目录全名
 *             D:\Photos\2024\Vacation\Japan\Kyoto  →  D:\P\2\V\J\Kyoto
 *   POSIX     第一级与最后一级写全，中间取首字母
 *             /home/andares/Pictures/Wallpapers   →  /home/a/P/Wallpapers
 *
 * **两层算法**（这是本文件存在的核心理由）：
 *   ① 只缩中间各级（首尾按平台规则保留）
 *   ② 仍超长 → **放弃 ① 的结果整体重缩**（末级也参与缩写）
 *   ③ 还超长 → 中间首字母段整体塌成 `...`，保留两头
 *
 * 为什么必须整体重缩（不能直接在 ① 的结果上截断）：
 *   若首级或末级本身很长，① 里生成的 `...` 会在某一侧「戳出来」，
 *   产生完全错误的截断 —— 例如把 `D:\P\...\VeryLongName` 直接从中间砍一刀，
 *   结果会是 `D:\P\...\VeryL` 这种既不像缩写也不像截断的东西。
 */

/** 路径分隔符 */
export type Separator = "\\" | "/";

export interface ShortPathOptions {
 /**
  * 长度上限（**字符数**，含分隔符）。
  * 省略时不缩略长度，只做首字母缩写。
  */
 maxLength?: number;
 /** 强制指定分隔符；省略时按路径内容自动判断 */
 separator?: Separator;
}

interface Parsed {
 /** 前缀：Windows 是 `D:` 或 `\\server\share`，POSIX 是 ``（空） */
 prefix: string;
 /**
  * 是否为绝对路径。
  * 它决定「前缀为空时要不要补分隔符」—— 绝对路径 `/a/b` 要，
  * 相对路径 `a/b` 不要。少了这个标识就会给相对路径凭空加上前导 `/`。
  */
 absolute: boolean;
 /** 各级目录名（已去掉空段） */
 segments: string[];
 separator: Separator;
 /** 原路径是否以分隔符结尾 */
 trailingSeparator: boolean;
}

/** 判断分隔符：含反斜杠或形如 `X:` 的走 Windows 规则 */
function detectSeparator(input: string): Separator {
 if (input.includes("\\")) return "\\";
 if (/^[a-zA-Z]:/.test(input)) return "\\";
 return "/";
}

function parse(input: string, forced?: Separator): Parsed {
 const separator = forced ?? detectSeparator(input);
 const isWin = separator === "\\";

 // UNC：\\server\share\a\b —— prefix 取 `\\server\share`
 if (isWin && input.startsWith("\\\\")) {
  const rest = input.slice(2);
  const parts = rest.split(/[\\/]+/).filter(Boolean);
  const prefix =
   parts.length >= 2
    ? `\\\\${parts[0]}\\${parts[1]}`
    : `\\\\${parts.join("\\")}`;
  return {
   prefix,
   absolute: true,
   segments: parts.slice(2),
   separator,
   trailingSeparator: /[\\/]$/.test(input),
  };
 }

 // Windows 盘符：D:\a\b
 if (isWin && /^[a-zA-Z]:/.test(input)) {
  const prefix = input.slice(0, 2);
  const rest = input.slice(2).replace(/^[\\/]+/, "");
  return {
   prefix,
   absolute: true,
   segments: rest.split(/[\\/]+/).filter(Boolean),
   separator,
   trailingSeparator: /[\\/]$/.test(input),
  };
 }

 // POSIX：/a/b 或相对路径 a/b
 const rootMatch = /^\/+/.exec(input);
 const prefix = "";
 const rest = input.slice(rootMatch ? rootMatch[0].length : 0);
 return {
  prefix,
  absolute: Boolean(rootMatch) || /^[\\/]/.test(input),
  segments: rest.split(/[\\/]+/).filter(Boolean),
  separator,
  trailingSeparator: /[\\/]$/.test(input),
 };
}

/** 取首字符（按码点，避免把 emoji/代理对切成半个） */
function firstChar(segment: string): string {
 const chars = Array.from(segment);
 return chars.length > 0 ? chars[0] : "";
}

function isWindows(p: Parsed): boolean {
 return p.separator === "\\";
}

/**
 * 路径的「根」：前缀 + 必需的尾随分隔符。
 * 前缀为空时，只有绝对路径才产生分隔符。
 */
function rootOf(p: Parsed): string {
 if (p.prefix) return p.prefix + p.separator;
 return p.absolute ? p.separator : "";
}

/** 把各级首字母拼成缩写段 */
function initials(segments: string[], p: Parsed): string {
 return segments.map(firstChar).join(p.separator);
}

/**
 * ① 只缩中间：Windows 保留末级全名（首级也缩），POSIX 保留首末两级全名。
 */
function abbreviateMiddle(parsed: Parsed): string {
 const { segments, separator } = parsed;
 const root = rootOf(parsed);
 if (segments.length === 0)
  return parsed.prefix || (parsed.absolute ? separator : "");
 if (segments.length === 1) return root + segments[0];

 if (isWindows(parsed)) {
  // 除末级外全部取首字母
  const head = initials(segments.slice(0, -1), parsed);
  return `${root}${head}${separator}${segments[segments.length - 1]}`;
 }

 // POSIX：首、末写全，中间取首字母
 if (segments.length === 2) {
  return `${root}${segments[0]}${separator}${segments[1]}`;
 }
 const middle = initials(segments.slice(1, -1), parsed);
 return `${root}${segments[0]}${separator}${middle}${separator}${segments[segments.length - 1]}`;
}

/**
 * ② 整体重缩：末级也参与缩写。
 */
function abbreviateAll(parsed: Parsed): string {
 const { segments, separator } = parsed;
 const root = rootOf(parsed);
 if (segments.length === 0)
  return parsed.prefix || (parsed.absolute ? separator : "");

 if (isWindows(parsed)) {
  return root + initials(segments, parsed);
 }

 // POSIX 仍保留首级全名，末级取首字母
 const first = segments[0];
 const restInitials = initials(segments.slice(1, -1), parsed);
 const lastInitial = firstChar(segments[segments.length - 1]);
 const body = [restInitials, lastInitial].filter(Boolean).join(separator);
 return body ? `${root}${first}${separator}${body}` : `${root}${first}`;
}

/**
 * ③ 中间塌成 `...`，保留两头。
 * 优先保住「末级目录名」这个识别度最高的信息。
 */
function collapseWithEllipsis(parsed: Parsed, maxLength: number): string {
 const { segments, separator } = parsed;
 const root = rootOf(parsed);
 const ELLIPSIS = "...";

 // 极小上限：只能给一个省略号。不返回空串 —— 界面上一段空白比一个 `…` 更让人困惑。
 if (maxLength <= 1) return "\u2026";
 if (segments.length === 0)
  return parsed.prefix || (parsed.absolute ? separator : "");

 // 首段：Windows 用盘符，POSIX 用第一级
 const headToken = isWindows(parsed) ? parsed.prefix : segments[0];
 const tailToken = segments[segments.length - 1];
 const headPrefix = isWindows(parsed)
  ? root
  : `${root}${headToken}${separator}`;

 const build = (head: string, tail: string) =>
  `${head}${ELLIPSIS}${separator}${tail}`;

 // 先试「两头都完整」
 let out = build(headPrefix || (parsed.absolute ? separator : ""), tailToken);
 if (out.length <= maxLength) return out;

 // 再试「砍掉头，只留 ...\末级」
 out = `${ELLIPSIS}${separator}${tailToken}`;
 if (out.length <= maxLength) return out;

 // 最后：末级名本身也放不下 → 保留其开头，末尾补一个 `…`。
 // 这里必须**严格**不超过 maxLength（下界由上面的 maxLength <= 1 兵底）。
 const budget = maxLength - 1; // 留 1 位给结尾的 …
 const clipped = Array.from(tailToken).slice(0, budget).join("");
 return `${clipped}\u2026`;
}

/**
 * 把完整路径缩成 `shortpath` 形式。
 *
 * @example
 * shortPath("D:\\Photos\\2024\\Vacation\\Japan\\Kyoto")
 * // → "D:\\P\\2\\V\\J\\Kyoto"
 *
 * shortPath("D:\\Photos\\2024\\Vacation\\Japan\\Kyoto", { maxLength: 12 })
 * // → 触发第 ② 层，末级也参与缩写
 */
export function shortPath(
 fullPath: string,
 options: ShortPathOptions = {},
): string {
 if (typeof fullPath !== "string" || fullPath.length === 0) return "";

 const parsed = parse(fullPath, options.separator);

 // 没有可缩的层级
 if (parsed.segments.length === 0) return parsed.prefix || fullPath;

 const abbreviated = abbreviateMiddle(parsed);

 const { maxLength } = options;
 if (maxLength === undefined || maxLength <= 0) return abbreviated;
 if (abbreviated.length <= maxLength) return abbreviated;

 // ② 放弃 ① 的结果，整体重缩
 const reAbbreviated = abbreviateAll(parsed);
 if (reAbbreviated.length <= maxLength) return reAbbreviated;

 // ③ 中间塌成 `...`
 return collapseWithEllipsis(parsed, maxLength);
}
