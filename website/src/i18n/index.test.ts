import { flush } from 'solid-js';
import { afterEach, describe, expect, it } from 'vitest';
import { detectLocale, locale, messages, nextLocale, setLocale } from './index.ts';

/**
 * 注意 **Solid 2 的信号写入是批量（batched）的** —— `setLocale()` 之后要 `await flush()`
 * 才能读到新值（写盘与 `<html lang>` 是同步的，不受影响）。见 `website/AGENTS.md`。
 */
afterEach(async () => {
  localStorage.clear();
  setLocale('zh', { persist: false });
  await flush();
});

describe('detectLocale', () => {
  it('中文优先：只要候选里有中文就用中文', () => {
    expect(detectLocale(['zh-CN', 'en-US'])).toBe('zh');
    expect(detectLocale(['zh-Hant-TW'])).toBe('zh');
  });

  it('英文用英文', () => {
    expect(detectLocale(['en-US'])).toBe('en');
    expect(detectLocale(['en-GB', 'zh-CN'])).toBe('en');
  });

  it('其它语言回落英文（不认识就按国际默认）', () => {
    expect(detectLocale(['ja-JP'])).toBe('en');
    expect(detectLocale(['fr', 'de'])).toBe('en');
  });

  it('完全拿不到语言信息时回落中文（与静态 head 的默认语言一致）', () => {
    expect(detectLocale([])).toBe('zh');
  });
});

describe('setLocale', () => {
  it('切换语言会写 localStorage 并同步 <html lang>', async () => {
    setLocale('en');
    await flush();

    expect(locale()).toBe('en');
    expect(localStorage.getItem('raybend.locale')).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('persist: false 时只切换、不落盘（测试与预览用）', async () => {
    setLocale('en', { persist: false });
    await flush();

    expect(locale()).toBe('en');
    expect(localStorage.getItem('raybend.locale')).toBeNull();
  });

  it('同一个语言重复调用不重复写盘（保留已有的存储值）', async () => {
    setLocale('en');
    await flush();
    localStorage.setItem('raybend.locale', 'marker');

    setLocale('en');
    await flush();

    expect(localStorage.getItem('raybend.locale')).toBe('marker');
  });

  it('文案跟着语言切换，且两种语言的键完全对齐', async () => {
    setLocale('zh', { persist: false });
    await flush();
    const zh = messages();
    expect(zh.hero.earlyBadge).toBe('早期开发中');

    setLocale('en', { persist: false });
    await flush();
    const en = messages();
    expect(en.hero.earlyBadge).toBe('Early development');

    // `en: Dictionary` 已经保证编译期键一致，这里再核一遍运行时形状
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort());
    expect(Object.keys(en.workflows.items).sort()).toEqual(
      Object.keys(zh.workflows.items).sort(),
    );
    expect(Object.keys(en.features.items).sort()).toEqual(
      Object.keys(zh.features.items).sort(),
    );
  });

  it('nextLocale 在两种语言间来回', async () => {
    setLocale('zh', { persist: false });
    await flush();
    expect(nextLocale()).toBe('en');

    setLocale('en', { persist: false });
    await flush();
    expect(nextLocale()).toBe('zh');
  });
});
