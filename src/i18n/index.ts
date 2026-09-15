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
import { enUS } from "./en-US";
import { zhCN } from "./zh-CN";

/** 以中文包为准推导 key 联合类型；新增文案只改 zh-CN.ts 即可 */
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

export function setLocale(next: LocaleId): void {
 setLocaleSignal(next);
 // 让浏览器/WebView 知道当前语言（影响断行规则、字体回退与无障碍朗读）
 if (typeof document !== "undefined") {
  document.documentElement.lang = next;
 }
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
