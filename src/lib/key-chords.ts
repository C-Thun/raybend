/**
 * 快捷键「键位串」的解析 / 格式化 / 匹配（`specs/M2-W3.md` §2.2）。
 *
 * 为什么单独一份、还带测试：这是整个快捷键体系里**最容易写错**的一块 ——
 * 修饰键平台差异、`+` 这个键本身、大小写、命名键（Esc / Enter / 方向键）、
 * 以及「按了 Shift 之后 `event.key` 会变成别的字符」这几件事，错一个就是
 * 「改完键不生效」或「两条命令一起触发」。
 *
 * ## 两种写法
 *
 * | 写法 | 含义 |
 * | --- | --- |
 * | `Mod+K` | **平台主修饰键**：Windows / Linux = `Ctrl`，macOS = `Cmd` |
 * | `Ctrl+Shift+K` | 显式写出修饰键（跨平台时按字面理解） |
 *
 * ## 三个约定
 *
 * 1. **字母键一律小写**（`P` 与 `p` 是同一条键；匹配时也忽略大小写）；
 * 2. **`+` 与 `=` 是同一个键**：键盘上「+」要按 Shift，但用户心里它就是那个键 ——
 *    所以内部统一存成 `=`，且**匹配 `=` 时忽略 Shift**；
 * 3. **命名键规范化**：`Esc`→`escape`、`Del`→`delete`、`Return`→`enter`、
 *    `↑`→`arrowup`…… 一套别名表，显示时再换回好看的写法。
 */

/** 平台（只影响 `Mod` 与格式化） */
export type ChordPlatform = "win" | "mac";

export interface Chord {
  /** 规范化后的键名（小写字母 / 数字 / `escape` / `enter` / `arrowup` …） */
  key: string;
  /** 平台主修饰键（Win/Linux = Ctrl，macOS = Cmd） */
  mod: boolean;
  /** 显式 Ctrl（macOS 上 `Mod` 是 Cmd，两者不同） */
  ctrl: boolean;
  /** 显式 Cmd/Win 键 */
  meta: boolean;
  shift: boolean;
  alt: boolean;
}

/** 解析结果：非法输入要能说清「哪里不对」（设置界面直接显示这句话） */
export type ChordParse =
  | { ok: true; chord: Chord; text: string }
  | { ok: false; reason: "empty" | "modifier-only" | "unknown-key"; detail: string };

/** 命名键别名 → 规范化键名 */
const KEY_ALIASES: Record<string, string> = {
  esc: "escape",
  escape: "escape",
  return: "enter",
  enter: "enter",
  del: "delete",
  delete: "delete",
  ins: "insert",
  insert: "insert",
  space: "space",
  spacebar: "space",
  tab: "tab",
  backspace: "backspace",
  bs: "backspace",
  home: "home",
  end: "end",
  pageup: "pageup",
  pgup: "pageup",
  pagedown: "pagedown",
  pgdn: "pagedown",
  up: "arrowup",
  down: "arrowdown",
  left: "arrowleft",
  right: "arrowright",
  arrowup: "arrowup",
  arrowdown: "arrowdown",
  arrowleft: "arrowleft",
  arrowright: "arrowright",
  // 显示时想用箭头也行
  "↑": "arrowup",
  "↓": "arrowdown",
  "←": "arrowleft",
  "→": "arrowright",
  "＋": "+",
};

/** 规范化一个键名（`null` = 认不出来） */
export function normalizeKey(raw: string): string | null {
  const value = raw.trim();
  if (value === "") return null;
  // `+` 与 `=` 是同一个键（见文件头的约定 2）
  if (value === "+" || value === "=") return "=";
  const alias = KEY_ALIASES[value.toLowerCase()];
  if (alias !== undefined) return alias === "+" ? "=" : alias;
  if (/^[a-z]$/.test(value.toLowerCase())) return value.toLowerCase();
  if (/^[0-9]$/.test(value)) return value;
  if (/^f([1-9]|1[0-2])$/i.test(value)) return value.toLowerCase();
  // 其它单字符键（`,` `.` `/` `;` `'` `[` `]` `\` `` ` `` `-`）
  if (value.length === 1) return value;
  return null;
}

