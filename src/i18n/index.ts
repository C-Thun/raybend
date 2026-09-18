/**
 * 轻量 i18n 运行时。
 *
 * 为什么不引第三方库（AGENTS.md §2.9：新增框架/大型依赖前先讨论）：
 *   - DESIGN.md §11 的 key 集规模很小（当前 ~60 条），没有复数、性别、日期格式化等需求
 *   - 需求只有两条：**零硬编码** + **中英切换**
 *   - 语言包类型由 TS 静态保证完整性（见 en-US.ts 的 Record 声明），比运行时检查更早发现问题
 *
 * 若将来需要复数规则、ICU MessageFormat 或按需加载，再评估引入库。
 */

import { createSignal } from "solid-js";
import { enUS } from "./en-US.ts";
import { zhCN } from "./zh-CN.ts";

/**
 * 以中文包为准推导 key 联合类型；新增文案只改 zh-CN.ts 即可。
 *
 * ⚠️ 加了新键之后若类型检查器仍报「键不存在」，那是**陈旧快照**误报（不是真错）：
 * 判据与验证配方见 `ASSISTANCE.md` §二第 3 条（`MessageKey[]` 临时证明文件 + tsc 退出码）。
 *
 * （2026-09-19：本会话内同一模式出现 5 次 —— 加完键先跑一次 tsc，再改一次本文件，
 * 检查器的缓存就会跟着刷新；这一步比挨个标误报省事。）
 */
export type MessageKey = keyof typeof zhCN;

export const LOCALES = {
 "zh-CN": zhCN,
 "en-US": enUS,
} as const;

export type LocaleId = keyof typeof LOCALES;

export const LOCALE_IDS = Object.keys(LOCALES) as LocaleId[];

/** 默认中文（项目工作语言） */
const [locale, setLocaleSignal] = createSignal<LocaleId>("zh-CN");
export { locale };

/**
 * 语言选择的存储键（`localStorage`）。
 *
 * 为什么走 localStorage 而不是 `app.db`：与主题/密度同一类 —— **设备级偏好**
 * （换台机器重新选一次是合理的），而且要在首次渲染前就能拿到，走 IPC 进库会闪一帧。
 */
export const LOCALE_STORAGE_KEY = "raybend.locale";

/** 非法值一律回落到默认 —— 存储里的垃圾不能让界面起不来 */
export function normalizeLocale(value: unknown): LocaleId {
 return value === "en-US" || value === "zh-CN" ? value : "zh-CN";
}

export function setLocale(next: LocaleId): void {
 setLocaleSignal(next);
 // 让浏览器/WebView 知道当前语言（影响断行规则、字体回退与无障碍朗读）
 if (typeof document !== "undefined") {
  document.documentElement.lang = next;
 }
 try {
  globalThis.localStorage?.setItem(LOCALE_STORAGE_KEY, next);
 } catch {
  // 隐私模式 / 禁用存储：不记就不记，不影响本次使用
 }
}

/**
 * 启动时读回上次选的语言。
 *
 * 必须在首次渲染前调（`src/index.tsx`）—— 否则会先渲一遍中文再跳成英文。
 */
export function hydrateLocale(
 storage: { getItem: (key: string) => string | null } | undefined = globalThis.localStorage,
): LocaleId {
 let stored: string | null = null;
 try {
  stored = storage?.getItem(LOCALE_STORAGE_KEY) ?? null;
 } catch {
  stored = null;
 }
 const next = normalizeLocale(stored);
 setLocale(next);
 return next;
}

/** 另一种语言（目前只有中/英两种，所以就是「翻转」） */
export function nextLocale(current: LocaleId): LocaleId {
 return current === "zh-CN" ? "en-US" : "zh-CN";
}

/**
 * 语言**自己的名字**（endonym）：中文页包说 `中文`、英文包说 `English`。
 *
 * ⚠️ 帮帮助菜单里的那条切换**必须**从**目标语言的包**里取这一条，
 * 而不是用 `t()` 取当前语言包 —— 菜单上要写的是「切过去会变成什么」：
 * 中文界面显示 `English`、英文界面显示 `中文`。用 `t()` 会让两种界面都写自己的名字，
 * 那条菜单就失去了意义（用户不知道点了会变成什么）。
 */
export function localeLabel(id: LocaleId): string {
 return LOCALES[id]["locale.name"];
}

export type TParams = Record<string, string | number>;

/**
 * 取文案。参数用 `{name}` 占位，例如
 * `t("common.selected_summary", { dirs: 3, photos: 1248 })`
 *
 * 未翻译的 key 会**原样返回 key 名**——这是有意的：界面上出现 `source.recent`
 * 这种字样时一眼就能看出漏译，比返回空串更容易发现。
 */
export function t(key: MessageKey, params?: TParams): string {
 const raw: string = LOCALES[locale()][key] ?? key;
 if (!params) return raw;
 return raw.replace(/\{(\w+)\}/g, (match, name: string) =>
  name in params ? String(params[name]) : match,
 );
}

/**
 * 「超时」那句话的拼装（给 `lib/timeout.ts` 的调用方用）。
 *
 * 为什么拼装在**这里**而不是 `lib/timeout.ts`：那个文件在纯逻辑层，
 * 而分层检查器不允许 `lib` import `i18n`（`ARCHITECTURE.md` §1）——
 * 所以句子由应用层拼好再递给它，`TimeoutError.message` 就是成品。
 *
 * `whatKey` 是「哪个动作卡住了」（如 `import.timeout.start` =
 * 「启动导入」），它自己也是一条文案，所以也得走语言包。
 */
export function timeoutMessage(whatKey: MessageKey, timeoutMs: number): string {
 return t("common.timeout", {
  what: t(whatKey),
  seconds: Math.round(timeoutMs / 1000),
 });
}
