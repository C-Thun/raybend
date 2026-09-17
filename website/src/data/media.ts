import type { Locale } from '../i18n/index.ts';

/**
 * 站点图片素材清单。
 *
 * **当前状态：图片还没到位** —— 缺界面截图与少量美化图（用户会自己截 / 用 AI 生成）。
 * 所以每个条目先只描述「该放什么」：组件据此渲染**版式正确的占位块**，
 * 图片到位后把 `src` 填上，占位自动消失、版式不变。
 *
 * 提示词原文与取景说明整理在 `website/ASSETS.md`（给用户看的清单）。
 */
export type MediaSource = 'screenshot' | 'art' | 'provided';

export interface MediaEntry {
  /** 图片来源：截图（用户截）/ AI 生图 / 已有素材 */
  source: MediaSource;
  /** 建议尺寸（同时决定占位块的宽高比） */
  width: number;
  height: number;
  /** 图片路径（`public/` 下的文件名）；按语言分版本的用对象。留空 = 还没图 */
  src?: string | Record<Locale, string>;
  /** 页面占位块给人看的一句话（开发期可见） */
  hint: string;
  /** AI 生图提示词（英文，图片模型更好用）；截图类写取景要点 */
  prompt: string;
}

export const media = {
  /** hero 右侧：应用窗口截图（浏览工作区三列） */
  heroApp: {
    source: 'screenshot',
    width: 1600,
    height: 1000,
    hint: 'hero 主视觉：浏览工作区截图（1600×1000）',
    prompt:
      'Screenshot to take in the app: browse workspace at a comfortable window size — left: library + folder tree; middle: photo grid with real photos; right: metadata panel with histogram. Light theme.',
  },

  /** 特性 A：导入面板 */
  featureImport: {
    source: 'screenshot',
    width: 1440,
    height: 900,
    hint: '导入工作区截图（1440×900）',
    prompt:
      'Screenshot to take in the app: import workspace — source tree on the left, pending photo list with checkboxes, destination library and naming template on the right.',
  },

  /** 特性 B：浏览网格 / 按时间分组 */
  featureBrowse: {
    source: 'screenshot',
    width: 1440,
    height: 900,
    hint: '浏览网格截图，含按时间分组（1440×900）',
    prompt:
      'Screenshot to take in the app: browse grid grouped by time, with rating/color-label badges visible on the tiles and a few photos selected.',
  },

  /** 特性 C：命令面板（整宽带） */
  featureKeyboard: {
    source: 'screenshot',
    width: 1440,
    height: 900,
    hint: '命令面板截图（1440×900）',
    prompt:
      'Screenshot to take in the app: command palette open over the browse grid, showing a few matched commands with their keyboard shortcuts.',
  },

  /** 装饰：斜向胶片带（中段整宽带的背景） */
  filmStrip: {
    source: 'art',
    width: 1600,
    height: 600,
    hint: '胶片带装饰图（透明底，1600×600）',
    prompt:
      'Flat vector illustration, transparent background: a long strip of 35mm film curving diagonally, drawn with clean 3px strokes, teal (#52C6AB) with amber (#F0B033) accents, no text, no logos, minimal, geometric, generous negative space.',
  },

  /** 装饰：下载区的柔光星点（纯美化） */
  downloadGlow: {
    source: 'art',
    width: 1200,
    height: 800,
    hint: '下载区柔光装饰（透明底，1200×800）',
    prompt:
      'Flat vector illustration, transparent background: soft abstract light rays and a few star sparkles, teal (#52C6AB) and amber (#F0B033) on a warm cream base, no text, very subtle, low contrast, wide empty center.',
  },

  /** 已有素材：闪屏艺术图（中英两版），拿来做下载区/中段的装饰 */
  splash: {
    source: 'provided',
    width: 1086,
    height: 814,
    src: { zh: 'splash_v1-cn.webp', en: 'splash_v1-en.webp' },
    hint: '已有素材：splash_v1（中英两版）',
    prompt: '已有素材，无需生成。',
  },
} satisfies Record<string, MediaEntry>;

export type MediaId = keyof typeof media;

/** 取某个素材在当前语言下的 URL（还没有图时返回 undefined） */
export function mediaSrc(id: MediaId, locale: Locale): string | undefined {
  const entry: MediaEntry = media[id];
  if (!entry.src) return undefined;
  return typeof entry.src === 'string' ? entry.src : entry.src[locale];
}

/** 占位块的宽高比（`width / height`） */
export function mediaAspect(id: MediaId): number {
  const entry: MediaEntry = media[id];
  return entry.width / entry.height;
}