/** 修饰键别名 → 内部标记 */
function modifierOf(token: string): keyof Pick<Chord, "mod" | "ctrl" | "meta" | "shift" | "alt"> | null {
  switch (token.toLowerCase()) {
    case "mod":
    case "cmdorctrl":
      return "mod";
    case "ctrl":
    case "control":
      return "ctrl";
    case "cmd":
    case "command":
    case "meta":
    case "super":
    case "win":
      return "meta";
    case "shift":
      return "shift";
    case "alt":
    case "option":
      return "alt";
    default:
      return null;
  }
}

/**
 * 解析键位串。**按字面解析，不猜平台** —— `Mod` 的含义留给匹配/格式化那一步。
 */
export function parseChord(text: string): ChordParse {
  const raw = text.trim();
  if (raw === "") return { ok: false, reason: "empty", detail: text };
  /*
   * 拆分：`+` 是分隔符，但它自己也是一个键 ——
   * 所以「末尾为空的一段」意味着最后那个键就是 `+`（例：`Mod++` → ["Mod", "", ""]）。
   */
  const parts = raw.split("+");
  if (parts.length > 1 && parts[parts.length - 1] === "" && parts[parts.length - 2] === "") {
    parts.splice(parts.length - 2, 2, "+");
  }
  const chord: Chord = { key: "", mod: false, ctrl: false, meta: false, shift: false, alt: false };
  let keySeen: string | null = null;
  for (const part of parts) {
    if (part === "") continue;
    const modifier = modifierOf(part);
    if (modifier !== null) {
      chord[modifier] = true;
      continue;
    }
    const key = normalizeKey(part);
    if (key === null) return { ok: false, reason: "unknown-key", detail: part };
    // 已经有一个键了：说明中间出现了 `+` 这种歧义写法（`Ctrl+K+P`）—— 不猜
    if (keySeen !== null) return { ok: false, reason: "unknown-key", detail: part };
    keySeen = key;
  }
  if (keySeen === null) return { ok: false, reason: "modifier-only", detail: raw };
  chord.key = keySeen;
  return { ok: true, chord, text: formatChord(chord) };
}

/** 显示用的键名（与 `normalizeKey` 反向） */
const KEY_LABELS: Record<string, string> = {
  escape: "Esc",
  enter: "Enter",
  delete: "Del",
  insert: "Ins",
  space: "Space",
  tab: "Tab",
  backspace: "Backspace",
  pageup: "PgUp",
  pagedown: "PgDn",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  "=": "+",
};

/** 键名 → 显示文案 */
export function keyLabel(key: string): string {
  const label = KEY_LABELS[key];
  if (label !== undefined) return label;
  /*
   * F 键统一**大写**（`f11` → `F11`）。
   * 不这样写的话，`key.length === 1` 那条会把它原样透出去 —— 界面上会显示小写 `f11`，
   * 与其它键位（`P` / `Tab` / `Ctrl+K`）的观感不一致。
   */
  if (/^f([1-9]|1[0-2])$/.test(key)) return key.toUpperCase();
  return key.length === 1 ? key.toUpperCase() : key;
}

/** 解析出平台（浏览器 / WebView2 都能给 userAgent） */
export function detectPlatform(): ChordPlatform {
  if (typeof navigator === "undefined") return "win";
  return /mac|iphone|ipad|ipod/i.test(navigator.userAgent) ? "mac" : "win";
}

/**
 * 格式化成人看的键位串（**Windows 习惯优先**，这是本项目的目标平台）：
 * `Mod+K` → `Ctrl+K`；macOS 上 `Mod+K` → `⌘K`。
 */
