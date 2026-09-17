import { createMemo, createSignal } from 'solid-js';
import { en } from './en.ts';
import { zh, type Dictionary } from './zh.ts';

export type Locale = 'zh' | 'en';

export const LOCALES: readonly Locale[] = ['zh', 'en'];

/** 语言 → `<html lang>` 值 */
const HTML_LANG = { zh: 'zh-CN', en: 'en' } satisfies Record<Locale, string>;

/** 手动选择存在这里；下次访问直接生效 */
const STORAGE_KEY = 'raybend.locale';

function isLocale(value: unknown): value is Locale {
  return value === 'zh' || value === 'en';
}

/** 浏览器给的候选语言（`navigator.languages` 优先，退回 `navigator.language`） */
function browserLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') return [];
  const languages = navigator.languages;
  if (Array.isArray(languages) && languages.length > 0) return languages;
  return navigator.language ? [navigator.language] : [];
}

/**
 * 判定首访语言：按浏览器语言顺序取第一个认识的 —— 中文 → `zh`，英文/其它 → `en`。
 * 一个都不认识时回落 `en`；完全拿不到语言信息（非浏览器环境）时回落 `zh`（主场默认，
 * 与静态 head 的默认语言一致）。
 */
export function detectLocale(candidates: readonly string[] = browserLanguages()): Locale {
  if (candidates.length === 0) return 'zh';
  for (const candidate of candidates) {
    const tag = candidate.toLowerCase();
    if (tag.startsWith('zh')) return 'zh';
    if (tag.startsWith('en')) return 'en';
  }
  return 'en';
}

function readStoredLocale(): Locale | undefined {
  if (typeof localStorage === 'undefined') return undefined;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return isLocale(stored) ? stored : undefined;
  } catch {
    // 隐私模式等场景下 localStorage 会抛异常 —— 语言不该因此挂掉
    return undefined;
  }
}

function writeStoredLocale(locale: Locale): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // 同上：存不下就算了，本次会话仍然按选择渲染
  }
}

const [locale, setLocaleSignal] = createSignal<Locale>(readStoredLocale() ?? detectLocale());

/** 当前语言（响应式） */
export { locale };

/** 当前语言的文案表（响应式；`zh` 与 `en` 结构由类型保证一致） */
export const messages = createMemo<Dictionary>(() => (locale() === 'zh' ? zh : en));

/**
 * 切换语言。
 *
 * 副作用只有两件：写 `localStorage`（`persist: false` 可跳过）与同步 `<html lang>`
 * —— 标题目录等交给 `@solidjs/meta`，在组件里随语言一起更新，避免两处数据源。
 */
export function setLocale(next: Locale, options: { persist?: boolean } = {}): void {
  if (next === locale()) return;
  if (options.persist !== false) writeStoredLocale(next);

  if (typeof document !== 'undefined') {
    document.documentElement.lang = HTML_LANG[next];
  }

  setLocaleSignal(next);
}

/** 供 `App` 在挂载时同步一次（首访时 `<html lang>` 还是外壳里的默认值） */
export function syncDocumentLanguage(): void {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = HTML_LANG[locale()];
}

/** 另一种语言（双语切换控件用） */
export function nextLocale(): Locale {
  return locale() === 'zh' ? 'en' : 'zh';
}