export function formatChord(chord: Chord, platform: ChordPlatform = detectPlatform()): string {
  const parts: string[] = [];
  if (chord.mod) parts.push(platform === "mac" ? "⌘" : "Ctrl");
  if (chord.ctrl && !chord.mod) parts.push("Ctrl");
  if (chord.meta && !chord.mod) parts.push(platform === "mac" ? "⌘" : "Win");
  if (chord.alt) parts.push(platform === "mac" ? "⌥" : "Alt");
  if (chord.shift) parts.push("Shift");
  parts.push(keyLabel(chord.key));
  return parts.join("+");
}

/** 键盘事件里我们关心的那几项（便于测试注入） */
export interface ChordEventLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

/** 事件 → 规范化键名（`+`/`=` 归一，字母小写） */
export function eventKey(event: ChordEventLike): string | null {
  return normalizeKey(event.key);
}

/**
 * 事件 + 键位是否匹配。
 *
 * 两条特例（都在文件头写了理由）：
 * * 键是 `=` 时**忽略 Shift** —— 键盘上打出 `+` 必须按 Shift，用户不认为那是「Shift+」；
 * * `Mod` 按平台展开成 Ctrl（Win/Linux）或 Cmd（macOS）。
 */
export function chordMatches(
  chord: Chord,
  event: ChordEventLike,
  platform: ChordPlatform = detectPlatform(),
): boolean {
  const key = eventKey(event);
  if (key === null || key !== chord.key) return false;
  const wantCtrl = chord.ctrl || (chord.mod && platform !== "mac");
  const wantMeta = chord.meta || (chord.mod && platform === "mac");
  if ((event.ctrlKey === true) !== wantCtrl) return false;
  if ((event.metaKey === true) !== wantMeta) return false;
  if ((event.altKey === true) !== chord.alt) return false;
  if (chord.key === "=") return true;
  return (event.shiftKey === true) === chord.shift;
}

/** 事件 → 键位串（设置界面「按一下捕获」用；认不出来返回 `null`） */
export function chordFromEvent(
  event: ChordEventLike,
  platform: ChordPlatform = detectPlatform(),
): string | null {
  const key = eventKey(event);
  if (key === null) return null;
  // 只有修饰键按下：还没按到真正的键，不产出
  if (["shift", "control", "alt", "meta", "os"].includes(key)) return null;
  const chord: Chord = {
    key,
    // 事件里分不出「Mod」与「显式 Ctrl」：Win 上就是 Ctrl，Mac 上把 Cmd 记成 Mod
    mod: platform === "mac" ? event.metaKey === true : event.ctrlKey === true,
    ctrl: platform === "mac" ? event.ctrlKey === true : false,
    meta: false,
    shift: event.shiftKey === true,
    alt: event.altKey === true,
  };
  if (chord.key === "=") chord.shift = false;
  return formatChord(chord, platform);
}

/**
 * 系统 / 宿主保留的键位（**绑不了**，绑了也抢不过来）。
 *
 * `Win+*` 这类拿不到事件（Windows 键在 WebView2 里不上报），所以只在文档里提醒，
 * 不进这张表；表里是**能表达但必须拒绝**的几条。
 */
export const RESERVED_CHORDS: readonly string[] = ["Alt+F4", "Ctrl+Alt+Delete", "Ctrl+Shift+Esc"];

/** 这条键位串是不是系统保留（比较用规范化后的形式） */
export function isReservedChord(text: string): boolean {
  const parsed = parseChord(text);
  if (!parsed.ok) return false;
  return RESERVED_CHORDS.some((reserved) => {
    const other = parseChord(reserved);
    return other.ok && sameChord(parsed.chord, other.chord);
  });
}

/** 两条键位是不是同一条（忽略书写差异） */
export function sameChord(a: Chord, b: Chord): boolean {
  return (
    a.key === b.key &&
    a.mod === b.mod &&
    a.ctrl === b.ctrl &&
    a.meta === b.meta &&
    a.shift === b.shift &&
    a.alt === b.alt
  );
}
